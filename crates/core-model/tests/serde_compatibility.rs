use core_model::{FileRecord, FreshnessStatus, IndexingStatus, RepoRecord, SymbolRecord, Validate};

const SYMBOL_V1: &str = include_str!("fixtures/symbol_record.v1.json");
const SYMBOL_V1_EXTRA_FIELD: &str = include_str!("fixtures/symbol_record.v1.extra_field.json");
const SYMBOL_V2: &str = include_str!("fixtures/symbol_record.v2.json");
const FILE_V1: &str = include_str!("fixtures/file_record.v1.json");
const FILE_V1_EXTRA_FIELD: &str = include_str!("fixtures/file_record.v1.extra_field.json");
const REPO_V1: &str = include_str!("fixtures/repo_record.v1.json");
const REPO_V1_EXTRA_FIELD: &str = include_str!("fixtures/repo_record.v1.extra_field.json");
const REPO_V2: &str = include_str!("fixtures/repo_record.v2.json");
const SYMBOL_MISSING_ID: &str = include_str!("fixtures/symbol_record.invalid.missing_id.json");

// ── Symbol v2 (current format: repo-prefixed IDs) ────────────────────

#[test]
fn symbol_v2_fixture_round_trip_matches_snapshot() {
    let parsed: SymbolRecord =
        serde_json::from_str(SYMBOL_V2).expect("deserialize symbol v2 fixture");
    parsed
        .validate()
        .expect("symbol v2 fixture should validate");

    let serialized = serde_json::to_string_pretty(&parsed).expect("serialize symbol v2 fixture");
    assert_eq!(serialized, trim_newline(SYMBOL_V2));
}

// ── Symbol v1 (old format: no repo prefix in ID) ─────────────────────
//
// The v1 fixtures preserve the pre-#150 symbol ID format
// (`file::qualified#kind` without a `repo_id//` prefix). These records
// deserialize successfully but fail validation because the ID no longer
// matches the canonical `build_symbol_id` output. This is an intentional
// breaking change documented in ticket #150.

#[test]
fn symbol_v1_fixture_deserializes_but_fails_current_validation() {
    let parsed: SymbolRecord =
        serde_json::from_str(SYMBOL_V1).expect("deserialize v1 symbol fixture");

    // Deserialization works — the JSON shape is still valid.
    assert_eq!(parsed.id, "src/main.rs::crate::run#function");
    assert_eq!(parsed.repo_id, "repo-1");

    // Validation fails — the ID does not match the new repo-prefixed format.
    let err = parsed
        .validate()
        .expect_err("v1 symbol ID should fail current validation");
    assert!(
        format!("{err}").contains("id"),
        "expected id validation error, got: {err}"
    );
}

#[test]
fn symbol_v1_extra_field_fixture_deserializes_but_fails_current_validation() {
    let parsed: SymbolRecord = serde_json::from_str(SYMBOL_V1_EXTRA_FIELD)
        .expect("deserialize v1 symbol fixture with extra field");

    // Deserialization works and ignores unknown fields.
    assert_eq!(parsed.id, "src/main.rs::crate::run#function");

    // Validation fails — old ID format.
    assert!(parsed.validate().is_err());
}

// ── File fixtures ────────────────────────────────────────────────────

#[test]
fn file_fixture_round_trip_matches_snapshot() {
    let parsed: FileRecord = serde_json::from_str(FILE_V1).expect("deserialize file fixture");
    parsed.validate().expect("file fixture should validate");

    let serialized = serde_json::to_string_pretty(&parsed).expect("serialize file fixture");
    assert_eq!(serialized, trim_newline(FILE_V1));
}

#[test]
fn file_fixture_deserialization_ignores_unknown_fields_for_compatibility() {
    let parsed: FileRecord = serde_json::from_str(FILE_V1_EXTRA_FIELD)
        .expect("deserialize file fixture with extra field");
    parsed
        .validate()
        .expect("file fixture with extra field should still validate");

    assert_eq!(parsed.file_path, "src/main.rs");
}

// ── Repo fixtures ────────────────────────────────────────────────────

#[test]
fn repo_v2_fixture_round_trip_matches_snapshot_with_stable_map_order() {
    let parsed: RepoRecord = serde_json::from_str(REPO_V2).expect("deserialize repo v2 fixture");
    parsed.validate().expect("repo v2 fixture should validate");

    let serialized_once = serde_json::to_string_pretty(&parsed).expect("serialize repo v2 fixture");
    let serialized_twice =
        serde_json::to_string_pretty(&parsed).expect("serialize repo v2 fixture again");

    assert_eq!(serialized_once, trim_newline(REPO_V2));
    assert_eq!(serialized_once, serialized_twice);
}

#[test]
fn repo_v1_fixture_deserializes_with_defaults_for_new_fields() {
    let parsed: RepoRecord = serde_json::from_str(REPO_V1).expect("deserialize repo v1 fixture");

    // New fields get their defaults when missing from v1 JSON.
    assert_eq!(parsed.registered_at, None);
    assert_eq!(parsed.indexing_status, IndexingStatus::Ready);
    assert_eq!(parsed.freshness_status, FreshnessStatus::Fresh);

    // Core fields are preserved.
    assert_eq!(parsed.repo_id, "repo-1");
    assert_eq!(parsed.display_name, "CodeAtlas");
}

#[test]
fn repo_fixture_deserialization_ignores_unknown_fields_for_compatibility() {
    let parsed: RepoRecord =
        serde_json::from_str(REPO_V1_EXTRA_FIELD).expect("deserialize fixture with extra field");
    parsed
        .validate()
        .expect("fixture with extra field should still validate");

    assert_eq!(parsed.repo_id, "repo-1");
}

// ── Negative cases ───────────────────────────────────────────────────

#[test]
fn symbol_fixture_missing_required_field_fails_deserialization() {
    let err = serde_json::from_str::<SymbolRecord>(SYMBOL_MISSING_ID)
        .expect_err("deserialization should fail for missing required id");
    assert!(err.to_string().contains("missing field"));
}

fn trim_newline(value: &str) -> &str {
    value.strip_suffix('\n').unwrap_or(value)
}
