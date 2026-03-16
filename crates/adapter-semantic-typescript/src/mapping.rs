use core_model::{SourceSpan, SymbolKind};
use semantic_api::SemanticSymbol;
use serde::Deserialize;

/// A node in tsserver's navigation tree response.
///
/// Represents a single symbol in the file's hierarchical structure,
/// as returned by the `navtree` command. Each node may contain child
/// items forming the scope tree.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavTreeItem {
    pub text: String,
    pub kind: String,
    #[serde(default)]
    pub kind_modifiers: String,
    #[serde(default)]
    pub spans: Vec<TextSpan>,
    #[serde(default)]
    pub child_items: Vec<NavTreeItem>,
}

/// A text span as returned by tsserver.
#[derive(Debug, Clone, Deserialize)]
pub struct TextSpan {
    pub start: SpanLocation,
    pub end: SpanLocation,
}

/// A location within a source file (tsserver format).
#[derive(Debug, Clone, Deserialize)]
pub struct SpanLocation {
    pub line: u32,
    pub offset: u32,
}

/// Maps a tsserver `ScriptElementKind` string to a canonical `SymbolKind`.
///
/// tsserver uses strings like "function", "class", "method", "interface",
/// "enum", "const", "let", "var", "type", "module", etc.
fn map_ts_kind(ts_kind: &str, parent_kind: Option<&str>) -> Option<SymbolKind> {
    match ts_kind {
        "function" => {
            if is_class_like(parent_kind) {
                Some(SymbolKind::Method)
            } else {
                Some(SymbolKind::Function)
            }
        }
        "method" | "constructor" | "getter" | "setter" => Some(SymbolKind::Method),
        "class" => Some(SymbolKind::Class),
        "interface" | "enum" | "type" => Some(SymbolKind::Type),
        "const" | "let" | "var" => Some(SymbolKind::Constant),
        // Skip structural nodes that don't map to symbols.
        "module" | "script" | "directory" | "property" | "index" | "call" | "parameter"
        | "local function" | "local class" | "string" => None,
        _ => None,
    }
}

/// Returns whether a parent kind represents a class-like container.
fn is_class_like(parent_kind: Option<&str>) -> bool {
    matches!(parent_kind, Some("class" | "interface"))
}

/// Converts a tsserver `TextSpan` to a `SourceSpan` using source bytes
/// for accurate byte offset computation.
///
/// tsserver reports 1-based lines and 1-based character offsets. We convert
/// to the canonical schema: 1-based lines, 0-based byte offsets, with
/// byte_length computed from the source content.
fn text_span_to_source_span(span: &TextSpan, source: &[u8]) -> Option<SourceSpan> {
    let start_line = span.start.line;
    let end_line = span.end.line;

    if start_line == 0 || end_line == 0 || end_line < start_line {
        return None;
    }

    let start_byte = line_offset_to_byte(source, span.start.line, span.start.offset)?;
    let end_byte = line_offset_to_byte(source, span.end.line, span.end.offset)?;

    if end_byte <= start_byte {
        return None;
    }

    Some(SourceSpan {
        start_line,
        end_line,
        start_byte: start_byte as u64,
        byte_length: (end_byte - start_byte) as u64,
    })
}

/// Converts a 1-based line and 1-based UTF-16 offset to a 0-based byte offset.
///
/// tsserver offsets are 1-based positions measured in UTF-16 code units.
/// A BMP character (U+0000..U+FFFF) counts as 1 code unit; a supplementary
/// character (U+10000+) counts as 2 (a surrogate pair). This function walks
/// the source bytes as UTF-8, counting UTF-16 code units consumed, so that
/// the returned byte offset is correct for any valid UTF-8 source.
fn line_offset_to_byte(source: &[u8], line: u32, offset: u32) -> Option<usize> {
    if line == 0 || offset == 0 {
        return None;
    }

    let mut current_line: u32 = 1;
    let mut byte_pos: usize = 0;

    // Advance to the start of the target line.
    while current_line < line && byte_pos < source.len() {
        if source[byte_pos] == b'\n' {
            current_line += 1;
        }
        byte_pos += 1;
    }

    if current_line != line {
        return None;
    }

    // Advance by (offset - 1) UTF-16 code units within the line.
    // tsserver offsets are measured in UTF-16 code units: BMP characters
    // (U+0000..U+FFFF) count as 1, supplementary characters (U+10000+)
    // count as 2 (a surrogate pair).
    let target_utf16_units = (offset - 1) as usize;
    let mut utf16_units_consumed: usize = 0;
    let source_str = std::str::from_utf8(&source[byte_pos..]).ok()?;

    for ch in source_str.chars() {
        if utf16_units_consumed >= target_utf16_units {
            break;
        }
        utf16_units_consumed += ch.len_utf16();
        byte_pos += ch.len_utf8();
    }

    if byte_pos > source.len() {
        return None;
    }

    Some(byte_pos)
}

