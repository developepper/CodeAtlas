//! Deterministic ranking pipeline for symbol search.
//!
//! Implements the scoring signals from spec section 9.2:
//! - Exact name match
//! - Token overlap (name, qualified_name, signature, summary, keywords)
//! - Confidence boost for semantic quality
//!
//! Tie-breaking uses the symbol ID for deterministic ordering.

use core_model::{QualityLevel, SymbolRecord};

use crate::ScoredSymbol;

/// Computes the ranking score for a candidate symbol against a query string.
///
/// Returns a score in `[0.0, 1.0]` or `None` if the candidate does not match
/// the query at all (no token overlap).
pub fn score_symbol(query: &str, record: &SymbolRecord) -> Option<f32> {
    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return None;
    }

    let name_lower = record.name.to_lowercase();
    let query_lower = query.to_lowercase();

    // Signal 1: Exact name match (0.0 or 0.4).
    let exact_match = if name_lower == query_lower { 0.4 } else { 0.0 };

    // Signal 2: Token overlap across searchable fields.
    let candidate_tokens = symbol_tokens(record);
    let overlap = token_overlap(&query_tokens, &candidate_tokens);

    // If no overlap at all, this candidate is not a match.
    if overlap == 0.0 && exact_match == 0.0 {
        return None;
    }

    // Signal 3: Signature relevance — bonus if query appears in the signature.
    let sig_lower = record.signature.to_lowercase();
    let sig_bonus = if sig_lower.contains(&query_lower) {
        0.05
    } else {
        0.0
    };

    // Signal 4: Confidence boost for semantic quality.
    let confidence_boost = match record.quality_level {
        QualityLevel::Semantic => record.confidence_score * 0.1,
        QualityLevel::Syntax => 0.0,
    };

    // Weighted combination, clamped to [0.0, 1.0].
    let raw = exact_match + overlap * 0.45 + sig_bonus + confidence_boost;
    Some(raw.min(1.0))
}

/// Sorts scored symbols deterministically: score descending, then ID ascending.
pub fn sort_scored(results: &mut [ScoredSymbol]) {
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.record.id.cmp(&b.record.id))
    });
}

// ---------------------------------------------------------------------------
// Tokenization
// ---------------------------------------------------------------------------

