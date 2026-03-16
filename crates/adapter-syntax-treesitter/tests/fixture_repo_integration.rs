#![allow(deprecated)]

use std::fs;
use std::path::PathBuf;

use adapter_api::{AdapterOutput, IndexContext, LanguageAdapter, SourceFile};
use adapter_syntax_treesitter::create_adapter;
use core_model::SymbolKind;
use tempfile::TempDir;

/// Helper: create a temporary fixture repository with Rust source files.
struct FixtureRepo {
    _tempdir: TempDir,
    root: PathBuf,
}

impl FixtureRepo {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let root = tempdir.path().to_path_buf();
        Self {
            _tempdir: tempdir,
            root,
        }
    }

    fn write(&self, rel: &str, contents: &str) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create dirs");
        }
        fs::write(&path, contents).expect("write file");
    }

    fn index_file(&self, rel: &str) -> AdapterOutput {
        let adapter = create_adapter("rust").expect("rust adapter");
        let ctx = IndexContext {
            repo_id: "fixture-repo".to_string(),
            source_root: self.root.clone(),
        };
        let abs = self.root.join(rel);
        let content = fs::read(&abs).expect("read file");
        let file = SourceFile {
            relative_path: PathBuf::from(rel),
            absolute_path: abs,
            content,
            language: "rust".to_string(),
        };
        adapter.index_file(&ctx, &file).expect("index file")
    }
}

// ---------------------------------------------------------------------------
// Fixture: realistic Rust project layout
// ---------------------------------------------------------------------------

const LIB_RS: &str = r#"//! Crate-level docs.

/// A 2D point.
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    /// Creates a new point.
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Euclidean distance from origin.
    pub fn distance(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

/// Shape enumeration.
pub enum Shape {
    Circle(f64),
    Rect { w: f64, h: f64 },
}

pub trait Area {
    fn area(&self) -> f64;
}

pub type Meters = f64;

pub const DEFAULT_ORIGIN: Point = Point { x: 0.0, y: 0.0 };
"#;

const MAIN_RS: &str = r#"use lib::Point;

fn main() {
    let p = Point::new(3.0, 4.0);
    println!("distance = {}", p.distance());
}
"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn fixture_repo_lib_extracts_expected_symbols() {
    let repo = FixtureRepo::new();
    repo.write("src/lib.rs", LIB_RS);

    let output = repo.index_file("src/lib.rs");

    let names: Vec<(&str, SymbolKind)> = output
        .symbols
        .iter()
        .map(|s| (s.name.as_str(), s.kind))
        .collect();

    assert!(names.contains(&("Point", SymbolKind::Class)));
    assert!(names.contains(&("new", SymbolKind::Method)));
    assert!(names.contains(&("distance", SymbolKind::Method)));
    assert!(names.contains(&("Shape", SymbolKind::Type)));
    assert!(names.contains(&("Area", SymbolKind::Type)));
    assert!(names.contains(&("area", SymbolKind::Method)));
    assert!(names.contains(&("Meters", SymbolKind::Type)));
    assert!(names.contains(&("DEFAULT_ORIGIN", SymbolKind::Constant)));
}

#[test]
fn fixture_repo_main_extracts_main_function() {
    let repo = FixtureRepo::new();
    repo.write("src/main.rs", MAIN_RS);

    let output = repo.index_file("src/main.rs");

    assert_eq!(output.symbols.len(), 1);
    assert_eq!(output.symbols[0].name, "main");
    assert_eq!(output.symbols[0].kind, SymbolKind::Function);
    assert_eq!(output.symbols[0].qualified_name, "main");
}

#[test]
fn fixture_repo_output_provenance_is_self_describing() {
    let repo = FixtureRepo::new();
    repo.write("src/lib.rs", LIB_RS);

    let output = repo.index_file("src/lib.rs");

    assert_eq!(output.source_adapter, "syntax-treesitter-rust");
    assert_eq!(output.quality_level, core_model::QualityLevel::Syntax);

    // Every symbol must have a resolved confidence score.
    for sym in &output.symbols {
        let score = sym
            .confidence_score
            .unwrap_or_else(|| panic!("symbol '{}' missing confidence", sym.name));
        assert!(
            (0.0..=1.0).contains(&score),
            "symbol '{}' confidence {score} out of range",
            sym.name
        );
    }
}

#[test]
fn fixture_repo_extraction_is_deterministic_across_files() {
    let repo = FixtureRepo::new();
    repo.write("src/lib.rs", LIB_RS);
    repo.write("src/main.rs", MAIN_RS);

    let lib1 = repo.index_file("src/lib.rs");
    let lib2 = repo.index_file("src/lib.rs");
    let main1 = repo.index_file("src/main.rs");
    let main2 = repo.index_file("src/main.rs");

    assert_eq!(lib1.symbols.len(), lib2.symbols.len());
    assert_eq!(main1.symbols.len(), main2.symbols.len());

    for (a, b) in lib1.symbols.iter().zip(lib2.symbols.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.span, b.span);
    }
}

#[test]
fn fixture_repo_empty_file_produces_no_symbols() {
    let repo = FixtureRepo::new();
    repo.write("src/empty.rs", "");

    let output = repo.index_file("src/empty.rs");
    assert!(output.symbols.is_empty());
    // Provenance still present even with no symbols.
    assert_eq!(output.source_adapter, "syntax-treesitter-rust");
}

#[test]
fn fixture_repo_qualified_names_reflect_impl_scope() {
    let repo = FixtureRepo::new();
    repo.write("src/lib.rs", LIB_RS);

    let output = repo.index_file("src/lib.rs");

    let new_sym = output
        .symbols
        .iter()
        .find(|s| s.name == "new")
        .expect("find 'new'");
    assert_eq!(new_sym.qualified_name, "Point::new");
    assert_eq!(new_sym.parent_qualified_name.as_deref(), Some("Point"));

    let area_sym = output
        .symbols
        .iter()
        .find(|s| s.name == "area")
        .expect("find 'area'");
    assert_eq!(area_sym.qualified_name, "Area::area");
    assert_eq!(area_sym.parent_qualified_name.as_deref(), Some("Area"));
}