/// Builds a signature string from the node text and kind.
///
/// Since tsserver's navtree doesn't include full signature text,
/// we reconstruct a representative signature from available information.
fn build_signature(item: &NavTreeItem) -> String {
    let modifiers = if item.kind_modifiers.is_empty() {
        String::new()
    } else {
        format!("{} ", item.kind_modifiers)
    };

    match item.kind.as_str() {
        "function" => format!("{modifiers}function {}", item.text),
        "method" | "constructor" | "getter" | "setter" => {
            format!("{modifiers}{}", item.text)
        }
        "class" => format!("{modifiers}class {}", item.text),
        "interface" => format!("{modifiers}interface {}", item.text),
        "enum" => format!("{modifiers}enum {}", item.text),
        "type" => format!("{modifiers}type {}", item.text),
        "const" => format!("const {}", item.text),
        "let" => format!("let {}", item.text),
        "var" => format!("var {}", item.text),
        _ => item.text.clone(),
    }
}

/// Recursively maps a tsserver navigation tree into `SemanticSymbol` values.
///
/// Walks the navtree depth-first, tracking scope for qualified name construction.
/// Only items that map to a canonical `SymbolKind` are emitted.
pub fn map_navtree_to_symbols(items: &[NavTreeItem], source: &[u8]) -> Vec<SemanticSymbol> {
    let mut symbols = Vec::new();
    let mut scope_stack: Vec<String> = Vec::new();
    walk_navtree_items(items, source, &mut symbols, &mut scope_stack, None);
    symbols
}

fn walk_navtree_items(
    items: &[NavTreeItem],
    source: &[u8],
    symbols: &mut Vec<SemanticSymbol>,
    scope_stack: &mut Vec<String>,
    parent_kind: Option<&str>,
) {
    for item in items {
        let mapped_kind = map_ts_kind(&item.kind, parent_kind);

        if let Some(kind) = mapped_kind {
            // Skip items with empty or structural names.
            if item.text.trim().is_empty() || item.text == "<global>" {
                // Still recurse into children for scope tracking.
                walk_navtree_items(
                    &item.child_items,
                    source,
                    symbols,
                    scope_stack,
                    Some(&item.kind),
                );
                continue;
            }

            let span = item
                .spans
                .first()
                .and_then(|s| text_span_to_source_span(s, source));

            if let Some(span) = span {
                let qualified_name = build_qualified_name(scope_stack, &item.text);
                let parent = if scope_stack.is_empty() {
                    None
                } else {
                    Some(scope_stack.join("::"))
                };

                symbols.push(SemanticSymbol {
                    name: item.text.clone(),
                    qualified_name,
                    kind,
                    span,
                    signature: build_signature(item),
                    confidence_score: None,
                    docstring: None,
                    parent_qualified_name: parent,
                    type_refs: vec![],
                    call_refs: vec![],
                });
            }
        }

        // Push scope for class-like containers before recursing.
        let is_scope = matches!(
            item.kind.as_str(),
            "class" | "interface" | "enum" | "module"
        ) && !item.text.trim().is_empty()
            && item.text != "<global>";

        if is_scope {
            scope_stack.push(item.text.clone());
        }

        walk_navtree_items(
            &item.child_items,
            source,
            symbols,
            scope_stack,
            Some(&item.kind),
        );

        if is_scope {
            scope_stack.pop();
        }
    }
}

