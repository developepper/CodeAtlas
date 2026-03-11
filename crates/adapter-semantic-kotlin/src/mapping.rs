use adapter_api::{ExtractedSymbol, SourceSpan};
use core_model::SymbolKind;
use serde::Deserialize;

/// A node in the Kotlin analysis bridge's navigation tree response.
///
/// Represents a single declaration in the file's hierarchical structure.
/// Each node may contain child items forming the scope tree.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KtNavTreeItem {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub modifiers: String,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub start_line: u32,
    #[serde(default)]
    pub end_line: u32,
    #[serde(default)]
    pub start_byte: u64,
    #[serde(default)]
    pub byte_length: u64,
    #[serde(default)]
    pub child_items: Vec<KtNavTreeItem>,
}

/// Maps a Kotlin PSI declaration kind to a canonical `SymbolKind`.
///
/// The analysis bridge reports Kotlin-specific kind strings based on
/// PSI element types. This function maps them to the canonical schema.
fn map_kt_kind(kt_kind: &str, parent_kind: Option<&str>) -> Option<SymbolKind> {
    match kt_kind {
        "fun" => {
            if is_class_like(parent_kind) {
                Some(SymbolKind::Method)
            } else {
                Some(SymbolKind::Function)
            }
        }
        "constructor" => Some(SymbolKind::Method),
        "class" | "object" => Some(SymbolKind::Class),
        "interface" | "enum" | "typealias" => Some(SymbolKind::Type),
        "val" | "var" | "const" => Some(SymbolKind::Constant),
        // Skip structural nodes that don't map to symbols.
        "package" | "import" | "companion" | "init" | "property" | "parameter" | "enum_entry" => {
            None
        }
        _ => None,
    }
}

/// Returns whether a parent kind represents a class-like container.
fn is_class_like(parent_kind: Option<&str>) -> bool {
    matches!(parent_kind, Some("class" | "interface" | "object" | "enum"))
}

/// Converts raw span fields from the bridge into a `SourceSpan`.
///
/// The Kotlin analysis bridge reports byte offsets directly (unlike
/// tsserver's UTF-16 offsets), so no encoding conversion is needed.
fn item_to_source_span(item: &KtNavTreeItem) -> Option<SourceSpan> {
    if item.start_line == 0 || item.end_line == 0 || item.end_line < item.start_line {
        return None;
    }
    if item.byte_length == 0 {
        return None;
    }

    Some(SourceSpan {
        start_line: item.start_line,
        end_line: item.end_line,
        start_byte: item.start_byte,
        byte_length: item.byte_length,
    })
}

/// Builds a signature string from the item's signature field or kind + name.
fn build_signature(item: &KtNavTreeItem) -> String {
    if let Some(ref sig) = item.signature {
        if !sig.is_empty() {
            return sig.clone();
        }
    }

    let modifiers = if item.modifiers.is_empty() {
        String::new()
    } else {
        format!("{} ", item.modifiers)
    };

    match item.kind.as_str() {
        "fun" => format!("{modifiers}fun {}", item.name),
        "constructor" => format!("{modifiers}constructor {}", item.name),
        "class" => format!("{modifiers}class {}", item.name),
        "object" => format!("{modifiers}object {}", item.name),
        "interface" => format!("{modifiers}interface {}", item.name),
        "enum" => format!("{modifiers}enum class {}", item.name),
        "typealias" => format!("{modifiers}typealias {}", item.name),
        "val" => format!("val {}", item.name),
        "var" => format!("var {}", item.name),
        "const" => format!("const val {}", item.name),
        _ => item.name.clone(),
    }
}

/// Recursively maps a Kotlin navigation tree into `ExtractedSymbol` values.
///
/// Walks the tree depth-first, tracking scope for qualified name construction.
/// Only items that map to a canonical `SymbolKind` are emitted.
pub fn map_kt_navtree_to_symbols(items: &[KtNavTreeItem]) -> Vec<ExtractedSymbol> {
    let mut symbols = Vec::new();
    let mut scope_stack: Vec<String> = Vec::new();
    walk_kt_items(items, &mut symbols, &mut scope_stack, None);
    symbols
}

