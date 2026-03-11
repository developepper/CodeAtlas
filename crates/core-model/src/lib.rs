use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub mod schema_version;
pub mod symbol_id;

pub use schema_version::{
    current_index_schema_version, migration_decision, parse_schema_version,
    CoreModelMigrationContract, MigrationContract, MigrationDecision, SchemaVersion,
};
pub use symbol_id::{
    build_symbol_id, disambiguate_symbol_id, normalize_file_path, normalize_qualified_name,
    parse_symbol_id, validate_symbol_id, ParsedSymbolId,
};

pub type ValidationResult = Result<(), ValidationError>;

pub trait Validate {
    fn validate(&self) -> ValidationResult;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    MissingField {
        field: &'static str,
    },
    InvalidField {
        field: &'static str,
        reason: &'static str,
    },
    InvalidElement {
        field: &'static str,
        index: usize,
        reason: &'static str,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField { field } => write!(f, "missing required field: {field}"),
            Self::InvalidField { field, reason } => {
                write!(f, "invalid field {field}: {reason}")
            }
            Self::InvalidElement {
                field,
                index,
                reason,
            } => write!(f, "invalid value in {field}[{index}]: {reason}"),
        }
    }
}

impl Error for ValidationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityLevel {
    Semantic,
    Syntax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Class,
    Method,
    Type,
    Constant,
    #[serde(other)]
    Unknown,
}