fn build_qualified_name(scope_stack: &[String], name: &str) -> String {
    if scope_stack.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", scope_stack.join("::"))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_span(start_line: u32, start_offset: u32, end_line: u32, end_offset: u32) -> TextSpan {
        TextSpan {
            start: SpanLocation {
                line: start_line,
                offset: start_offset,
            },
            end: SpanLocation {
                line: end_line,
                offset: end_offset,
            },
        }
    }

    #[test]
    fn maps_function_kind() {
        assert_eq!(map_ts_kind("function", None), Some(SymbolKind::Function));
    }

    #[test]
    fn maps_function_in_class_to_method() {
        assert_eq!(
            map_ts_kind("function", Some("class")),
            Some(SymbolKind::Method)
        );
    }

    #[test]
    fn maps_method_kind() {
        assert_eq!(map_ts_kind("method", None), Some(SymbolKind::Method));
    }

    #[test]
    fn maps_class_kind() {
        assert_eq!(map_ts_kind("class", None), Some(SymbolKind::Class));
    }

    #[test]
    fn maps_interface_to_type() {
        assert_eq!(map_ts_kind("interface", None), Some(SymbolKind::Type));
    }

    #[test]
    fn maps_enum_to_type() {
        assert_eq!(map_ts_kind("enum", None), Some(SymbolKind::Type));
    }

    #[test]
    fn maps_const_to_constant() {
        assert_eq!(map_ts_kind("const", None), Some(SymbolKind::Constant));
    }

    #[test]
    fn skips_module_kind() {
        assert_eq!(map_ts_kind("module", None), None);
    }

    #[test]
    fn skips_unknown_kind() {
        assert_eq!(map_ts_kind("something_else", None), None);
    }

    #[test]
    fn text_span_converts_to_source_span() {
        let source = b"const x = 1;\nfunction foo() {}\n";
        let span = make_span(2, 1, 2, 19);
        let result = text_span_to_source_span(&span, source).unwrap();
        assert_eq!(result.start_line, 2);
        assert_eq!(result.end_line, 2);
        assert_eq!(result.start_byte, 13); // byte offset of 'f' on line 2
        assert_eq!(result.byte_length, 18);
    }

    #[test]
    fn text_span_rejects_zero_line() {
        let source = b"hello\n";
        let span = make_span(0, 1, 1, 6);
        assert!(text_span_to_source_span(&span, source).is_none());
    }

    #[test]
    fn text_span_rejects_end_before_start() {
        let source = b"hello\nworld\n";
        let span = make_span(2, 1, 1, 1);
        assert!(text_span_to_source_span(&span, source).is_none());
    }

    #[test]
    fn line_offset_to_byte_first_line() {
        let source = b"const x = 1;\n";
        assert_eq!(line_offset_to_byte(source, 1, 1), Some(0));
        assert_eq!(line_offset_to_byte(source, 1, 7), Some(6));
    }

    #[test]
    fn line_offset_to_byte_second_line() {
        let source = b"line1\nline2\n";
        assert_eq!(line_offset_to_byte(source, 2, 1), Some(6));
        assert_eq!(line_offset_to_byte(source, 2, 3), Some(8));
    }

    #[test]
    fn line_offset_to_byte_rejects_zero() {
        let source = b"hello\n";
        assert_eq!(line_offset_to_byte(source, 0, 1), None);
        assert_eq!(line_offset_to_byte(source, 1, 0), None);
    }

    #[test]
    fn line_offset_to_byte_handles_bmp_multibyte() {
        // "café" — 'é' is U+00E9, 2 bytes in UTF-8, 1 UTF-16 code unit.
        // Bytes: c(1) a(1) f(1) é(2) = 5 bytes total.
        // tsserver sees: c=1, a=2, f=3, é=4 (4 UTF-16 code units).
        let source = "café\n".as_bytes();
        assert_eq!(source.len(), 6); // 5 bytes + newline
                                     // Offset 1 → byte 0 ('c')
        assert_eq!(line_offset_to_byte(source, 1, 1), Some(0));
        // Offset 4 → byte 3 ('é', which is 2 bytes at positions 3..5)
        assert_eq!(line_offset_to_byte(source, 1, 4), Some(3));
        // Offset 5 → byte 5 (past 'é', at newline)
        assert_eq!(line_offset_to_byte(source, 1, 5), Some(5));
    }

    #[test]
    fn line_offset_to_byte_handles_cjk() {
        // "const 名前 = 1;" — '名' and '前' are U+540D and U+524D,
        // each 3 bytes in UTF-8, 1 UTF-16 code unit.
        let source = "const 名前 = 1;\n".as_bytes();
        // "const " = 6 bytes, "名" = 3 bytes, "前" = 3 bytes, " = 1;\n" = 6 bytes
        assert_eq!(source.len(), 18);
        // tsserver offset 7 → '名' (UTF-16 units: c=1,o=2,n=3,s=4,t=5,' '=6,'名'=7)
        assert_eq!(line_offset_to_byte(source, 1, 7), Some(6));
        // tsserver offset 8 → '前'
        assert_eq!(line_offset_to_byte(source, 1, 8), Some(9));
        // tsserver offset 9 → ' ' after '前'
        assert_eq!(line_offset_to_byte(source, 1, 9), Some(12));
    }

    #[test]
    fn line_offset_to_byte_handles_supplementary_characters() {
        // "𝑓" is U+1D453 — 4 bytes in UTF-8, 2 UTF-16 code units (surrogate pair).
        // Source: "𝑓x\n" — 𝑓(4 bytes) + x(1 byte) + \n(1 byte) = 6 bytes.
        let source = "𝑓x\n".as_bytes();
        assert_eq!(source.len(), 6);
        // Offset 1 → byte 0 ('𝑓')
        assert_eq!(line_offset_to_byte(source, 1, 1), Some(0));
        // Offset 3 → byte 4 ('x'), because '𝑓' consumes 2 UTF-16 code units
        assert_eq!(line_offset_to_byte(source, 1, 3), Some(4));
    }

    #[test]
    fn text_span_correct_for_unicode_source() {
        // "const 名前 = 1;\nfunction foo() {}\n"
        // Line 2 starts at byte 18.
        // tsserver would report line 2, offset 1..19 for "function foo() {}".
        let source = "const 名前 = 1;\nfunction foo() {}\n".as_bytes();
        let span = make_span(2, 1, 2, 19);
        let result = text_span_to_source_span(&span, source).unwrap();
        assert_eq!(result.start_line, 2);
        assert_eq!(result.end_line, 2);
        assert_eq!(result.start_byte, 18);
        assert_eq!(result.byte_length, 18);
    }

    #[test]
    fn build_signature_function() {
        let item = NavTreeItem {
            text: "greet".to_string(),
            kind: "function".to_string(),
            kind_modifiers: "export".to_string(),
            spans: vec![],
            child_items: vec![],
        };
        assert_eq!(build_signature(&item), "export function greet");
    }

    #[test]
    fn build_signature_class() {
        let item = NavTreeItem {
            text: "MyClass".to_string(),
            kind: "class".to_string(),
            kind_modifiers: "".to_string(),
            spans: vec![],
            child_items: vec![],
        };
        assert_eq!(build_signature(&item), "class MyClass");
    }

    #[test]
    fn maps_flat_navtree() {
        let source = b"function greet() {}\nconst PI = 3.14;\n";
        let items = vec![
            NavTreeItem {
                text: "greet".to_string(),
                kind: "function".to_string(),
                kind_modifiers: String::new(),
                spans: vec![make_span(1, 1, 1, 20)],
                child_items: vec![],
            },
            NavTreeItem {
                text: "PI".to_string(),
                kind: "const".to_string(),
                kind_modifiers: String::new(),
                spans: vec![make_span(2, 1, 2, 17)],
                child_items: vec![],
            },
        ];

        let symbols = map_navtree_to_symbols(&items, source);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "greet");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].qualified_name, "greet");
        assert!(symbols[0].parent_qualified_name.is_none());

        assert_eq!(symbols[1].name, "PI");
        assert_eq!(symbols[1].kind, SymbolKind::Constant);
    }

    #[test]
    fn maps_nested_class_methods() {
        let source = b"class Foo {\n  bar() {}\n  baz() {}\n}\n";
        let items = vec![NavTreeItem {
            text: "Foo".to_string(),
            kind: "class".to_string(),
            kind_modifiers: String::new(),
            spans: vec![make_span(1, 1, 4, 2)],
            child_items: vec![
                NavTreeItem {
                    text: "bar".to_string(),
                    kind: "method".to_string(),
                    kind_modifiers: String::new(),
                    spans: vec![make_span(2, 3, 2, 12)],
                    child_items: vec![],
                },
                NavTreeItem {
                    text: "baz".to_string(),
                    kind: "method".to_string(),
                    kind_modifiers: String::new(),
                    spans: vec![make_span(3, 3, 3, 12)],
                    child_items: vec![],
                },
            ],
        }];

        let symbols = map_navtree_to_symbols(&items, source);
        assert_eq!(symbols.len(), 3);

        assert_eq!(symbols[0].name, "Foo");
        assert_eq!(symbols[0].kind, SymbolKind::Class);

        assert_eq!(symbols[1].name, "bar");
        assert_eq!(symbols[1].kind, SymbolKind::Method);
        assert_eq!(symbols[1].qualified_name, "Foo::bar");
        assert_eq!(symbols[1].parent_qualified_name.as_deref(), Some("Foo"));

        assert_eq!(symbols[2].name, "baz");
        assert_eq!(symbols[2].qualified_name, "Foo::baz");
    }

    #[test]
    fn skips_global_and_structural_nodes() {
        let source = b"function hello() {}\n";
        let items = vec![NavTreeItem {
            text: "<global>".to_string(),
            kind: "module".to_string(),
            kind_modifiers: String::new(),
            spans: vec![make_span(1, 1, 1, 20)],
            child_items: vec![NavTreeItem {
                text: "hello".to_string(),
                kind: "function".to_string(),
                kind_modifiers: String::new(),
                spans: vec![make_span(1, 1, 1, 20)],
                child_items: vec![],
            }],
        }];

        let symbols = map_navtree_to_symbols(&items, source);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "hello");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn mapping_is_deterministic() {
        let source = b"class A {\n  method1() {}\n}\nfunction b() {}\n";
        let items = vec![
            NavTreeItem {
                text: "A".to_string(),
                kind: "class".to_string(),
                kind_modifiers: String::new(),
                spans: vec![make_span(1, 1, 3, 2)],
                child_items: vec![NavTreeItem {
                    text: "method1".to_string(),
                    kind: "method".to_string(),
                    kind_modifiers: String::new(),
                    spans: vec![make_span(2, 3, 2, 16)],
                    child_items: vec![],
                }],
            },
            NavTreeItem {
                text: "b".to_string(),
                kind: "function".to_string(),
                kind_modifiers: String::new(),
                spans: vec![make_span(4, 1, 4, 16)],
                child_items: vec![],
            },
        ];

        let run1 = map_navtree_to_symbols(&items, source);
        let run2 = map_navtree_to_symbols(&items, source);

        assert_eq!(run1.len(), run2.len());
        for (a, b) in run1.iter().zip(run2.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.qualified_name, b.qualified_name);
            assert_eq!(a.span, b.span);
            assert_eq!(a.signature, b.signature);
            assert_eq!(a.parent_qualified_name, b.parent_qualified_name);
        }
    }

    #[test]
    fn maps_interface_with_methods() {
        let source = b"interface Shape {\n  area(): number;\n}\n";
        let items = vec![NavTreeItem {
            text: "Shape".to_string(),
            kind: "interface".to_string(),
            kind_modifiers: String::new(),
            spans: vec![make_span(1, 1, 3, 2)],
            child_items: vec![NavTreeItem {
                text: "area".to_string(),
                kind: "method".to_string(),
                kind_modifiers: String::new(),
                spans: vec![make_span(2, 3, 2, 19)],
                child_items: vec![],
            }],
        }];

        let symbols = map_navtree_to_symbols(&items, source);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "Shape");
        assert_eq!(symbols[0].kind, SymbolKind::Type);
        assert_eq!(symbols[1].name, "area");
        assert_eq!(symbols[1].kind, SymbolKind::Method);
        assert_eq!(symbols[1].qualified_name, "Shape::area");
    }

    #[test]
    fn maps_enum_members_are_skipped() {
        // Enum members have kind "property" in tsserver, which we skip.
        let source = b"enum Color {\n  Red,\n  Green,\n}\n";
        let items = vec![NavTreeItem {
            text: "Color".to_string(),
            kind: "enum".to_string(),
            kind_modifiers: String::new(),
            spans: vec![make_span(1, 1, 4, 2)],
            child_items: vec![
                NavTreeItem {
                    text: "Red".to_string(),
                    kind: "property".to_string(),
                    kind_modifiers: String::new(),
                    spans: vec![make_span(2, 3, 2, 6)],
                    child_items: vec![],
                },
                NavTreeItem {
                    text: "Green".to_string(),
                    kind: "property".to_string(),
                    kind_modifiers: String::new(),
                    spans: vec![make_span(3, 3, 3, 8)],
                    child_items: vec![],
                },
            ],
        }];

        let symbols = map_navtree_to_symbols(&items, source);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Color");
        assert_eq!(symbols[0].kind, SymbolKind::Type);
    }

    #[test]
    fn constructor_maps_to_method() {
        assert_eq!(map_ts_kind("constructor", None), Some(SymbolKind::Method));
    }

    #[test]
    fn getter_setter_map_to_method() {
        assert_eq!(map_ts_kind("getter", None), Some(SymbolKind::Method));
        assert_eq!(map_ts_kind("setter", None), Some(SymbolKind::Method));
    }
}
