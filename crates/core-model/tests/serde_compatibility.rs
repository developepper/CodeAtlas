use core_model::{FileRecord, RepoRecord, SymbolRecord, Validate};

const SYMBOL_V1: &str = include_str!("fixtures/symbol_record.v1.json");
const SYMBOL_V1_EXTRA_FIELD: &str = include_str!("fixtures/symbol_record.v1.extra_field.json");
const FILE_V1: &str = include_str!("fixtures/file_record.v1.json");
const FILE_V1_EXTRA_FIELD: &str = include_str!("fixtures/file_record.v1.extra_field.json");
const REPO_V1: &str = include_str!("fixtures/repo_record.v1.json");
const REPO_V1_EXTRA_FIELD: &str = include_str!("fixtures/repo_record.v1.extra_field.json");
const SYMBOL_MISSING_ID: &str = include_str!("fixtures/symbol_record.invalid.missing_id.json");

#[test]
fn symbol_fixture_round_trip_matches_snapshot() {
    let parsed: SymbolRecord = serde_json::from_str(SYMBOL_V1).expect("deserialize symbol fixture");
    parsed.validate().expect("symbol fixture should validate");

    let serialized = serde_json::to_string_pretty(&parsed).expect("serialize symbol fixture");
    assert_eq!(serialized, trim_newline(SYMBOL_V1));
}

#[test]
fn file_fixture_round_trip_matches_snapshot() {
    let parsed: FileRecord = serde_json::from_str(FILE_V1).expect("deserialize file fixture");
    parsed.validate().expect("file fixture should validate");

    let serialized = serde_json::to_string_pretty(&parsed).expect("serialize file fixture");
    assert_eq!(serialized, trim_newline(FILE_V1));
}

#[test]
fn repo_fixture_round_trip_matches_snapshot_with_stable_map_order() {
    let parsed: RepoRecord = serde_json::from_str(REPO_V1).expect("deserialize repo fixture");
    parsed.validate().expect("repo fixture should validate");

    let serialized_once = serde_json::to_string_pretty(&parsed).expect("serialize repo fixture");
    let serialized_twice =
        serde_json::to_string_pretty(&parsed).expect("serialize repo fixture again");

    assert_eq!(serialized_once, trim_newline(REPO_V1));
    assert_eq!(serialized_once, serialized_twice);
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

#[test]
fn symbol_fixture_deserialization_ignores_unknown_fields_for_compatibility() {
    let parsed: SymbolRecord = serde_json::from_str(SYMBOL_V1_EXTRA_FIELD)
        .expect("deserialize symbol fixture with extra field");
    parsed
        .validate()
        .expect("symbol fixture with extra field should still validate");

    assert_eq!(parsed.id, "src/main.rs::crate::run#function");
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

#[test]
fn symbol_fixture_missing_required_field_fails_deserialization() {
    let err = serde_json::from_str::<SymbolRecord>(SYMBOL_MISSING_ID)
        .expect_err("deserialization should fail for missing required id");
    assert!(err.to_string().contains("missing field"));
}

fn trim_newline(value: &str) -> &str {
    value.strip_suffix('\n').unwrap_or(value)
}
