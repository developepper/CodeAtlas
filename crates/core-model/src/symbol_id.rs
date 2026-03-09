use crate::{SymbolKind, ValidationError, ValidationResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSymbolId {
    pub file_path: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
}

pub fn build_symbol_id(
    file_path: &str,
    qualified_name: &str,
    kind: SymbolKind,
) -> Result<String, ValidationError> {
    let file_path = normalize_file_path(file_path)?;
    let qualified_name = normalize_qualified_name(qualified_name)?;
    if kind == SymbolKind::Unknown {
        return Err(ValidationError::InvalidField {
            field: "kind",
            reason: "unknown is not allowed for canonical symbol id construction",
        });
    }

    Ok(format!("{file_path}::{qualified_name}#{}", kind.as_str()))
}

pub fn parse_symbol_id(value: &str) -> Result<ParsedSymbolId, ValidationError> {
    // ID format invariant: `file_path` must not contain `::`.
    // We split on the first `::` and treat the remainder as `{qualified_name}#{kind}`.
    let (file_path, symbol_part) = value
        .split_once("::")
        .ok_or(ValidationError::InvalidField {
            field: "id",
            reason: "must contain '::' separator",
        })?;

    let (qualified_name, kind_with_suffix) =
        symbol_part
            .rsplit_once('#')
            .ok_or(ValidationError::InvalidField {
                field: "id",
                reason: "must contain '#{kind}' suffix",
            })?;

    let (kind_token, disambiguator) = match kind_with_suffix.split_once('@') {
        Some((kind_token, disambiguator)) => (kind_token, Some(disambiguator)),
        None => (kind_with_suffix, None),
    };

    let normalized_file = normalize_file_path(file_path)?;
    let normalized_qualified = normalize_qualified_name(qualified_name)?;
    let kind = SymbolKind::from_id_token(kind_token).ok_or(ValidationError::InvalidField {
        field: "id",
        reason: "kind token is invalid",
    })?;
    if let Some(disambiguator) = disambiguator {
        validate_disambiguator(disambiguator)?;
    }

    Ok(ParsedSymbolId {
        file_path: normalized_file,
        qualified_name: normalized_qualified,
        kind,
    })
}

pub fn disambiguate_symbol_id(
    base_id: &str,
    start_byte: u64,
    byte_length: u64,
) -> Result<String, ValidationError> {
    parse_symbol_id(base_id)?;
    if byte_length == 0 {
        return Err(ValidationError::InvalidField {
            field: "byte_length",
            reason: "must be greater than zero for collision disambiguation",
        });
    }

    Ok(format!("{base_id}@{start_byte}:{byte_length}"))
}

pub fn normalize_file_path(value: &str) -> Result<String, ValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::MissingField { field: "file_path" });
    }

    let mut normalized = String::with_capacity(trimmed.len());
    let mut previous_was_slash = false;
    for ch in trimmed.chars() {
        let mapped = if ch == '\\' { '/' } else { ch };
        if mapped == '/' {
            if previous_was_slash {
                continue;
            }
            previous_was_slash = true;
        } else {
            previous_was_slash = false;
        }
        normalized.push(mapped);
    }

    while normalized.ends_with('/') {
        normalized.pop();
    }

    if normalized.is_empty() {
        return Err(ValidationError::MissingField { field: "file_path" });
    }
    if normalized.contains("::") {
        return Err(ValidationError::InvalidField {
            field: "file_path",
            reason: "must not contain '::' because it conflicts with symbol ID separators",
        });
    }

    Ok(normalized)
}

pub fn normalize_qualified_name(value: &str) -> Result<String, ValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::MissingField {
            field: "qualified_name",
        });
    }

    let parts: Vec<&str> = trimmed.split("::").map(str::trim).collect();
    if parts.iter().any(|part| part.is_empty()) {
        return Err(ValidationError::InvalidField {
            field: "qualified_name",
            reason: "must not contain empty path segments",
        });
    }

    Ok(parts.join("::"))
}

pub fn validate_symbol_id(value: &str) -> ValidationResult {
    let parsed = parse_symbol_id(value)?;
    if parsed.kind == SymbolKind::Unknown {
        return Err(ValidationError::InvalidField {
            field: "id",
            reason: "kind token 'unknown' is not allowed in canonical symbol IDs",
        });
    }
    Ok(())
}

fn validate_disambiguator(value: &str) -> ValidationResult {
    let (start_byte, byte_length) = value.split_once(':').ok_or(ValidationError::InvalidField {
        field: "id",
        reason: "invalid disambiguator; expected '@{start_byte}:{byte_length}'",
    })?;

    if start_byte.parse::<u64>().is_err() {
        return Err(ValidationError::InvalidField {
            field: "id",
            reason: "disambiguator start_byte must be an unsigned integer",
        });
    }

    let byte_length = byte_length
        .parse::<u64>()
        .map_err(|_| ValidationError::InvalidField {
            field: "id",
            reason: "disambiguator byte_length must be an unsigned integer",
        })?;
    if byte_length == 0 {
        return Err(ValidationError::InvalidField {
            field: "id",
            reason: "disambiguator byte_length must be greater than zero",
        });
    }

    Ok(())
}
