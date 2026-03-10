//! Individual MCP tool handler implementations.
//!
//! Each function takes a `&dyn QueryService` and a JSON params value,
//! deserializes the request, calls the query engine, and returns a
//! serialized payload or typed error.

use core_model::SymbolKind;
use query_engine::{
    FileContentRequest, FileOutlineRequest, FileTreeRequest, QueryFilters, QueryService,
    RepoOutlineRequest, SymbolQuery, TextQuery,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::types::{McpError, QualityStats, ToolOutput, ToolResult};

// ── Request param types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SearchSymbolsParams {
    pub repo_id: String,
    pub query: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Deserialize)]
pub struct GetSymbolParams {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct GetSymbolsParams {
    pub ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct FileOutlineParams {
    pub repo_id: String,
    pub file_path: String,
}

#[derive(Debug, Deserialize)]
pub struct FileContentParams {
    pub repo_id: String,
    pub file_path: String,
}

#[derive(Debug, Deserialize)]
pub struct FileTreeParams {
    pub repo_id: String,
    #[serde(default)]
    pub path_prefix: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RepoOutlineParams {
    pub repo_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchTextParams {
    pub repo_id: String,
    pub pattern: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    20
}

// ── Response serialization helpers ─────────────────────────────────────

#[derive(Serialize)]
struct SymbolPayload {
    id: String,
    name: String,
    kind: String,
    qualified_name: String,
    file_path: String,
    language: String,
    signature: String,
    start_line: u32,
    end_line: u32,
    quality_level: String,
    confidence_score: f32,
    source_adapter: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    docstring: Option<String>,
}

impl From<&core_model::SymbolRecord> for SymbolPayload {
    fn from(r: &core_model::SymbolRecord) -> Self {
        Self {
            id: r.id.clone(),
            name: r.name.clone(),
            kind: r.kind.as_str().to_string(),
            qualified_name: r.qualified_name.clone(),
            file_path: r.file_path.clone(),
            language: r.language.clone(),
            signature: r.signature.clone(),
            start_line: r.start_line,
            end_line: r.end_line,
            quality_level: format!("{:?}", r.quality_level).to_lowercase(),
            confidence_score: r.confidence_score,
            source_adapter: r.source_adapter.clone(),
            docstring: r.docstring.clone(),
        }
    }
}

/// Serialize a `SymbolPayload` to JSON, returning an `McpError::Internal` on failure.
fn serialize_symbol(record: &core_model::SymbolRecord) -> Result<serde_json::Value, McpError> {
    serde_json::to_value(SymbolPayload::from(record))
        .map_err(|e| McpError::internal(format!("failed to serialize symbol: {e}")))
}

// ── Quality stats computation ─────────────────────────────────────────

/// Compute quality mix from a slice of symbol records.
fn compute_quality_stats(records: &[&core_model::SymbolRecord]) -> QualityStats {
    if records.is_empty() {
        return QualityStats::default();
    }
    let total = records.len() as f64;
    let semantic = records
        .iter()
        .filter(|r| r.quality_level == core_model::QualityLevel::Semantic)
        .count() as f64;
    QualityStats {
        semantic_percent: (semantic / total) * 100.0,
        syntax_percent: ((total - semantic) / total) * 100.0,
    }
}

// ── Tool handlers ──────────────────────────────────────────────────────

pub fn search_symbols(svc: &dyn QueryService, params: serde_json::Value) -> ToolResult {
    let p: SearchSymbolsParams =
        serde_json::from_value(params).map_err(|e| McpError::invalid_params(e.to_string()))?;

    let kind = p.kind.as_deref().map(parse_symbol_kind).transpose()?;

    let query = SymbolQuery {
        repo_id: p.repo_id,
        text: p.query,
        filters: QueryFilters {
            kind,
            language: p.language,
            quality_level: None,
            file_path: None,
        },
        limit: p.limit,
        offset: p.offset,
    };

    let result = svc.search_symbols(&query)?;

    let refs: Vec<&core_model::SymbolRecord> = result.items.iter().map(|s| &s.record).collect();
    let quality_stats = compute_quality_stats(&refs);

    let mut items: Vec<serde_json::Value> = Vec::with_capacity(result.items.len());
    for scored in &result.items {
        let mut val = serialize_symbol(&scored.record)?;
        let obj = val
            .as_object_mut()
            .ok_or_else(|| McpError::internal("serialized symbol is not an object"))?;
        obj.insert("score".into(), json!(scored.score));
        items.push(val);
    }

    Ok(ToolOutput {
        payload: json!({
            "items": items,
            "total_candidates": result.meta.total_candidates,
            "truncated": result.meta.truncated,
        }),
        quality_stats,
    })
}

pub fn get_symbol(svc: &dyn QueryService, params: serde_json::Value) -> ToolResult {
    let p: GetSymbolParams =
        serde_json::from_value(params).map_err(|e| McpError::invalid_params(e.to_string()))?;

    let record = svc.get_symbol(&p.id)?;
    let quality_stats = compute_quality_stats(&[&record]);
    Ok(ToolOutput {
        payload: serialize_symbol(&record)?,
        quality_stats,
    })
}

pub fn get_symbols(svc: &dyn QueryService, params: serde_json::Value) -> ToolResult {
    let p: GetSymbolsParams =
        serde_json::from_value(params).map_err(|e| McpError::invalid_params(e.to_string()))?;

    let id_refs: Vec<&str> = p.ids.iter().map(|s| s.as_str()).collect();
    let records = svc.get_symbols(&id_refs)?;

    let refs: Vec<&core_model::SymbolRecord> = records.iter().collect();
    let quality_stats = compute_quality_stats(&refs);

    let mut items: Vec<serde_json::Value> = Vec::with_capacity(records.len());
    for r in &records {
        items.push(serialize_symbol(r)?);
    }

    Ok(ToolOutput {
        payload: json!({ "items": items }),
        quality_stats,
    })
}

pub fn get_file_outline(svc: &dyn QueryService, params: serde_json::Value) -> ToolResult {
    let p: FileOutlineParams =
        serde_json::from_value(params).map_err(|e| McpError::invalid_params(e.to_string()))?;

    let outline = svc.get_file_outline(&FileOutlineRequest {
        repo_id: p.repo_id,
        file_path: p.file_path,
    })?;

    let refs: Vec<&core_model::SymbolRecord> = outline.symbols.iter().collect();
    let quality_stats = compute_quality_stats(&refs);

    let mut symbols: Vec<serde_json::Value> = Vec::with_capacity(outline.symbols.len());
    for s in &outline.symbols {
        symbols.push(serialize_symbol(s)?);
    }

    Ok(ToolOutput {
        payload: json!({
            "file": {
                "file_path": outline.file.file_path,
                "language": outline.file.language,
                "symbol_count": outline.file.symbol_count,
            },
            "symbols": symbols,
        }),
        quality_stats,
    })
}

pub fn get_file_content(svc: &dyn QueryService, params: serde_json::Value) -> ToolResult {
    let p: FileContentParams =
        serde_json::from_value(params).map_err(|e| McpError::invalid_params(e.to_string()))?;

    let content = svc.get_file_content(&FileContentRequest {
        repo_id: p.repo_id,
        file_path: p.file_path,
    })?;

    Ok(ToolOutput {
        payload: json!({
            "file": {
                "file_path": content.file.file_path,
                "language": content.file.language,
                "symbol_count": content.file.symbol_count,
            },
            "content": content.content,
        }),
        quality_stats: QualityStats::default(),
    })
}

pub fn get_file_tree(svc: &dyn QueryService, params: serde_json::Value) -> ToolResult {
    let p: FileTreeParams =
        serde_json::from_value(params).map_err(|e| McpError::invalid_params(e.to_string()))?;

    let entries = svc.get_file_tree(&FileTreeRequest {
        repo_id: p.repo_id,
        path_prefix: p.path_prefix,
    })?;

    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            json!({
                "path": e.path,
                "language": e.language,
                "symbol_count": e.symbol_count,
            })
        })
        .collect();

