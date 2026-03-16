//! Enrichment stage: heuristic file summaries, symbol summaries, and keyword
//! extraction.
//!
//! All functions are pure and deterministic — given the same input they always
//! produce the same output, with stable sort order and no randomness.
//!
//! See spec §8.3 (Enrichment Stage).

use core_model::SymbolKind;

use crate::merge_engine::MergedSymbol;

// ---------------------------------------------------------------------------
// File summary
// ---------------------------------------------------------------------------

/// Generates a heuristic summary for a source file based on its language
/// and the symbols extracted from it.
///
/// Format: `"{Language} source file with {kind counts} — {top symbol names}"`
///
/// When no symbols are present: `"{Language} source file (no symbols extracted)"`.
pub fn file_summary(language: &str, symbols: &[MergedSymbol]) -> String {
    if symbols.is_empty() {
        return format!("{language} source file (no symbols extracted)");
    }

    let kind_summary = kind_count_summary(symbols);
    let top_names = top_symbol_names(symbols, 5);

    if top_names.is_empty() {
        format!("{language} source file with {kind_summary}")
    } else {
        format!("{language} source file with {kind_summary} — {top_names}")
    }
}

/// Produces a comma-separated summary of symbol kind counts, e.g.
/// `"2 functions, 1 class, 1 constant"`.
///
/// Kinds are listed in a fixed order (function, class, method, type, constant,
/// unknown) and kinds with zero count are omitted.
fn kind_count_summary(symbols: &[MergedSymbol]) -> String {
    // Fixed display order.
    const ORDERED_KINDS: &[(SymbolKind, &str, &str)] = &[
        (SymbolKind::Function, "function", "functions"),
        (SymbolKind::Class, "class", "classes"),
        (SymbolKind::Method, "method", "methods"),
        (SymbolKind::Type, "type", "types"),
        (SymbolKind::Constant, "constant", "constants"),
        (SymbolKind::Unknown, "unknown symbol", "unknown symbols"),
    ];

    let mut counts = [0u32; 6];
    for sym in symbols {
        let idx = match sym.kind {
            SymbolKind::Function => 0,
            SymbolKind::Class => 1,
            SymbolKind::Method => 2,
            SymbolKind::Type => 3,
            SymbolKind::Constant => 4,
            SymbolKind::Unknown => 5,
        };
        counts[idx] += 1;
    }

    let parts: Vec<String> = ORDERED_KINDS
        .iter()
        .enumerate()
        .filter(|(i, _)| counts[*i] > 0)
        .map(|(i, (_, singular, plural))| {
            let c = counts[i];
            if c == 1 {
                format!("{c} {singular}")
            } else {
                format!("{c} {plural}")
            }
        })
        .collect();

    parts.join(", ")
}

/// Returns a comma-separated list of up to `limit` top-level symbol names,
/// sorted alphabetically for determinism.
fn top_symbol_names(symbols: &[MergedSymbol], limit: usize) -> String {
    let mut names: Vec<&str> = symbols
        .iter()
        .filter(|s| s.parent_qualified_name.is_none())
        .map(|s| s.name.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    names.truncate(limit);
    names.join(", ")
}

// ---------------------------------------------------------------------------
// Symbol summary
// ---------------------------------------------------------------------------

/// Generates a heuristic summary for a single symbol.
///
/// If a docstring is present, uses its first sentence. Otherwise falls back
/// to a signature-based description.
pub fn symbol_summary(symbol: &MergedSymbol) -> String {
    if let Some(sentence) = first_docstring_sentence(symbol.docstring.as_deref()) {
        return sentence;
    }
    signature_summary(symbol)
}

/// Extracts the first sentence from a docstring, trimming leading `///`
/// markers, `*` prefixes, and whitespace.
fn first_docstring_sentence(docstring: Option<&str>) -> Option<String> {
    let doc = docstring?;

    // Collect lines, stripping common doc-comment prefixes.
    let cleaned: String = doc
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            let stripped = trimmed
                .strip_prefix("///")
                .or_else(|| trimmed.strip_prefix("//!"))
                .or_else(|| trimmed.strip_prefix("/**"))
                .or_else(|| trimmed.strip_prefix("*"))
                .unwrap_or(trimmed);
            stripped.trim()
        })
        .collect::<Vec<_>>()
        .join(" ");

    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }

    // Take up to the first sentence-ending punctuation.
    let sentence = if let Some(pos) = cleaned.find(['.', '!', '?']) {
        &cleaned[..=pos]
    } else {
        cleaned
    };

    Some(sentence.to_string())
}

