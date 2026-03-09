use serde::Deserialize;

use core_model::{migration_decision, parse_schema_version, MigrationDecision};

const MIGRATION_CASES_V1: &str = include_str!("fixtures/migration_contract_cases.v1.json");

#[derive(Debug, Deserialize)]
struct MigrationCase {
    from: String,
    to: String,
    expected: String,
}

#[test]
fn migration_contract_fixture_cases_match_expected_decisions() {
    let cases: Vec<MigrationCase> =
        serde_json::from_str(MIGRATION_CASES_V1).expect("deserialize migration cases fixture");

    for case in cases {
        let from = parse_schema_version(&case.from).expect("parse from version");
        let to = parse_schema_version(&case.to).expect("parse to version");
        let decision = migration_decision(from, to);
        let expected = expected_decision(case.expected.as_str(), from, to);

        assert_eq!(decision, expected, "case from={} to={}", case.from, case.to);
    }
}

fn expected_decision(
    expected: &str,
    from: core_model::SchemaVersion,
    to: core_model::SchemaVersion,
) -> MigrationDecision {
    match expected {
        "no_action" => MigrationDecision::NoAction,
        "migrate_in_place" => MigrationDecision::MigrateInPlace { from, to },
        "reindex_required" => MigrationDecision::ReindexRequired { from, to },
        "unsupported_future_version" => MigrationDecision::UnsupportedFutureVersion { from, to },
        other => panic!("unexpected expected decision token: {other}"),
    }
}