/// Tokenizes a query string. Keeps all non-empty tokens including single-char
/// ones so that queries like `"x"` can match symbol names.
fn tokenize(text: &str) -> Vec<String> {
    split_identifiers(text)
        .into_iter()
        .map(|t| t.to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Tokenizes symbol fields for candidate matching. Filters out single-char
/// fragments that arise from splitting identifiers (e.g. generic `T`) to
/// reduce noise in overlap scoring.
fn symbol_tokens(record: &SymbolRecord) -> Vec<String> {
    let mut parts = Vec::new();
    parts.push(record.name.clone());
    parts.push(record.qualified_name.clone());
    parts.push(record.signature.clone());
    if let Some(ref summary) = record.summary {
        parts.push(summary.clone());
    }
    if let Some(ref docstring) = record.docstring {
        parts.push(docstring.clone());
    }
    if let Some(ref keywords) = record.keywords {
        parts.extend(keywords.iter().cloned());
    }

    parts
        .iter()
        .flat_map(|p| split_identifiers(p))
        .map(|t| t.to_lowercase())
        .filter(|t| t.len() > 1)
        .collect()
}

/// Splits text on word boundaries: snake_case, camelCase, PascalCase,
/// whitespace, punctuation.
fn split_identifiers(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch == '_' || ch == ':' || ch == '(' || ch == ')' || ch == ',' || ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else if ch.is_uppercase() && !current.is_empty() {
            // camelCase boundary.
            tokens.push(std::mem::take(&mut current));
            current.push(ch);
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

/// Computes the fraction of query tokens found in the candidate token set.
fn token_overlap(query_tokens: &[String], candidate_tokens: &[String]) -> f32 {
    if query_tokens.is_empty() {
        return 0.0;
    }
    let matched = query_tokens
        .iter()
        .filter(|qt| candidate_tokens.iter().any(|ct| ct.contains(qt.as_str())))
        .count();
    matched as f32 / query_tokens.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_model::{build_symbol_id, SymbolKind};

    fn make_symbol(name: &str, kind: SymbolKind) -> SymbolRecord {
        let file_path = "src/lib.rs";
        let qualified_name = format!("crate::{name}");
        SymbolRecord {
            id: build_symbol_id(file_path, &qualified_name, kind).expect("build id"),
            repo_id: "repo-1".into(),
            file_path: file_path.into(),
            language: "rust".into(),
            kind,
            name: name.into(),
            qualified_name,
            signature: format!("fn {name}()"),
            start_line: 1,
            end_line: 5,
            start_byte: 0,
            byte_length: 50,
            content_hash: "hash".into(),
            quality_level: QualityLevel::Syntax,
            confidence_score: 0.8,
            source_adapter: "syntax-treesitter-v1".into(),
            indexed_at: "2026-03-09T00:00:00Z".into(),
            docstring: None,
            summary: None,
            parent_symbol_id: None,
            keywords: None,
            decorators_or_attributes: None,
            semantic_refs: None,
        }
    }

    #[test]
    fn exact_name_match_scores_highest() {
        let sym = make_symbol("run", SymbolKind::Function);
        let score = score_symbol("run", &sym).expect("should match");
        // Exact match bonus (0.4) + token overlap (1.0 * 0.45) + sig bonus (0.05) = 0.9.
        assert!(score > 0.8, "exact match score should be high: {score}");
    }

    #[test]
    fn partial_name_match_scores_lower_than_exact() {
        let sym = make_symbol("run_server", SymbolKind::Function);
        let exact_score = score_symbol("run_server", &sym).expect("should match");
        let partial_score = score_symbol("run", &sym).expect("should match");
        assert!(
            exact_score > partial_score,
            "exact ({exact_score}) should beat partial ({partial_score})"
        );
    }

    #[test]
    fn no_overlap_returns_none() {
        let sym = make_symbol("run", SymbolKind::Function);
        assert!(score_symbol("xyz_unrelated", &sym).is_none());
    }

    #[test]
    fn semantic_quality_boosts_score() {
        let mut syntax_sym = make_symbol("process", SymbolKind::Function);
        syntax_sym.quality_level = QualityLevel::Syntax;

        let mut semantic_sym = syntax_sym.clone();
        semantic_sym.quality_level = QualityLevel::Semantic;
        semantic_sym.confidence_score = 0.95;

        let syntax_score = score_symbol("process", &syntax_sym).expect("match");
        let semantic_score = score_symbol("process", &semantic_sym).expect("match");
        assert!(
            semantic_score > syntax_score,
            "semantic ({semantic_score}) should beat syntax ({syntax_score})"
        );
    }

    #[test]
    fn keyword_match_contributes_to_score() {
        let mut sym = make_symbol("handle_request", SymbolKind::Function);
        sym.keywords = Some(vec!["http".into(), "server".into()]);

        let without_kw_score = score_symbol(
            "server",
            &make_symbol("handle_request", SymbolKind::Function),
        );
        let with_kw_score = score_symbol("server", &sym);

        // Without keywords, "server" doesn't match at all.
        assert!(without_kw_score.is_none());
        // With keywords, it should match.
        assert!(with_kw_score.is_some());
    }

    #[test]
    fn sort_scored_is_deterministic() {
        let sym_a = make_symbol("alpha", SymbolKind::Function);
        let sym_b = make_symbol("beta", SymbolKind::Function);
        let mut items = vec![
            ScoredSymbol {
                record: sym_b.clone(),
                score: 0.5,
            },
            ScoredSymbol {
                record: sym_a.clone(),
                score: 0.5,
            },
        ];
        sort_scored(&mut items);

        let mut items2 = vec![
            ScoredSymbol {
                record: sym_a,
                score: 0.5,
            },
            ScoredSymbol {
                record: sym_b,
                score: 0.5,
            },
        ];
        sort_scored(&mut items2);

        let ids1: Vec<&str> = items.iter().map(|s| s.record.id.as_str()).collect();
        let ids2: Vec<&str> = items2.iter().map(|s| s.record.id.as_str()).collect();
        assert_eq!(ids1, ids2);
    }

    #[test]
    fn sort_scored_higher_score_first() {
        let sym_a = make_symbol("alpha", SymbolKind::Function);
        let sym_b = make_symbol("beta", SymbolKind::Function);
        let mut items = vec![
            ScoredSymbol {
                record: sym_a,
                score: 0.3,
            },
            ScoredSymbol {
                record: sym_b,
                score: 0.9,
            },
        ];
        sort_scored(&mut items);

        assert_eq!(items[0].record.name, "beta");
        assert_eq!(items[1].record.name, "alpha");
    }

    #[test]
    fn tokenize_splits_snake_case() {
        let tokens = tokenize("run_server");
        assert!(tokens.contains(&"run".to_string()));
        assert!(tokens.contains(&"server".to_string()));
    }

    #[test]
    fn tokenize_splits_camel_case() {
        let tokens = tokenize("runServer");
        assert!(tokens.contains(&"run".to_string()));
        assert!(tokens.contains(&"server".to_string()));
    }

    #[test]
    fn single_char_query_matches_exact_name() {
        let sym = make_symbol("x", SymbolKind::Function);
        let score = score_symbol("x", &sym);
        assert!(
            score.is_some(),
            "single-char query should match symbol named 'x'"
        );
        assert!(score.unwrap() > 0.0);
    }

    #[test]
    fn single_char_query_matches_via_substring() {
        let sym = make_symbol("max", SymbolKind::Function);
        let score = score_symbol("x", &sym);
        // "x" appears as a substring of "max" in candidate tokens.
        assert!(
            score.is_some(),
            "single-char query should match via substring"
        );
    }

    #[test]
    fn single_char_query_no_match() {
        let sym = make_symbol("run", SymbolKind::Function);
        let score = score_symbol("z", &sym);
        assert!(
            score.is_none(),
            "single-char query 'z' should not match 'run'"
        );
    }

    #[test]
    fn score_clamped_to_one() {
        let mut sym = make_symbol("x", SymbolKind::Function);
        sym.quality_level = QualityLevel::Semantic;
        sym.confidence_score = 1.0;
        sym.keywords = Some(vec!["x".into()]);
        sym.summary = Some("x".into());
        sym.docstring = Some("x".into());
        // Even with max signals, score should not exceed 1.0.
        if let Some(score) = score_symbol("x", &sym) {
            assert!(score <= 1.0, "score should be clamped: {score}");
        }
    }
}
