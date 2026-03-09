use std::fs;
use std::io::{self, Read};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Kotlin,
    Java,
    Python,
    Php,
    Go,
    Ruby,
    C,
    Cpp,
    CSharp,
    Swift,
    Shell,
    Json,
    Yaml,
    Toml,
    Markdown,
    Sql,
    Dockerfile,
    Unknown,
}

impl Language {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Kotlin => "kotlin",
            Self::Java => "java",
            Self::Python => "python",
            Self::Php => "php",
            Self::Go => "go",
            Self::Ruby => "ruby",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
            Self::Swift => "swift",
            Self::Shell => "shell",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Markdown => "markdown",
            Self::Sql => "sql",
            Self::Dockerfile => "dockerfile",
            Self::Unknown => "unknown",
        }
    }
}

pub fn detect_language_for_file(path: &Path) -> Result<Language, io::Error> {
    if let Some(language) = detect_by_filename(path) {
        return Ok(language);
    }
    if let Some(language) = detect_by_extension(path) {
        return Ok(language);
    }

    let content = read_file_head(path, 4 * 1024)?;
    Ok(detect_language(path, &content))
}

#[must_use]
pub fn detect_language(path: &Path, content: &[u8]) -> Language {
    if let Some(language) = detect_by_filename(path) {
        return language;
    }

    if let Some(language) = detect_by_extension(path) {
        return language;
    }

    if let Some(language) = detect_by_shebang(content) {
        return language;
    }

    if let Some(language) = detect_by_content(content) {
        return language;
    }

    Language::Unknown
}

fn detect_by_filename(path: &Path) -> Option<Language> {
    let name = path.file_name()?.to_str()?;
    match name {
        "Dockerfile" => Some(Language::Dockerfile),
        _ => None,
    }
}

fn detect_by_extension(path: &Path) -> Option<Language> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();

    let language = match ext.as_str() {
        "rs" => Language::Rust,
        "ts" | "tsx" => Language::TypeScript,
        "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
        "kt" | "kts" => Language::Kotlin,
        "java" => Language::Java,
        "py" => Language::Python,
        "php" | "phtml" => Language::Php,
        "go" => Language::Go,
        "rb" => Language::Ruby,
        // `.h` is ambiguous between C and C++; default to C deterministically.
        "c" | "h" => Language::C,
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => Language::Cpp,
        "cs" => Language::CSharp,
        "swift" => Language::Swift,
        "sh" | "bash" | "zsh" => Language::Shell,
        "json" => Language::Json,
        "yaml" | "yml" => Language::Yaml,
        "toml" => Language::Toml,
        "md" | "markdown" => Language::Markdown,
        "sql" => Language::Sql,
        _ => return None,
    };

    Some(language)
}

fn detect_by_shebang(content: &[u8]) -> Option<Language> {
    let text = std::str::from_utf8(content).ok()?;
    let first_line = text.lines().next()?.trim();
    if !first_line.starts_with("#!") {
        return None;
    }

    let interpreter = first_line.trim_start_matches("#!").trim();
    if interpreter.contains("python") {
        return Some(Language::Python);
    }
    if interpreter.contains("node") {
        return Some(Language::JavaScript);
    }
    if interpreter.ends_with("/bash")
        || interpreter.ends_with("/zsh")
        || interpreter.ends_with("/sh")
        || interpreter.ends_with(" bash")
        || interpreter.ends_with(" zsh")
        || interpreter.ends_with(" sh")
    {
        return Some(Language::Shell);
    }
    if interpreter.contains("php") {
        return Some(Language::Php);
    }

    None
}

fn detect_by_content(content: &[u8]) -> Option<Language> {
    let text = std::str::from_utf8(content).ok()?;
    let trimmed = text.trim();

    if trimmed.starts_with("<?php") {
        return Some(Language::Php);
    }

    if looks_like_json(trimmed) {
        return Some(Language::Json);
    }

    None
}

fn looks_like_json(trimmed: &str) -> bool {
    if trimmed.len() < 2 {
        return false;
    }

    (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
}

fn read_file_head(path: &Path, max_bytes: usize) -> Result<Vec<u8>, io::Error> {
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0_u8; max_bytes];
    let read = file.read(&mut buffer)?;
    buffer.truncate(read);
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{detect_language, Language};

    #[test]
    fn extension_mapping_is_deterministic() {
        let cases = [
            ("main.rs", Language::Rust),
            ("app.ts", Language::TypeScript),
            ("app.js", Language::JavaScript),
            ("Main.kt", Language::Kotlin),
            ("Main.java", Language::Java),
            ("script.py", Language::Python),
            ("index.php", Language::Php),
            ("mod.go", Language::Go),
            ("Gemfile.rb", Language::Ruby),
            ("main.c", Language::C),
            ("main.cpp", Language::Cpp),
            ("Program.cs", Language::CSharp),
            ("main.swift", Language::Swift),
            ("script.sh", Language::Shell),
            ("data.json", Language::Json),
            ("config.yaml", Language::Yaml),
            ("Cargo.toml", Language::Toml),
            ("README.md", Language::Markdown),
            ("query.sql", Language::Sql),
            ("Dockerfile", Language::Dockerfile),
        ];

        for (path, expected) in cases {
            let detected = detect_language(Path::new(path), b"");
            assert_eq!(detected, expected, "path={path}");
        }
    }

    #[test]
    fn shebang_detection_covers_unextensioned_scripts() {
        let py = detect_language(Path::new("run"), b"#!/usr/bin/env python\nprint('hi')\n");
        let js = detect_language(
            Path::new("run"),
            b"#!/usr/bin/env node\nconsole.log('hi')\n",
        );
        let sh = detect_language(Path::new("run"), b"#!/bin/bash\necho hi\n");
        assert_eq!(py, Language::Python);
        assert_eq!(js, Language::JavaScript);
        assert_eq!(sh, Language::Shell);
    }

    #[test]
    fn content_fallback_detects_php_and_json() {
        let php = detect_language(Path::new("snippet"), b"<?php echo 'hi';");
        let json = detect_language(Path::new("blob"), br#"{"ok":true}"#);
        assert_eq!(php, Language::Php);
        assert_eq!(json, Language::Json);
    }

    #[test]
    fn unknown_fallback_is_stable() {
        let detected = detect_language(Path::new("mystery.file"), b"hello world");
        assert_eq!(detected, Language::Unknown);
    }
}