/// Builds a summary from kind + signature when no docstring is available.
fn signature_summary(symbol: &MergedSymbol) -> String {
    let kind_label = match symbol.kind {
        SymbolKind::Function => "Function",
        SymbolKind::Class => "Class",
        SymbolKind::Method => "Method",
        SymbolKind::Type => "Type",
        SymbolKind::Constant => "Constant",
        SymbolKind::Unknown => "Symbol",
    };

    format!("{kind_label} {}", symbol.signature)
}

// ---------------------------------------------------------------------------
// Keyword extraction
// ---------------------------------------------------------------------------

/// Extracts searchable keywords from a symbol's name, qualified name,
/// signature, and docstring.
///
/// Tokens are lowercased, deduplicated, and sorted for determinism. Single-
/// character tokens and common noise words are filtered out.
pub fn extract_keywords(symbol: &MergedSymbol) -> Vec<String> {
    let mut tokens = Vec::new();

    // Name tokens (split snake_case and camelCase).
    tokens.extend(split_identifier(&symbol.name));

    // Qualified name segments.
    for segment in symbol.qualified_name.split("::") {
        tokens.extend(split_identifier(segment));
    }

    // Signature type tokens.
    tokens.extend(extract_signature_tokens(&symbol.signature));

    // Docstring tokens (first 50 words).
    if let Some(doc) = &symbol.docstring {
        tokens.extend(extract_doc_tokens(doc, 50));
    }

    normalize_keywords(&mut tokens);
    tokens
}

/// Splits an identifier on snake_case and camelCase boundaries.
fn split_identifier(ident: &str) -> Vec<String> {
    let mut tokens = Vec::new();

    // First split on underscores.
    for part in ident.split('_') {
        if part.is_empty() {
            continue;
        }
        // Then split camelCase.
        tokens.extend(split_camel_case(part));
    }

    tokens
}

/// Splits a camelCase or PascalCase string into individual words.
fn split_camel_case(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();

    for i in 0..chars.len() {
        let c = chars[i];
        if c.is_uppercase() && !current.is_empty() {
            // Look ahead: if next char is lowercase, this starts a new word.
            // Also split on transitions like "HTTPServer" → "HTTP", "Server".
            let next_is_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            let prev_is_upper = current.chars().last().is_some_and(|p| p.is_uppercase());
            if !prev_is_upper || next_is_lower {
                words.push(std::mem::take(&mut current));
            }
        }
        current.push(c);
    }
    if !current.is_empty() {
        words.push(current);
    }

    words
}

/// Extracts type-like tokens from a function signature.
///
/// Looks for identifiers that start with an uppercase letter or common type
/// patterns, skipping Rust keywords like `fn`, `pub`, `mut`, etc.
fn extract_signature_tokens(signature: &str) -> Vec<String> {
    let mut tokens = Vec::new();

    for word in tokenize_source(signature) {
        if is_rust_keyword(&word) {
            continue;
        }
        // Type names start with uppercase or are multi-segment paths.
        if word.chars().next().is_some_and(|c| c.is_uppercase()) {
            tokens.extend(split_identifier(&word));
        }
    }

    tokens
}

/// Extracts the first `limit` meaningful words from a docstring.
fn extract_doc_tokens(doc: &str, limit: usize) -> Vec<String> {
    doc.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() > 1)
        .take(limit)
        .map(|w| w.to_string())
        .collect()
}

