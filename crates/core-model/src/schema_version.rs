use std::cmp::Ordering;
use std::fmt;

use crate::ValidationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl SchemaVersion {
    #[must_use]
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl PartialOrd for SchemaVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SchemaVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationDecision {
    NoAction,
    MigrateInPlace {
        from: SchemaVersion,
        to: SchemaVersion,
    },
    ReindexRequired {
        from: SchemaVersion,
        to: SchemaVersion,
    },
    UnsupportedFutureVersion {
        from: SchemaVersion,
        to: SchemaVersion,
    },
}

pub trait MigrationContract {
    fn current_version(&self) -> SchemaVersion;

    fn decision_for(&self, from: SchemaVersion) -> MigrationDecision {
        migration_decision(from, self.current_version())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CoreModelMigrationContract;

impl MigrationContract for CoreModelMigrationContract {
    fn current_version(&self) -> SchemaVersion {
        current_index_schema_version()
    }
}

#[must_use]
pub const fn current_index_schema_version() -> SchemaVersion {
    SchemaVersion::new(1, 0, 0)
}

pub fn parse_schema_version(value: &str) -> Result<SchemaVersion, ValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ValidationError::MissingField {
            field: "index_version",
        });
    }

    // Legacy `v<major>` is accepted for backward compatibility, but canonical
    // persisted/displayed form is semver `major.minor.patch`.
    if let Some(legacy_major) = value.strip_prefix('v') {
        let major = parse_numeric(legacy_major, "index_version")?;
        return Ok(SchemaVersion::new(major, 0, 0));
    }

    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 3 {
        return Err(ValidationError::InvalidField {
            field: "index_version",
            reason: "must be semver `major.minor.patch` or legacy `v<major>`",
        });
    }

    let major = parse_numeric(parts[0], "index_version")?;
    let minor = parse_numeric(parts[1], "index_version")?;
    let patch = parse_numeric(parts[2], "index_version")?;
    Ok(SchemaVersion::new(major, minor, patch))
}

#[must_use]
pub fn migration_decision(from: SchemaVersion, to: SchemaVersion) -> MigrationDecision {
    if from == to {
        return MigrationDecision::NoAction;
    }

    if from > to {
        return MigrationDecision::UnsupportedFutureVersion { from, to };
    }

    // Pre-1.0 schema versions are treated conservatively as incompatible.
    if from.major == 0 || to.major == 0 {
        return MigrationDecision::ReindexRequired { from, to };
    }

    // Same-major upgrades are considered backward compatible for schema readers.
    if from.major == to.major {
        return MigrationDecision::NoAction;
    }

    // N-1 major support: one major behind can be migrated in place.
    if from.major + 1 == to.major {
        return MigrationDecision::MigrateInPlace { from, to };
    }

    MigrationDecision::ReindexRequired { from, to }
}

fn parse_numeric(value: &str, field: &'static str) -> Result<u16, ValidationError> {
    if value.is_empty() {
        return Err(ValidationError::InvalidField {
            field,
            reason: "contains empty numeric component",
        });
    }

    if !value.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(ValidationError::InvalidField {
            field,
            reason: "contains non-numeric component",
        });
    }

    value
        .parse::<u16>()
        .map_err(|_| ValidationError::InvalidField {
            field,
            reason: "numeric component is out of range",
        })
}

#[cfg(test)]
mod tests {
    use super::{
        current_index_schema_version, migration_decision, parse_schema_version,
        CoreModelMigrationContract, MigrationContract, MigrationDecision, SchemaVersion,
    };

    #[test]
    fn parses_semver_schema_version() {
        let parsed = parse_schema_version("1.2.3").expect("parse semver");
        assert_eq!(parsed, SchemaVersion::new(1, 2, 3));
    }

    #[test]
    fn parses_legacy_schema_version() {
        let parsed = parse_schema_version("v2").expect("parse legacy version");
        assert_eq!(parsed, SchemaVersion::new(2, 0, 0));
    }

    #[test]
    fn rejects_invalid_schema_versions() {
        for invalid in ["", "v", "v2.1", "1.2", "1.a.0", "a.b.c", "1..0"] {
            assert!(
                parse_schema_version(invalid).is_err(),
                "expected invalid schema version: {invalid}"
            );
        }
    }

    #[test]
    fn migration_decision_covers_no_action() {
        let version = SchemaVersion::new(1, 0, 0);
        assert_eq!(
            migration_decision(version, version),
            MigrationDecision::NoAction
        );
    }

    #[test]
    fn migration_decision_supports_n_minus_one_major() {
        let from = SchemaVersion::new(1, 4, 2);
        let to = SchemaVersion::new(2, 0, 0);
        assert_eq!(
            migration_decision(from, to),
            MigrationDecision::MigrateInPlace { from, to }
        );
    }

    #[test]
    fn migration_decision_treats_same_major_as_no_action() {
        let from = SchemaVersion::new(1, 0, 0);
        let to = SchemaVersion::new(1, 5, 0);
        assert_eq!(migration_decision(from, to), MigrationDecision::NoAction);
    }

    #[test]
    fn migration_decision_requires_reindex_for_pre_one_zero() {
        let from = SchemaVersion::new(0, 1, 0);
        let to = SchemaVersion::new(0, 2, 0);
        assert_eq!(
            migration_decision(from, to),
            MigrationDecision::ReindexRequired { from, to }
        );
    }

    #[test]
    fn migration_decision_requires_reindex_for_older_majors() {
        let from = SchemaVersion::new(1, 0, 0);
        let to = SchemaVersion::new(3, 0, 0);
        assert_eq!(
            migration_decision(from, to),
            MigrationDecision::ReindexRequired { from, to }
        );
    }

    #[test]
    fn migration_decision_rejects_future_versions() {
        let from = SchemaVersion::new(2, 0, 0);
        let to = SchemaVersion::new(1, 9, 0);
        assert_eq!(
            migration_decision(from, to),
            MigrationDecision::UnsupportedFutureVersion { from, to }
        );
    }

    #[test]
    fn migration_contract_uses_current_version() {
        let contract = CoreModelMigrationContract;
        let current = contract.current_version();
        assert_eq!(current, current_index_schema_version());

        let from = SchemaVersion::new(1, 0, 0);
        let decision = contract.decision_for(from);
        assert_eq!(decision, MigrationDecision::NoAction);
    }
}