fn walk_kt_items(
    items: &[KtNavTreeItem],
    symbols: &mut Vec<ExtractedSymbol>,
    scope_stack: &mut Vec<String>,
    parent_kind: Option<&str>,
) {
    for item in items {
        let mapped_kind = map_kt_kind(&item.kind, parent_kind);

        if let Some(kind) = mapped_kind {
            if item.name.trim().is_empty() {
                walk_kt_items(&item.child_items, symbols, scope_stack, Some(&item.kind));
                continue;
            }

            if let Some(span) = item_to_source_span(item) {
                let qualified_name = build_qualified_name(scope_stack, &item.name);
                let parent = if scope_stack.is_empty() {
                    None
                } else {
                    Some(scope_stack.join("::"))
                };

                symbols.push(ExtractedSymbol {
                    name: item.name.clone(),
                    qualified_name,
                    kind,
                    span,
                    signature: build_signature(item),
                    confidence_score: None,
                    docstring: None,
                    parent_qualified_name: parent,
                });
            }
        }

        // Push scope for class-like containers before recursing.
        let is_scope = matches!(
            item.kind.as_str(),
            "class" | "interface" | "enum" | "object"
        ) && !item.name.trim().is_empty();

        if is_scope {
            scope_stack.push(item.name.clone());
        }

        walk_kt_items(&item.child_items, symbols, scope_stack, Some(&item.kind));

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

    fn make_item(
        name: &str,
        kind: &str,
        start_line: u32,
        end_line: u32,
        start_byte: u64,
        byte_length: u64,
    ) -> KtNavTreeItem {
        KtNavTreeItem {
            name: name.to_string(),
            kind: kind.to_string(),
            modifiers: String::new(),
            signature: None,
            start_line,
            end_line,
            start_byte,
            byte_length,
            child_items: vec![],
        }
    }

    #[test]
    fn maps_fun_kind() {
        assert_eq!(map_kt_kind("fun", None), Some(SymbolKind::Function));
    }

    #[test]
    fn maps_fun_in_class_to_method() {
        assert_eq!(map_kt_kind("fun", Some("class")), Some(SymbolKind::Method));
    }

    #[test]
    fn maps_fun_in_interface_to_method() {
        assert_eq!(
            map_kt_kind("fun", Some("interface")),
            Some(SymbolKind::Method)
        );
    }

    #[test]
    fn maps_constructor_to_method() {
        assert_eq!(map_kt_kind("constructor", None), Some(SymbolKind::Method));
    }

    #[test]
    fn maps_class_kind() {
        assert_eq!(map_kt_kind("class", None), Some(SymbolKind::Class));
    }

    #[test]
    fn maps_object_to_class() {
        assert_eq!(map_kt_kind("object", None), Some(SymbolKind::Class));
    }

    #[test]
    fn maps_interface_to_type() {
        assert_eq!(map_kt_kind("interface", None), Some(SymbolKind::Type));
    }

    #[test]
    fn maps_enum_to_type() {
        assert_eq!(map_kt_kind("enum", None), Some(SymbolKind::Type));
    }

    #[test]
    fn maps_typealias_to_type() {
        assert_eq!(map_kt_kind("typealias", None), Some(SymbolKind::Type));
    }

    #[test]
    fn maps_val_to_constant() {
        assert_eq!(map_kt_kind("val", None), Some(SymbolKind::Constant));
    }

    #[test]
    fn maps_const_to_constant() {
        assert_eq!(map_kt_kind("const", None), Some(SymbolKind::Constant));
    }

    #[test]
    fn skips_package_kind() {
        assert_eq!(map_kt_kind("package", None), None);
    }

    #[test]
    fn skips_companion_kind() {
        assert_eq!(map_kt_kind("companion", None), None);
    }

    #[test]
    fn skips_unknown_kind() {
        assert_eq!(map_kt_kind("something_else", None), None);
    }

    #[test]
    fn item_to_source_span_valid() {
        let item = make_item("foo", "fun", 1, 3, 0, 50);
        let span = item_to_source_span(&item).unwrap();
        assert_eq!(span.start_line, 1);
        assert_eq!(span.end_line, 3);
        assert_eq!(span.start_byte, 0);
        assert_eq!(span.byte_length, 50);
    }

    #[test]
    fn item_to_source_span_rejects_zero_line() {
        let item = make_item("foo", "fun", 0, 1, 0, 10);
        assert!(item_to_source_span(&item).is_none());
    }

    #[test]
    fn item_to_source_span_rejects_zero_byte_length() {
        let item = make_item("foo", "fun", 1, 1, 0, 0);
        assert!(item_to_source_span(&item).is_none());
    }

    #[test]
    fn build_signature_fun() {
        let mut item = make_item("greet", "fun", 1, 1, 0, 10);
        item.modifiers = "public".to_string();
        assert_eq!(build_signature(&item), "public fun greet");
    }

    #[test]
    fn build_signature_uses_explicit_signature_field() {
        let mut item = make_item("greet", "fun", 1, 1, 0, 10);
        item.signature = Some("fun greet(name: String): String".to_string());
        assert_eq!(build_signature(&item), "fun greet(name: String): String");
    }

    #[test]
    fn build_signature_class() {
        let item = make_item("Config", "class", 1, 5, 0, 100);
        assert_eq!(build_signature(&item), "class Config");
    }

    #[test]
    fn build_signature_enum() {
        let item = make_item("Mode", "enum", 1, 4, 0, 80);
        assert_eq!(build_signature(&item), "enum class Mode");
    }

    #[test]
    fn build_signature_const() {
        let item = make_item("MAX_SIZE", "const", 1, 1, 0, 30);
        assert_eq!(build_signature(&item), "const val MAX_SIZE");
    }

    #[test]
    fn maps_flat_navtree() {
        let items = vec![
            make_item("greet", "fun", 1, 3, 0, 50),
            make_item("MAX_SIZE", "const", 5, 5, 52, 28),
        ];

        let symbols = map_kt_navtree_to_symbols(&items);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "greet");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].qualified_name, "greet");
        assert!(symbols[0].parent_qualified_name.is_none());

        assert_eq!(symbols[1].name, "MAX_SIZE");
        assert_eq!(symbols[1].kind, SymbolKind::Constant);
    }

    #[test]
    fn maps_nested_class_methods() {
        let items = vec![KtNavTreeItem {
            name: "Processor".to_string(),
            kind: "class".to_string(),
            modifiers: String::new(),
            signature: None,
            start_line: 1,
            end_line: 5,
            start_byte: 0,
            byte_length: 100,
            child_items: vec![
                make_item("process", "fun", 2, 4, 20, 60),
                make_item("reset", "fun", 5, 5, 80, 20),
            ],
        }];

        let symbols = map_kt_navtree_to_symbols(&items);
        assert_eq!(symbols.len(), 3);

        assert_eq!(symbols[0].name, "Processor");
        assert_eq!(symbols[0].kind, SymbolKind::Class);

        assert_eq!(symbols[1].name, "process");
        assert_eq!(symbols[1].kind, SymbolKind::Method);
        assert_eq!(symbols[1].qualified_name, "Processor::process");
        assert_eq!(
            symbols[1].parent_qualified_name.as_deref(),
            Some("Processor")
        );

        assert_eq!(symbols[2].name, "reset");
        assert_eq!(symbols[2].qualified_name, "Processor::reset");
    }

    #[test]
    fn maps_interface_with_methods() {
        let items = vec![KtNavTreeItem {
            name: "Repository".to_string(),
            kind: "interface".to_string(),
            modifiers: String::new(),
            signature: None,
            start_line: 1,
            end_line: 4,
            start_byte: 0,
            byte_length: 80,
            child_items: vec![make_item("findAll", "fun", 2, 2, 20, 30)],
        }];

        let symbols = map_kt_navtree_to_symbols(&items);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "Repository");
        assert_eq!(symbols[0].kind, SymbolKind::Type);
        assert_eq!(symbols[1].name, "findAll");
        assert_eq!(symbols[1].kind, SymbolKind::Method);
        assert_eq!(symbols[1].qualified_name, "Repository::findAll");
    }

    #[test]
    fn maps_object_as_class() {
        let items = vec![make_item("Singleton", "object", 1, 5, 0, 100)];
        let symbols = map_kt_navtree_to_symbols(&items);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].kind, SymbolKind::Class);
    }

    #[test]
    fn skips_companion_recurses_into_children() {
        let items = vec![KtNavTreeItem {
            name: "MyClass".to_string(),
            kind: "class".to_string(),
            modifiers: String::new(),
            signature: None,
            start_line: 1,
            end_line: 8,
            start_byte: 0,
            byte_length: 200,
            child_items: vec![KtNavTreeItem {
                name: "Companion".to_string(),
                kind: "companion".to_string(),
                modifiers: String::new(),
                signature: None,
                start_line: 3,
                end_line: 6,
                start_byte: 40,
                byte_length: 100,
                child_items: vec![make_item("create", "fun", 4, 5, 60, 40)],
            }],
        }];

        let symbols = map_kt_navtree_to_symbols(&items);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"MyClass"));
        assert!(names.contains(&"create"));
        // "Companion" itself should be skipped.
        assert!(!names.contains(&"Companion"));
    }

    #[test]
    fn enum_entries_are_skipped() {
        let items = vec![KtNavTreeItem {
            name: "Color".to_string(),
            kind: "enum".to_string(),
            modifiers: String::new(),
            signature: None,
            start_line: 1,
            end_line: 5,
            start_byte: 0,
            byte_length: 60,
            child_items: vec![
                KtNavTreeItem {
                    name: "Red".to_string(),
                    kind: "enum_entry".to_string(),
                    modifiers: String::new(),
                    signature: None,
                    start_line: 2,
                    end_line: 2,
                    start_byte: 20,
                    byte_length: 3,
                    child_items: vec![],
                },
                KtNavTreeItem {
                    name: "Green".to_string(),
                    kind: "enum_entry".to_string(),
                    modifiers: String::new(),
                    signature: None,
                    start_line: 3,
                    end_line: 3,
                    start_byte: 25,
                    byte_length: 5,
                    child_items: vec![],
                },
            ],
        }];

        let symbols = map_kt_navtree_to_symbols(&items);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Color");
        assert_eq!(symbols[0].kind, SymbolKind::Type);
    }

    #[test]
    fn mapping_is_deterministic() {
        let items = vec![
            KtNavTreeItem {
                name: "A".to_string(),
                kind: "class".to_string(),
                modifiers: String::new(),
                signature: None,
                start_line: 1,
                end_line: 3,
                start_byte: 0,
                byte_length: 50,
                child_items: vec![make_item("doIt", "fun", 2, 2, 10, 20)],
            },
            make_item("b", "fun", 5, 7, 52, 30),
        ];

        let run1 = map_kt_navtree_to_symbols(&items);
        let run2 = map_kt_navtree_to_symbols(&items);

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
}
