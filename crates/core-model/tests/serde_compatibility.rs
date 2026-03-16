use core_model::{FileRecord, FreshnessStatus, IndexingStatus, RepoRecord, SymbolRecord, Validate};

const SYMBOL_V1: &str = include_str!("fixtures/symbol_record.v1.json");
const SYMBOL_V1_EXTRA_FIELD: &str = include_str!("fixtures/symbol_record.v1.extra_field.json");
const SYMBOL_V2: &str = include_str!("fixtures/symbol_record.v2.json");
const SYMBOL_V3: &str = include_str!("fixtures/symbol_record.v3.json");
const FILE_V1: &str = include_str!("fixtures/file_record.v1.json");
const FILE_V1_EXTRA_FIELD: &str = include_str!("fixtures/file_record.v1.extra_field.json");
const FILE_V2: &str = include_str!("fixtures/file_record.v2.json");
const REPO_V1: &str = include_str!("fixtures/repo_record.v1.json");
const REPO_V1_EXTRA_FIELD: &str = include_str!("fixtures/repo_record.v1.extra_field.json");
const REPO_V2: &str = include_str!("fixtures/repo_record.v2.json");
const SYMBOL_MISSING_ID: &str = include_str!("fixtures/symbol_record.invalid.missing_id.json");

// ── Symbol v3 (current format: capability_tier + source_backend) ──────

#[test]
fn symbol_v3_fixture_round_trip_matches_snapshot() {
    let parsed: SymbolRecord =
        serde_json::from_str(SYMBOL_V3).expect("deserialize symbol v3 fixture");
    parsed
        .validate()
        .expect("symbol v3 fixture should validate");

    let serialized = serde_json::to_string_pretty(&parsed).expect("serialize symbol v3 fixture");
    assert_eq!(serialized, trim_newline(SYMBOL_V3));
}

// ── Symbol v2 (old format: quality_level + source_adapter, repo-prefixed ID) ──
//
// v2 fixtures use the previous field names (quality_level, source_adapter).
// Deserialization fails because the fields have been renamed.

#[test]
fn symbol_v2_fixture_fails_deserialization_with_old_field_names() {
    let result = serde_json::from_str::<SymbolRecord>(SYMBOL_V2);
    assert!(
        result.is_err(),
        "v2 fixture should fail deserialization (quality_level renamed to capability_tier)"
    );
}

// ── Symbol v1 (old format: no repo prefix in ID) ─────────────────────
//
// v1 fixtures also use the old field names, so they fail deserialization
// for the same reason as v2.

#[test]
fn symbol_v1_fixture_fails_deserialization_with_old_field_names() {
    let result = serde_json::from_str::<SymbolRecord>(SYMBOL_V1);
    assert!(
        result.is_err(),
        "v1 fixture should fail deserialization (quality_level renamed to capability_tier)"
    );
}

#[test]
fn symbol_v1_extra_field_fixture_fails_deserialization_with_old_field_names() {
    let result = serde_json::from_str::<SymbolRecord>(SYMBOL_V1_EXTRA_FIELD);
    assert!(
        result.is_err(),
        "v1 extra field fixture should fail deserialization (quality_level renamed)"
    );
}

// ── File v2 (current format: capability_tier) ─────────────────────────

#[test]
fn file_v2_fixture_round_trip_matches_snapshot() {
    let parsed: FileRecord = serde_json::from_str(FILE_V2).expect("deserialize file v2 fixture");
    parsed.validate().expect("file v2 fixture should validate");

    let serialized = serde_json::to_string_pretty(&parsed).expect("serialize file v2 fixture");
    assert_eq!(serialized, trim_newline(FILE_V2));
}

// ── File v1 (old format: quality_mix) ─────────────────────────────────
//
// v1 fixtures use quality_mix instead of capability_tier. Deserialization fails.

#[test]
fn file_v1_fixture_fails_deserialization_with_old_field_names() {
    let result = serde_json::from_str::<FileRecord>(FILE_V1);
    assert!(
        result.is_err(),
        "v1 file fixture should fail deserialization (quality_mix replaced by capability_tier)"
    );
}

#[test]
fn file_v1_extra_field_fixture_fails_deserialization_with_old_field_names() {
    let result = serde_json::from_str::<FileRecord>(FILE_V1_EXTRA_FIELD);
    assert!(
        result.is_err(),
        "v1 extra field file fixture should fail deserialization"
    );
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
