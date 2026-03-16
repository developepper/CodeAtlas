//! HTTP route handlers for the persistent local service.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use query_engine::StoreQueryService;
use server_mcp::ToolRegistry;

use crate::state::SharedState;
use crate::ServiceError;

/// Maps a [`ServiceError`] to an HTTP response with appropriate status code
/// and a JSON error body with a consistent `{"error": "..."}` shape.
fn error_response(err: ServiceError) -> (StatusCode, Json<Value>) {
    match &err {
        ServiceError::Query(query_engine::QueryError::NotFound { .. }) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": err.to_string() })),
        ),
        ServiceError::Query(query_engine::QueryError::EmptyQuery) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": err.to_string() })),
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        ),
    }
}

// ── Health ─────────────────────────────────────────────────────────────

pub fn health_routes() -> Router<SharedState> {
    Router::new().route("/health", get(health))
}

async fn health() -> StatusCode {
    StatusCode::OK
}

// ── Status ─────────────────────────────────────────────────────────────

pub fn status_routes() -> Router<SharedState> {
    Router::new().route("/status", get(status))
}

async fn status(State(state): State<SharedState>) -> impl IntoResponse {
    let uptime_secs = state.started_at.elapsed().as_secs();

    let db_result = state.with_db(|db| {
        let count = db.repos().list_ids()?.len();
        let version = db.schema_version()?;
        Ok::<_, store::StoreError>((count, version))
    });

    let index_version = core_model::schema_version::current_index_schema_version().to_string();

    match db_result {
        Ok((repo_count, schema_version)) => (
            StatusCode::OK,
            Json(json!({
                "status": "ok",
                "version": env!("CARGO_PKG_VERSION"),
                "index_version": index_version,
                "schema_version": schema_version,
                "uptime_secs": uptime_secs,
                "data_root": state.config.data_root.to_string_lossy(),
                "repo_count": repo_count,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "status": "error",
                "error": e.to_string(),
                "version": env!("CARGO_PKG_VERSION"),
                "uptime_secs": uptime_secs,
                "data_root": state.config.data_root.to_string_lossy(),
            })),
        )
            .into_response(),
    }
}

// ── Repo catalog ───────────────────────────────────────────────────────

pub fn repo_routes() -> Router<SharedState> {
    Router::new()
        .route("/repos", get(list_repos))
        .route("/repos/{repo_id}", get(get_repo).delete(remove_repo))
}

async fn list_repos(State(state): State<SharedState>) -> impl IntoResponse {
    let result = state.with_db(|db| db.repos().list_all());

    match result {
        Ok(repos) => {
            let items: Vec<Value> = repos
                .iter()
                .map(|r| -> Value {
                    json!({
                        "repo_id": r.repo_id,
                        "display_name": r.display_name,
                        "source_root": r.source_root,
                        "indexed_at": r.indexed_at,
                        "indexing_status": r.indexing_status.as_str(),
                        "freshness_status": r.freshness_status.as_str(),
                        "file_count": r.file_count,
                        "symbol_count": r.symbol_count,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "repos": items }))).into_response()
        }
        Err(e) => error_response(e).into_response(),
    }
}

async fn get_repo(
    State(state): State<SharedState>,
    Path(repo_id): Path<String>,
) -> impl IntoResponse {
    let result = state.with_db(|db| {
        db.repos()
            .get(&repo_id)?
            .ok_or_else(|| store::StoreError::Validation(format!("repo '{repo_id}' not found")))
    });

    match result {
        Ok(repo) => (
            StatusCode::OK,
            Json(json!({
                "repo_id": repo.repo_id,
                "display_name": repo.display_name,
                "source_root": repo.source_root,
                "indexed_at": repo.indexed_at,
                "index_version": repo.index_version,
                "indexing_status": repo.indexing_status.as_str(),
                "freshness_status": repo.freshness_status.as_str(),
                "file_count": repo.file_count,
                "symbol_count": repo.symbol_count,
                "language_counts": repo.language_counts,
                "registered_at": repo.registered_at,
                "git_head": repo.git_head,
            })),
        )
            .into_response(),
        Err(ServiceError::Store(store::StoreError::Validation(_))) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("repo '{repo_id}' not found") })),
        )
            .into_response(),
        Err(e) => error_response(e).into_response(),
    }
}