/// Tokenizes source code into identifier-like tokens.
fn tokenize_source(src: &str) -> Vec<String> {
    src.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

/// Normalizes a keyword list: lowercase, deduplicate, sort, and filter noise.
fn normalize_keywords(tokens: &mut Vec<String>) {
    for t in tokens.iter_mut() {
        *t = t.to_lowercase();
    }

    tokens.retain(|t| t.len() > 1 && !is_noise_word(t));
    tokens.sort_unstable();
    tokens.dedup();
}

fn is_rust_keyword(word: &str) -> bool {
    matches!(
        word,
        "fn" | "pub"
            | "crate"
            | "super"
            | "self"
            | "Self"
            | "mut"
            | "const"
            | "static"
            | "let"
            | "if"
            | "else"
            | "match"
            | "for"
            | "while"
            | "loop"
            | "return"
            | "impl"
            | "struct"
            | "enum"
            | "trait"
            | "type"
            | "where"
            | "use"
            | "mod"
            | "async"
            | "await"
            | "dyn"
            | "ref"
            | "in"
            | "as"
    )
}

fn is_noise_word(word: &str) -> bool {
    matches!(
        word,
        "a" | "an"
            | "the"
            | "is"
            | "it"
            | "of"
            | "to"
            | "or"
            | "and"
            | "in"
            | "on"
            | "at"
            | "by"
            | "for"
            | "if"
            | "as"
            | "be"
            | "no"
            | "do"
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use core_model::SourceSpan;

    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        signature: &str,
        docstring: Option<&str>,
    ) -> MergedSymbol {
        MergedSymbol {
            name: name.to_string(),
            qualified_name: name.to_string(),
            kind,
            span: SourceSpan {
                start_line: 1,
                end_line: 1,
                start_byte: 0,
                byte_length: 10,
            },
            signature: signature.to_string(),
            confidence_score: 0.7,
            docstring: docstring.map(|s| s.to_string()),
            parent_qualified_name: None,
            type_refs: vec![],
            call_refs: vec![],
        }
    }

    fn make_method(name: &str, parent: &str, signature: &str) -> MergedSymbol {
        MergedSymbol {
            name: name.to_string(),
            qualified_name: format!("{parent}::{name}"),
            kind: SymbolKind::Method,
            span: SourceSpan {
                start_line: 1,
                end_line: 1,
                start_byte: 0,
                byte_length: 10,
            },
            signature: signature.to_string(),
            confidence_score: 0.7,
            docstring: None,
            parent_qualified_name: Some(parent.to_string()),
            type_refs: vec![],
            call_refs: vec![],
        }
    }

    // -- file_summary tests --

    #[test]
    fn file_summary_no_symbols() {
        let summary = file_summary("rust", &[]);
        assert_eq!(summary, "rust source file (no symbols extracted)");
    }

    #[test]
    fn file_summary_single_function() {
        let symbols = vec![make_symbol("main", SymbolKind::Function, "fn main()", None)];
        let summary = file_summary("rust", &symbols);
        assert_eq!(summary, "rust source file with 1 function — main");
    }

    #[test]
    fn file_summary_mixed_kinds() {
        let symbols = vec![
            make_symbol("Config", SymbolKind::Class, "struct Config", None),
            make_method("new", "Config", "fn new() -> Config"),
            make_symbol("greet", SymbolKind::Function, "fn greet()", None),
            make_symbol("MAX", SymbolKind::Constant, "const MAX: u32", None),
        ];
        let summary = file_summary("rust", &symbols);
        // Methods have a parent so they're excluded from top names.
        assert_eq!(
            summary,
            "rust source file with 1 function, 1 class, 1 method, 1 constant — Config, MAX, greet"
        );
    }

    #[test]
    fn file_summary_truncates_names_to_five() {
        let symbols: Vec<_> = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta"]
            .iter()
            .map(|n| make_symbol(n, SymbolKind::Function, &format!("fn {n}()"), None))
            .collect();
        let summary = file_summary("rust", &symbols);
        // Only first 5 names (alphabetically sorted).
        assert!(summary.contains("alpha, beta, delta, epsilon, gamma"));
        assert!(!summary.contains("zeta"));
    }

    #[test]
    fn file_summary_is_deterministic() {
        let symbols = vec![
            make_symbol("beta", SymbolKind::Function, "fn beta()", None),
            make_symbol("alpha", SymbolKind::Function, "fn alpha()", None),
        ];
        let a = file_summary("rust", &symbols);
        let b = file_summary("rust", &symbols);
        assert_eq!(a, b);
        assert!(a.contains("alpha, beta"));
    }

    // -- symbol_summary tests --

    #[test]
    fn symbol_summary_with_docstring_uses_first_sentence() {
        let sym = make_symbol(
            "greet",
            SymbolKind::Function,
            "fn greet()",
            Some("/// Greets the user. Returns a greeting string."),
        );
        let summary = symbol_summary(&sym);
        assert_eq!(summary, "Greets the user.");
    }

    #[test]
    fn symbol_summary_without_docstring_uses_signature() {
        let sym = make_symbol("greet", SymbolKind::Function, "fn greet(name: &str)", None);
        let summary = symbol_summary(&sym);
        assert_eq!(summary, "Function fn greet(name: &str)");
    }

    #[test]
    fn symbol_summary_class_kind() {
        let sym = make_symbol("Config", SymbolKind::Class, "struct Config", None);
        let summary = symbol_summary(&sym);
        assert_eq!(summary, "Class struct Config");
    }

    #[test]
    fn symbol_summary_docstring_no_period() {
        let sym = make_symbol(
            "init",
            SymbolKind::Function,
            "fn init()",
            Some("Initializes the system"),
        );
        let summary = symbol_summary(&sym);
        assert_eq!(summary, "Initializes the system");
    }

    #[test]
    fn symbol_summary_multiline_docstring() {
        let sym = make_symbol(
            "parse",
            SymbolKind::Function,
            "fn parse()",
            Some("/// Parses the input.\n/// Returns the AST."),
        );
        let summary = symbol_summary(&sym);
        assert_eq!(summary, "Parses the input.");
    }

    // -- extract_keywords tests --

    #[test]
    fn keywords_from_snake_case_name() {
        let sym = make_symbol(
            "parse_input_file",
            SymbolKind::Function,
            "fn parse_input_file()",
            None,
        );
        let kw = extract_keywords(&sym);
        assert!(kw.contains(&"parse".to_string()));
        assert!(kw.contains(&"input".to_string()));
        assert!(kw.contains(&"file".to_string()));
    }

    #[test]
    fn keywords_from_camel_case_name() {
        let sym = make_symbol(
            "parseInputFile",
            SymbolKind::Function,
            "fn parseInputFile()",
            None,
        );
        let kw = extract_keywords(&sym);
        assert!(kw.contains(&"parse".to_string()));
        assert!(kw.contains(&"input".to_string()));
        assert!(kw.contains(&"file".to_string()));
    }

    #[test]
    fn keywords_include_signature_types() {
        let sym = make_symbol(
            "greet",
            SymbolKind::Function,
            "fn greet(config: &Config) -> String",
            None,
        );
        let kw = extract_keywords(&sym);
        assert!(kw.contains(&"config".to_string()));
        assert!(kw.contains(&"string".to_string()));
    }

    #[test]
    fn keywords_include_docstring_terms() {
        let sym = make_symbol(
            "save",
            SymbolKind::Function,
            "fn save()",
            Some("Saves the configuration to disk"),
        );
        let kw = extract_keywords(&sym);
        assert!(kw.contains(&"configuration".to_string()));
        assert!(kw.contains(&"disk".to_string()));
        assert!(kw.contains(&"saves".to_string()));
    }

    #[test]
    fn keywords_are_deduplicated_and_sorted() {
        let sym = make_symbol("Config", SymbolKind::Class, "struct Config", None);
        let kw = extract_keywords(&sym);
        // Should be sorted.
        let mut sorted = kw.clone();
        sorted.sort_unstable();
        assert_eq!(kw, sorted);
        // No duplicates.
        let mut deduped = kw.clone();
        deduped.dedup();
        assert_eq!(kw, deduped);
    }

    #[test]
    fn keywords_exclude_noise_and_single_chars() {
        let sym = make_symbol(
            "a",
            SymbolKind::Function,
            "fn a(x: i32) -> i32",
            Some("A simple function"),
        );
        let kw = extract_keywords(&sym);
        // Single-char "a" and "x" should be filtered out.
        assert!(!kw.contains(&"a".to_string()));
        assert!(!kw.contains(&"x".to_string()));
        // Noise word "a" (lowercased) should be filtered.
        assert!(!kw.iter().any(|k| k == "a"));
    }

    #[test]
    fn keywords_are_deterministic() {
        let sym = make_symbol(
            "processData",
            SymbolKind::Method,
            "fn processData(input: &[u8]) -> Result",
            Some("Processes the raw data bytes"),
        );
        let a = extract_keywords(&sym);
        let b = extract_keywords(&sym);
        assert_eq!(a, b);
    }

    // -- split_camel_case tests --

    #[test]
    fn split_camel_case_pascal() {
        assert_eq!(split_camel_case("HttpServer"), vec!["Http", "Server"]);
    }

    #[test]
    fn split_camel_case_acronym_run() {
        assert_eq!(split_camel_case("HTTPServer"), vec!["HTTP", "Server"]);
    }

    #[test]
    fn split_camel_case_lowercase() {
        assert_eq!(split_camel_case("simple"), vec!["simple"]);
    }

    // -- first_docstring_sentence tests --

    #[test]
    fn docstring_strips_rust_comment_prefixes() {
        let s = first_docstring_sentence(Some("/// Hello world. More text.")).unwrap();
        assert_eq!(s, "Hello world.");
    }

    #[test]
    fn docstring_handles_empty() {
        assert!(first_docstring_sentence(Some("")).is_none());
        assert!(first_docstring_sentence(Some("///")).is_none());
        assert!(first_docstring_sentence(None).is_none());
    }
}