impl SymbolKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Class => "class",
            Self::Method => "method",
            Self::Type => "type",
            Self::Constant => "constant",
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub fn from_id_token(value: &str) -> Option<Self> {
        match value {
            "function" => Some(Self::Function),
            "class" => Some(Self::Class),
            "method" => Some(Self::Method),
            "type" => Some(Self::Type),
            "constant" => Some(Self::Constant),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolRecord {
    pub id: String,
    pub repo_id: String,
    pub file_path: String,
    pub language: String,
    pub kind: SymbolKind,
    pub name: String,
    pub qualified_name: String,
    pub signature: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_byte: u64,
    pub byte_length: u64,
    pub content_hash: String,
    pub quality_level: QualityLevel,
    pub confidence_score: f32,
    pub source_adapter: String,
    pub indexed_at: String,
    pub docstring: Option<String>,
    pub summary: Option<String>,
    pub parent_symbol_id: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub decorators_or_attributes: Option<Vec<String>>,
    pub semantic_refs: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityMix {
    pub semantic_percent: f32,
    pub syntax_percent: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileRecord {
    pub repo_id: String,
    pub file_path: String,
    pub language: String,
    pub file_hash: String,
    pub summary: String,
    pub symbol_count: u64,
    pub quality_mix: QualityMix,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoRecord {
    pub repo_id: String,
    pub display_name: String,
    pub source_root: String,
    pub indexed_at: String,
    pub index_version: String,
    pub language_counts: BTreeMap<String, u64>,
    pub file_count: u64,
    pub symbol_count: u64,
    pub git_head: Option<String>,
}

impl Validate for SymbolRecord {
    fn validate(&self) -> ValidationResult {
        require_non_empty(&self.id, "id")?;
        require_non_empty(&self.repo_id, "repo_id")?;
        require_non_empty(&self.file_path, "file_path")?;
        require_non_empty(&self.language, "language")?;
        require_non_empty(&self.name, "name")?;
        require_non_empty(&self.qualified_name, "qualified_name")?;
        require_non_empty(&self.signature, "signature")?;
        require_non_empty(&self.content_hash, "content_hash")?;
        require_non_empty(&self.source_adapter, "source_adapter")?;
        require_non_empty(&self.indexed_at, "indexed_at")?;
        validate_symbol_id(&self.id)?;

        let expected_id = build_symbol_id(&self.file_path, &self.qualified_name, self.kind)?;
        if self.id != expected_id {
            return Err(ValidationError::InvalidField {
                field: "id",
                reason: "must match canonical {file}::{qualified_name}#{kind} format",
            });
        }

        if self.start_line == 0 {
            return Err(ValidationError::InvalidField {
                field: "start_line",
                reason: "must be greater than zero",
            });
        }

        if self.end_line < self.start_line {
            return Err(ValidationError::InvalidField {
                field: "end_line",
                reason: "must be greater than or equal to start_line",
            });
        }

        if self.byte_length == 0 {
            return Err(ValidationError::InvalidField {
                field: "byte_length",
                reason: "must be greater than zero",
            });
        }

        if !self.confidence_score.is_finite() {
            return Err(ValidationError::InvalidField {
                field: "confidence_score",
                reason: "must be finite",
            });
        }

        if !(0.0..=1.0).contains(&self.confidence_score) {
            return Err(ValidationError::InvalidField {
                field: "confidence_score",
                reason: "must be within 0.0..=1.0",
            });
        }

        validate_rfc3339_timestamp(&self.indexed_at, "indexed_at")?;
        validate_optional_string(&self.docstring, "docstring")?;
        validate_optional_string(&self.summary, "summary")?;
        validate_optional_string(&self.parent_symbol_id, "parent_symbol_id")?;
        validate_optional_non_empty_items(&self.keywords, "keywords")?;
        validate_optional_non_empty_items(
            &self.decorators_or_attributes,
            "decorators_or_attributes",
        )?;
        validate_optional_non_empty_items(&self.semantic_refs, "semantic_refs")?;

        Ok(())
    }
}

impl Validate for QualityMix {
    fn validate(&self) -> ValidationResult {
        validate_percentage(self.semantic_percent, "quality_mix.semantic_percent")?;
        validate_percentage(self.syntax_percent, "quality_mix.syntax_percent")?;

        let total = self.semantic_percent + self.syntax_percent;
        if total > 100.0 + f32::EPSILON {
            return Err(ValidationError::InvalidField {
                field: "quality_mix",
                reason: "semantic_percent + syntax_percent must be <= 100",
            });
        }

        Ok(())
    }
}

impl Validate for FileRecord {
    fn validate(&self) -> ValidationResult {
        require_non_empty(&self.repo_id, "repo_id")?;
        require_non_empty(&self.file_path, "file_path")?;
        require_non_empty(&self.language, "language")?;
        require_non_empty(&self.file_hash, "file_hash")?;
        require_non_empty(&self.summary, "summary")?;
        require_non_empty(&self.updated_at, "updated_at")?;
        validate_rfc3339_timestamp(&self.updated_at, "updated_at")?;
        self.quality_mix.validate()?;
        Ok(())
    }
}

impl Validate for RepoRecord {
    fn validate(&self) -> ValidationResult {
        require_non_empty(&self.repo_id, "repo_id")?;
        require_non_empty(&self.display_name, "display_name")?;
        require_non_empty(&self.source_root, "source_root")?;
        require_non_empty(&self.indexed_at, "indexed_at")?;
        require_non_empty(&self.index_version, "index_version")?;

        parse_schema_version(&self.index_version)?;
        validate_rfc3339_timestamp(&self.indexed_at, "indexed_at")?;
        validate_optional_string(&self.git_head, "git_head")?;

        for language in self.language_counts.keys() {
            require_non_empty(language, "language_counts key")?;
        }

        Ok(())
    }
}

fn require_non_empty(value: &str, field: &'static str) -> ValidationResult {
    if value.trim().is_empty() {
        return Err(ValidationError::MissingField { field });
    }
    Ok(())
}

fn validate_optional_string(value: &Option<String>, field: &'static str) -> ValidationResult {
    if let Some(value) = value {
        if value.trim().is_empty() {
            return Err(ValidationError::InvalidField {
                field,
                reason: "must not be empty when present",
            });
        }
    }
    Ok(())
}

fn validate_optional_non_empty_items(
    values: &Option<Vec<String>>,
    field: &'static str,
) -> ValidationResult {
    if let Some(values) = values {
        for (index, value) in values.iter().enumerate() {
            if value.trim().is_empty() {
                return Err(ValidationError::InvalidElement {
                    field,
                    index,
                    reason: "must not contain empty values",
                });
            }
        }
    }
    Ok(())
}

fn validate_percentage(value: f32, field: &'static str) -> ValidationResult {
    if !value.is_finite() {
        return Err(ValidationError::InvalidField {
            field,
            reason: "must be finite",
        });
    }

    if !(0.0..=100.0).contains(&value) {
        return Err(ValidationError::InvalidField {
            field,
            reason: "must be within 0.0..=100.0",
        });
    }

    Ok(())
}

fn validate_rfc3339_timestamp(value: &str, field: &'static str) -> ValidationResult {
    if OffsetDateTime::parse(value, &Rfc3339).is_err() {
        return Err(ValidationError::InvalidField {
            field,
            reason: "must be RFC 3339 timestamp",
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{
        build_symbol_id, disambiguate_symbol_id, normalize_file_path, parse_symbol_id,
        validate_symbol_id, FileRecord, QualityLevel, QualityMix, RepoRecord, SymbolKind,
        SymbolRecord, Validate,
    };

    fn valid_symbol_record() -> SymbolRecord {
        let file_path = "src/main.rs";
        let qualified_name = "crate::run";
        let kind = SymbolKind::Function;
        SymbolRecord {
            id: build_symbol_id(file_path, qualified_name, kind).expect("build canonical id"),
            repo_id: "repo-1".to_string(),
            file_path: file_path.to_string(),
            language: "rust".to_string(),
            kind,
            name: "run".to_string(),
            qualified_name: qualified_name.to_string(),
            signature: "fn run()".to_string(),
            start_line: 10,
            end_line: 12,
            start_byte: 120,
            byte_length: 25,
            content_hash: "abc123".to_string(),
            quality_level: QualityLevel::Semantic,
            confidence_score: 0.95,
            source_adapter: "semantic-rust-v1".to_string(),
            indexed_at: "2026-03-09T00:00:00Z".to_string(),
            docstring: Some("Executes the app.".to_string()),
            summary: Some("Entry point".to_string()),
            parent_symbol_id: None,
            keywords: Some(vec!["entrypoint".to_string(), "startup".to_string()]),
            decorators_or_attributes: None,
            semantic_refs: Some(vec!["crate::Config".to_string()]),
        }
    }

    fn valid_file_record() -> FileRecord {
        FileRecord {
            repo_id: "repo-1".to_string(),
            file_path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            file_hash: "def456".to_string(),
            summary: "Main executable module".to_string(),
            symbol_count: 7,
            quality_mix: QualityMix {
                semantic_percent: 80.0,
                syntax_percent: 20.0,
            },
            updated_at: "2026-03-09T00:00:00Z".to_string(),
        }
    }

    fn valid_repo_record() -> RepoRecord {
        let mut language_counts = BTreeMap::new();
        language_counts.insert("rust".to_string(), 10);
        RepoRecord {
            repo_id: "repo-1".to_string(),
            display_name: "CodeAtlas".to_string(),
            source_root: "/repo".to_string(),
            indexed_at: "2026-03-09T00:00:00Z".to_string(),
            index_version: "1.0.0".to_string(),
            language_counts,
            file_count: 10,
            symbol_count: 90,
            git_head: None,
        }
    }

    #[test]
    fn symbol_validation_accepts_valid_record() {
        let record = valid_symbol_record();
        assert!(record.validate().is_ok());
    }

    #[test]
    fn symbol_validation_rejects_invalid_confidence_score() {
        let mut record = valid_symbol_record();
        record.confidence_score = 1.2;

        let err = record.validate().expect_err("expected validation failure");
        assert!(format!("{err}").contains("confidence_score"));
    }

    #[test]
    fn symbol_validation_rejects_zero_start_line() {
        let mut record = valid_symbol_record();
        record.start_line = 0;

        let err = record.validate().expect_err("expected validation failure");
        assert!(format!("{err}").contains("start_line"));
    }

    #[test]
    fn symbol_validation_rejects_zero_byte_length() {
        let mut record = valid_symbol_record();
        record.byte_length = 0;

        let err = record.validate().expect_err("expected validation failure");
        assert!(format!("{err}").contains("byte_length"));
    }

    #[test]
    fn symbol_validation_rejects_invalid_timestamp() {
        let mut record = valid_symbol_record();
        record.indexed_at = "yesterday".to_string();

        let err = record.validate().expect_err("expected validation failure");
        assert!(format!("{err}").contains("indexed_at"));
    }

    #[test]
    fn symbol_round_trips_through_json() {
        let record = valid_symbol_record();
        let payload = serde_json::to_string(&record).expect("serialize symbol");
        let decoded: SymbolRecord = serde_json::from_str(&payload).expect("deserialize symbol");

        assert_eq!(decoded, record);
    }

    #[test]
    fn symbol_deserialization_fails_when_required_field_missing() {
        let payload = json!({
            "repo_id": "repo-1",
            "file_path": "src/main.rs",
            "language": "rust",
            "kind": "function",
            "name": "run",
            "qualified_name": "crate::run",
            "signature": "fn run()",
            "start_line": 10,
            "end_line": 12,
            "start_byte": 120,
            "byte_length": 25,
            "content_hash": "abc123",
            "quality_level": "semantic",
            "confidence_score": 0.9,
            "source_adapter": "semantic-rust-v1",
            "indexed_at": "2026-03-09T00:00:00Z",
            "docstring": null,
            "summary": null,
            "parent_symbol_id": null,
            "keywords": null,
            "decorators_or_attributes": null,
            "semantic_refs": null
        });

        let result = serde_json::from_value::<SymbolRecord>(payload);
        assert!(result.is_err());
    }

    #[test]
    fn file_validation_rejects_invalid_quality_mix() {
        let mut record = valid_file_record();
        record.quality_mix.semantic_percent = 70.0;
        record.quality_mix.syntax_percent = 40.0;

        let err = record.validate().expect_err("expected validation failure");
        assert!(format!("{err}").contains("quality_mix"));
    }

    #[test]
    fn file_round_trips_through_json() {
        let record = valid_file_record();
        let payload = serde_json::to_string(&record).expect("serialize file");
        let decoded: FileRecord = serde_json::from_str(&payload).expect("deserialize file");

        assert_eq!(decoded, record);
    }

    #[test]
    fn repo_validation_rejects_missing_repo_id() {
        let mut record = valid_repo_record();
        record.repo_id.clear();

        let err = record.validate().expect_err("expected validation failure");
        assert!(format!("{err}").contains("repo_id"));
    }

    #[test]
    fn repo_validation_accepts_missing_git_head_when_not_available() {
        let record = valid_repo_record();
        assert!(record.validate().is_ok());
    }

    #[test]
    fn repo_round_trips_through_json() {
        let record = valid_repo_record();
        let payload = serde_json::to_string(&record).expect("serialize repo");
        let decoded: RepoRecord = serde_json::from_str(&payload).expect("deserialize repo");

        assert_eq!(decoded, record);
    }

    #[test]
    fn symbol_kind_unknown_deserializes_and_fails_validation() {
        let payload = json!({
            "id": "src/main.rs::crate::run#unknown",
            "repo_id": "repo-1",
            "file_path": "src/main.rs",
            "language": "rust",
            "kind": "fucntion",
            "name": "run",
            "qualified_name": "crate::run",
            "signature": "fn run()",
            "start_line": 10,
            "end_line": 12,
            "start_byte": 120,
            "byte_length": 25,
            "content_hash": "abc123",
            "quality_level": "semantic",
            "confidence_score": 0.9,
            "source_adapter": "semantic-rust-v1",
            "indexed_at": "2026-03-09T00:00:00Z",
            "docstring": null,
            "summary": null,
            "parent_symbol_id": null,
            "keywords": null,
            "decorators_or_attributes": null,
            "semantic_refs": null
        });

        let record: SymbolRecord =
            serde_json::from_value(payload).expect("unknown kind maps to SymbolKind::Unknown");
        let err = record
            .validate()
            .expect_err("expected unknown kind to fail");
        assert!(format!("{err}").contains("id"));
    }

    #[test]
    fn symbol_id_constructor_is_stable_for_unchanged_identity() {
        let id_a = build_symbol_id("src/lib.rs", "crate::service::run", SymbolKind::Function)
            .expect("build id");
        let id_b = build_symbol_id("src/lib.rs", "crate::service::run", SymbolKind::Function)
            .expect("build id");

        assert_eq!(id_a, id_b);
    }

    #[test]
    fn symbol_id_changes_on_symbol_rename() {
        let original = build_symbol_id("src/lib.rs", "crate::service::run", SymbolKind::Function)
            .expect("build id");
        let renamed = build_symbol_id(
            "src/lib.rs",
            "crate::service::execute",
            SymbolKind::Function,
        )
        .expect("build id");

        assert_ne!(original, renamed);
    }

    #[test]
    fn symbol_id_changes_on_file_move() {
        let original = build_symbol_id("src/lib.rs", "crate::service::run", SymbolKind::Function)
            .expect("build id");
        let moved = build_symbol_id(
            "src/engine/lib.rs",
            "crate::service::run",
            SymbolKind::Function,
        )
        .expect("build id");

        assert_ne!(original, moved);
    }

    #[test]
    fn symbol_id_changes_on_kind_change() {
        let function_id =
            build_symbol_id("src/lib.rs", "crate::service::run", SymbolKind::Function)
                .expect("build id");
        let method_id = build_symbol_id("src/lib.rs", "crate::service::run", SymbolKind::Method)
            .expect("build id");

        assert_ne!(function_id, method_id);
    }

    #[test]
    fn symbol_id_validation_rejects_malformed_values() {
        for malformed in [
            "",
            "src/lib.rs#function",
            "src/lib.rs::#function",
            "::crate::run#function",
            "src/lib.rs::crate::run",
            "src/lib.rs::crate::run#",
            "src/lib.rs::crate::run#invalid",
            "src/lib.rs::crate::run#function@bad",
            "src/lib.rs::crate::run#function@10:zero",
            "src/lib.rs::crate::run#function@10:0",
        ] {
            assert!(
                parse_symbol_id(malformed).is_err(),
                "expected malformed id to fail: {malformed}"
            );
        }
    }

    #[test]
    fn symbol_id_collision_disambiguation_is_deterministic() {
        let base = build_symbol_id("src/lib.rs", "crate::service::run", SymbolKind::Function)
            .expect("build id");
        let first = disambiguate_symbol_id(&base, 10, 20).expect("disambiguate");
        let again = disambiguate_symbol_id(&base, 10, 20).expect("disambiguate");
        let second = disambiguate_symbol_id(&base, 30, 20).expect("disambiguate");

        assert_eq!(first, again);
        assert_ne!(first, second);
    }

    #[test]
    fn symbol_id_parser_accepts_disambiguated_ids() {
        let base = build_symbol_id("src/lib.rs", "crate::service::run", SymbolKind::Function)
            .expect("build id");
        let disambiguated = disambiguate_symbol_id(&base, 10, 20).expect("disambiguate");
        let parsed = parse_symbol_id(&disambiguated).expect("parse disambiguated id");

        assert_eq!(parsed.file_path, "src/lib.rs");
        assert_eq!(parsed.qualified_name, "crate::service::run");
        assert_eq!(parsed.kind, SymbolKind::Function);
    }

    #[test]
    fn canonical_symbol_id_rejects_unknown_kind_token() {
        let err = validate_symbol_id("src/lib.rs::crate::service::run#unknown")
            .expect_err("unknown kind should fail canonical validation");
        assert!(format!("{err}").contains("unknown"));
    }

    #[test]
    fn file_path_normalization_removes_trailing_slashes() {
        let normalized = normalize_file_path("src/lib.rs///").expect("normalize path");
        assert_eq!(normalized, "src/lib.rs");
    }
}