/// Remove a repository, its metadata, and orphaned blobs.
///
/// This is a destructive operation accessible to any local process.
/// Per DR-5 the service binds to localhost only with no auth in the
/// first slice. Auth guards are deferred to the hosted deployment
/// model (see `docs/architecture/persistent-local-service.md`).
async fn remove_repo(
    State(state): State<SharedState>,
    Path(repo_id): Path<String>,
) -> impl IntoResponse {
    // Collect file hashes, delete repo metadata, then clean up orphaned blobs.
    let result = state.with_db(|db| -> Result<Option<Vec<String>>, store::StoreError> {
        // Verify the repo exists.
        if db.repos().get(&repo_id)?.is_none() {
            return Ok(None);
        }

        let hashes = db.files().list_hashes(&repo_id)?;

        // Delete repo (cascades to files/symbols via ON DELETE CASCADE).
        db.repos().delete(&repo_id)?;

        // Determine which hashes are now orphaned.
        let mut orphaned = Vec::new();
        for hash in hashes {
            if !db.files().is_hash_referenced(&hash)? {
                orphaned.push(hash);
            }
        }
        Ok(Some(orphaned))
    });

    match result {
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("repo '{repo_id}' not found") })),
        )
            .into_response(),
        Ok(Some(orphaned_hashes)) => {
            // Clean up orphaned blobs outside the DB lock.
            let blob_path = state.config.blob_path();
            let mut blobs_removed = 0u64;
            let mut blob_errors: Vec<String> = Vec::new();

            if blob_path.is_dir() {
                match store::BlobStore::open(&blob_path) {
                    Ok(blob_store) => {
                        for hash in &orphaned_hashes {
                            match blob_store.delete(hash) {
                                Ok(true) => blobs_removed += 1,
                                Ok(false) => {} // already absent
                                Err(e) => {
                                    blob_errors.push(format!("{hash}: {e}"));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        blob_errors.push(format!("failed to open blob store: {e}"));
                    }
                }
            }

            let mut resp = json!({ "removed": repo_id });
            if blobs_removed > 0 {
                resp["blobs_removed"] = json!(blobs_removed);
            }
            if !blob_errors.is_empty() {
                resp["blob_errors"] = json!(blob_errors);
            }

            // Repo metadata is removed regardless of blob cleanup outcome.
            // Blob failures are surfaced in the response body so callers
            // can detect partial cleanup, but the HTTP status remains 200
            // because the primary operation (metadata deletion) succeeded.
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => error_response(e).into_response(),
    }
}

// ── Query (tool dispatch) ──────────────────────────────────────────────

pub fn query_routes() -> Router<SharedState> {
    Router::new().route("/tools/call", post(tools_call))
}

#[derive(Debug, Deserialize)]
struct ToolCallRequest {
    name: String,
    #[serde(default)]
    arguments: Value,
}

/// Dispatch a tool call through the MCP registry.
///
/// On success (including MCP-level tool errors like unknown_tool or
/// invalid_params), returns 200 with the full MCP response envelope.
/// On infrastructure failure (lock poisoned), returns 500 with
/// `{"error": "..."}`.
async fn tools_call(
    State(state): State<SharedState>,
    Json(req): Json<ToolCallRequest>,
) -> impl IntoResponse {
    let blob_store = store::BlobStore::open(&state.config.blob_path());
    let result = state.with_db(|db| {
        let bs = blob_store.as_ref().map_err(|e| store::StoreError::Blob {
            path: None,
            reason: format!("failed to open blob store: {e}"),
        })?;
        let svc = StoreQueryService::new(db, bs);
        let registry = ToolRegistry::new(&svc);
        Ok::<_, store::StoreError>(registry.call(&req.name, req.arguments.clone()))
    });

    match result {
        Ok(mcp_response) => (
            StatusCode::OK,
            Json(serde_json::to_value(mcp_response).unwrap_or(json!(null))),
        )
            .into_response(),
        Err(e) => error_response(e).into_response(),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use std::time::Instant;
    use tower::ServiceExt;

    fn test_state() -> SharedState {
        let db = store::MetadataStore::open_in_memory().unwrap();
        SharedState {
            db: std::sync::Arc::new(std::sync::Mutex::new(db)),
            config: crate::ServiceConfig::new(std::path::PathBuf::from("/tmp/test")),
            started_at: Instant::now(),
        }
    }

    fn app(state: SharedState) -> Router {
        Router::new()
            .merge(health_routes())
            .merge(status_routes())
            .merge(repo_routes())
            .merge(query_routes())
            .with_state(state)
    }

    #[tokio::test]
    async fn health_returns_200() {
        let resp = app(test_state())
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn status_returns_json() {
        let resp = app(test_state())
            .oneshot(Request::get("/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert!(json["uptime_secs"].is_number());
        assert!(json["repo_count"].is_number());
        assert!(json["data_root"].is_string());
        assert!(json["version"].is_string());
        assert!(json["index_version"].is_string());
    }

    #[tokio::test]
    async fn list_repos_empty() {
        let resp = app(test_state())
            .oneshot(Request::get("/repos").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["repos"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_repo_not_found() {
        let resp = app(test_state())
            .oneshot(
                Request::get("/repos/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn remove_repo_not_found() {
        let resp = app(test_state())
            .oneshot(
                Request::delete("/repos/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn tools_call_unknown_tool() {
        let body = serde_json::to_string(&json!({
            "name": "nonexistent_tool",
            "arguments": {}
        }))
        .unwrap();

        let resp = app(test_state())
            .oneshot(
                Request::post("/tools/call")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        // MCP-level errors return 200 with the MCP error envelope.
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "error");
    }

    #[tokio::test]
    async fn tools_call_list_repos() {
        let body =
            serde_json::to_string(&json!({ "name": "list_repos", "arguments": {} })).unwrap();

        let resp = app(test_state())
            .oneshot(
                Request::post("/tools/call")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "success");
    }

    fn insert_test_repo(db: &store::MetadataStore) {
        use core_model::{FreshnessStatus, IndexingStatus, RepoRecord};
        use std::collections::BTreeMap;

        db.repos()
            .upsert(&RepoRecord {
                repo_id: "test-repo".to_string(),
                display_name: "Test".to_string(),
                source_root: "/tmp/test".to_string(),
                indexed_at: "2025-01-15T10:30:00Z".to_string(),
                index_version: "1.0.0".to_string(),
                language_counts: BTreeMap::new(),
                file_count: 1,
                symbol_count: 5,
                git_head: None,
                registered_at: Some("2025-01-15T10:30:00Z".to_string()),
                indexing_status: IndexingStatus::Ready,
                freshness_status: FreshnessStatus::Fresh,
            })
            .unwrap();
    }

    fn insert_test_file(db: &store::MetadataStore, hash: &str) {
        db.files()
            .upsert(&core_model::FileRecord {
                repo_id: "test-repo".to_string(),
                file_path: "src/main.rs".to_string(),
                language: "rust".to_string(),
                file_hash: hash.to_string(),
                summary: "test".to_string(),
                symbol_count: 1,
                capability_tier: core_model::CapabilityTier::SyntaxOnly,
                updated_at: "2025-01-15T10:30:00Z".to_string(),
            })
            .unwrap();
    }

    #[tokio::test]
    async fn remove_repo_success() {
        let state = test_state();
        {
            let db = state.db.lock().unwrap();
            insert_test_repo(&db);
        }

        let resp = app(state.clone())
            .oneshot(
                Request::delete("/repos/test-repo")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["removed"], "test-repo");

        // Verify repo is actually gone.
        let db = state.db.lock().unwrap();
        assert!(db.repos().get("test-repo").unwrap().is_none());
    }

    #[tokio::test]
    async fn remove_repo_cleans_up_orphaned_blobs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config = crate::ServiceConfig::new(tmp.path().to_path_buf());
        let db = store::MetadataStore::open(&config.db_path()).unwrap();
        let blob_store = store::BlobStore::open(&config.blob_path()).unwrap();

        // Insert a repo with a file whose content is stored as a blob.
        let content = b"fn main() {}";
        let hash = store::content_hash(content);
        blob_store.put(content).unwrap();

        insert_test_repo(&db);
        insert_test_file(&db, &hash);

        assert!(blob_store.exists(&hash).unwrap());

        let state = SharedState {
            db: std::sync::Arc::new(std::sync::Mutex::new(db)),
            config,
            started_at: Instant::now(),
        };

        let resp = app(state)
            .oneshot(
                Request::delete("/repos/test-repo")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["removed"], "test-repo");
        assert_eq!(json["blobs_removed"], 1);

        // Blob should be gone.
        assert!(!blob_store.exists(&hash).unwrap());
    }

    #[tokio::test]
    async fn remove_repo_surfaces_blob_errors() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config = crate::ServiceConfig::new(tmp.path().to_path_buf());
        let db = store::MetadataStore::open(&config.db_path()).unwrap();
        let blob_store = store::BlobStore::open(&config.blob_path()).unwrap();

        let content = b"fn main() {}";
        let hash = store::content_hash(content);
        blob_store.put(content).unwrap();

        insert_test_repo(&db);
        insert_test_file(&db, &hash);

        // Sabotage the blob by replacing it with a directory.
        let shard = &hash[..2];
        let blob_file = config.blob_path().join(shard).join(&hash);
        std::fs::remove_file(&blob_file).unwrap();
        std::fs::create_dir(&blob_file).unwrap();

        let state = SharedState {
            db: std::sync::Arc::new(std::sync::Mutex::new(db)),
            config,
            started_at: Instant::now(),
        };

        let resp = app(state)
            .oneshot(
                Request::delete("/repos/test-repo")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Metadata deletion succeeds, so HTTP status is still 200.
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["removed"], "test-repo");
        assert!(
            json["blob_errors"].is_array(),
            "should surface blob errors: {json}"
        );
        assert!(
            !json["blob_errors"].as_array().unwrap().is_empty(),
            "blob_errors should not be empty"
        );
    }

    #[tokio::test]
    async fn status_returns_error_when_db_poisoned() {
        let state = test_state();

        // Poison the mutex by panicking inside a lock.
        let db_clone = state.db.clone();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = db_clone.lock().unwrap();
            panic!("intentional panic to poison mutex");
        }));

        // The mutex is now poisoned.
        assert!(state.db.lock().is_err(), "mutex should be poisoned");

        let resp = app(state)
            .oneshot(Request::get("/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "error");
        assert!(json["error"].is_string());
    }

    #[tokio::test]
    async fn error_responses_have_consistent_shape() {
        // Both repo 404 and tool 500-path should have {"error": "..."}
        let resp = app(test_state())
            .oneshot(Request::get("/repos/missing").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["error"].is_string(),
            "error responses should have string 'error' field"
        );
    }
}