    Ok(ToolOutput {
        payload: json!({ "entries": items }),
        quality_stats: QualityStats::default(),
    })
}

pub fn get_repo_outline(svc: &dyn QueryService, params: serde_json::Value) -> ToolResult {
    let p: RepoOutlineParams =
        serde_json::from_value(params).map_err(|e| McpError::invalid_params(e.to_string()))?;

    let outline = svc.get_repo_outline(&RepoOutlineRequest { repo_id: p.repo_id })?;

    let files: Vec<serde_json::Value> = outline
        .files
        .iter()
        .map(|e| {
            json!({
                "path": e.path,
                "language": e.language,
                "symbol_count": e.symbol_count,
            })
        })
        .collect();

    Ok(ToolOutput {
        payload: json!({
            "repo": {
                "repo_id": outline.repo.repo_id,
                "display_name": outline.repo.display_name,
                "file_count": outline.repo.file_count,
                "symbol_count": outline.repo.symbol_count,
                "language_counts": outline.repo.language_counts,
            },
            "files": files,
        }),
        quality_stats: QualityStats::default(),
    })
}

pub fn search_text(svc: &dyn QueryService, params: serde_json::Value) -> ToolResult {
    let p: SearchTextParams =
        serde_json::from_value(params).map_err(|e| McpError::invalid_params(e.to_string()))?;

    let kind = p.kind.as_deref().map(parse_symbol_kind).transpose()?;

    let query = TextQuery {
        repo_id: p.repo_id,
        pattern: p.pattern,
        filters: QueryFilters {
            kind,
            language: p.language,
            quality_level: None,
            file_path: None,
        },
        limit: p.limit,
        offset: p.offset,
    };

    let result = svc.search_text(&query)?;

    let symbol_refs: Vec<&core_model::SymbolRecord> = result
        .items
        .iter()
        .filter_map(|m| m.symbol.as_ref())
        .collect();
    let quality_stats = compute_quality_stats(&symbol_refs);

    let items: Vec<serde_json::Value> = result
        .items
        .iter()
        .map(|m| {
            json!({
                "file_path": m.file_path,
                "line_number": m.line_number,
                "line_content": m.line_content,
                "score": m.score,
                "symbol": m.symbol.as_ref().map(SymbolPayload::from),
            })
        })
        .collect();

    Ok(ToolOutput {
        payload: json!({
            "items": items,
            "total_candidates": result.meta.total_candidates,
            "truncated": result.meta.truncated,
        }),
        quality_stats,
    })
}

// ── Helpers ────────────────────────────────────────────────────────────

fn parse_symbol_kind(s: &str) -> Result<SymbolKind, McpError> {
    match s.to_lowercase().as_str() {
        "function" => Ok(SymbolKind::Function),
        "class" => Ok(SymbolKind::Class),
        "method" => Ok(SymbolKind::Method),
        "type" => Ok(SymbolKind::Type),
        "constant" => Ok(SymbolKind::Constant),
        other => Err(McpError::invalid_params(format!(
            "unknown symbol kind: {other}"
        ))),
    }
}
