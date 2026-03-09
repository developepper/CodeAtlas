use std::error::Error;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobMatcher};
use ignore::WalkBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct WalkerOptions {
    pub extra_ignore_rules: Vec<String>,
    pub include_git_dir: bool,
    pub max_file_size_bytes: Option<u64>,
    pub max_file_count: Option<usize>,
    pub skip_binary_files: bool,
}

impl Default for WalkerOptions {
    fn default() -> Self {
        Self {
            extra_ignore_rules: Vec::new(),
            include_git_dir: false,
            max_file_size_bytes: None,
            max_file_count: None,
            skip_binary_files: true,
        }
    }
}

#[derive(Debug)]
pub enum WalkError {
    InvalidRoot {
        path: PathBuf,
        reason: &'static str,
    },
    InvalidIgnoreRule {
        rule: String,
        reason: String,
    },
    LimitExceeded {
        kind: &'static str,
        limit: usize,
    },
    Io {
        path: Option<PathBuf>,
        source: std::io::Error,
    },
}

impl fmt::Display for WalkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoot { path, reason } => {
                write!(f, "invalid repository root '{}': {reason}", path.display())
            }
            Self::InvalidIgnoreRule { rule, reason } => {
                write!(f, "invalid ignore rule '{rule}': {reason}")
            }
            Self::LimitExceeded { kind, limit } => {
                write!(f, "configured {kind} limit exceeded: {limit}")
            }
            Self::Io { path, source } => {
                if let Some(path) = path {
                    write!(f, "I/O error at '{}': {source}", path.display())
                } else {
                    write!(f, "I/O error: {source}")
                }
            }
        }
    }
}

impl Error for WalkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct IgnoreRule {
    matcher: GlobMatcher,
    is_ignore: bool,
}

pub fn walk_repository(
    root: &Path,
    options: &WalkerOptions,
) -> Result<Vec<DiscoveredFile>, WalkError> {
    ensure_root_is_valid(root)?;
    let root = fs::canonicalize(root).map_err(|source| WalkError::Io {
        path: Some(root.to_path_buf()),
        source,
    })?;

    let extra_rules = compile_ignore_rules(&options.extra_ignore_rules)?;
    let mut builder = WalkBuilder::new(&root);
    builder.hidden(false);
    builder.git_ignore(true);
    builder.git_global(false);
    builder.git_exclude(false);
    builder.ignore(true);
    builder.require_git(false);
    builder.parents(true);
    builder.follow_links(false);

    let mut discovered = Vec::new();

    for entry in builder.build() {
        let entry = entry.map_err(|err| WalkError::Io {
            path: None,
            source: std::io::Error::other(err.to_string()),
        })?;

        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }

        let absolute = entry.path().to_path_buf();
        let relative = absolute
            .strip_prefix(&root)
            .map_err(|source| WalkError::Io {
                path: Some(absolute.clone()),
                source: std::io::Error::other(source.to_string()),
            })?
            .to_path_buf();
        validate_relative_path(&relative)?;

        if !options.include_git_dir && starts_with_git_dir(&relative) {
            continue;
        }

        // `follow_links(false)` prevents descending into symlinked directories,
        // but symlink entries can still be yielded as files. Explicitly drop them.
        let metadata = symlink_metadata(&absolute)?;
        if metadata.file_type().is_symlink() {
            continue;
        }

        if is_ignored_by_extra_rules(&relative, &extra_rules) {
            continue;
        }

        if exceeds_size_cap(metadata.len(), options.max_file_size_bytes) {
            continue;
        }

        if options.skip_binary_files && is_probably_binary_file(&absolute)? {
            continue;
        }

        if let Some(limit) = options.max_file_count {
            if discovered.len() >= limit {
                return Err(WalkError::LimitExceeded {
                    kind: "file_count",
                    limit,
                });
            }
        }

        discovered.push(DiscoveredFile {
            relative_path: relative,
            absolute_path: absolute,
        });
    }

    discovered.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(discovered)
}

fn ensure_root_is_valid(root: &Path) -> Result<(), WalkError> {
    if !root.exists() {
        return Err(WalkError::InvalidRoot {
            path: root.to_path_buf(),
            reason: "path does not exist",
        });
    }
    if !root.is_dir() {
        return Err(WalkError::InvalidRoot {
            path: root.to_path_buf(),
            reason: "path is not a directory",
        });
    }

    Ok(())
}

fn compile_ignore_rules(rules: &[String]) -> Result<Vec<IgnoreRule>, WalkError> {
    let mut compiled = Vec::new();

    for rule in rules {
        let trimmed = rule.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let (is_ignore, pattern) = if let Some(pattern) = trimmed.strip_prefix('!') {
            (false, pattern.trim())
        } else {
            (true, trimmed)
        };

        if pattern.is_empty() {
            return Err(WalkError::InvalidIgnoreRule {
                rule: rule.clone(),
                reason: "pattern is empty".to_string(),
            });
        }

        let normalized_pattern = normalize_pattern(pattern);
        let glob = Glob::new(&normalized_pattern).map_err(|err| WalkError::InvalidIgnoreRule {
            rule: rule.clone(),
            reason: err.to_string(),
        })?;

        compiled.push(IgnoreRule {
            matcher: glob.compile_matcher(),
            is_ignore,
        });
    }

    Ok(compiled)
}

fn normalize_pattern(pattern: &str) -> String {
    let mut normalized = pattern.replace('\\', "/");
    let anchored_to_root = normalized.starts_with('/');
    if anchored_to_root {
        normalized = normalized.trim_start_matches('/').to_string();
    }

    if normalized.ends_with('/') {
        normalized.push_str("**");
    }

    let has_separator = normalized.contains('/');
    if !anchored_to_root && !has_separator {
        normalized = format!("**/{normalized}");
    }

    normalized
}

fn starts_with_git_dir(path: &Path) -> bool {
    path.components()
        .next()
        .is_some_and(|component| component.as_os_str() == ".git")
}

fn validate_relative_path(path: &Path) -> Result<(), WalkError> {
    for component in path.components() {
        match component {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(WalkError::Io {
                    path: Some(path.to_path_buf()),
                    source: std::io::Error::other("detected unsafe traversal component"),
                });
            }
            std::path::Component::CurDir | std::path::Component::Normal(_) => {}
        }
    }
    Ok(())
}

fn symlink_metadata(path: &Path) -> Result<fs::Metadata, WalkError> {
    fs::symlink_metadata(path).map_err(|source| WalkError::Io {
        path: Some(path.to_path_buf()),
        source,
    })
}

fn exceeds_size_cap(file_size: u64, max_file_size_bytes: Option<u64>) -> bool {
    let Some(limit) = max_file_size_bytes else {
        return false;
    };
    file_size > limit
}

fn is_probably_binary_file(path: &Path) -> Result<bool, WalkError> {
    const SAMPLE_BYTES: usize = 8 * 1024;
    const KNOWN_BINARY_EXTENSIONS: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "pdf", "zip", "gz", "tar", "7z", "exe",
        "dll", "so", "dylib", "class", "jar", "woff", "woff2",
    ];

    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        let ext = extension.to_ascii_lowercase();
        if KNOWN_BINARY_EXTENSIONS.contains(&ext.as_str()) {
            return Ok(true);
        }
    }

    let mut file = fs::File::open(path).map_err(|source| WalkError::Io {
        path: Some(path.to_path_buf()),
        source,
    })?;
    let mut buffer = [0_u8; SAMPLE_BYTES];
    let read = file.read(&mut buffer).map_err(|source| WalkError::Io {
        path: Some(path.to_path_buf()),
        source,
    })?;

    Ok(buffer[..read].contains(&0))
}

fn is_ignored_by_extra_rules(path: &Path, rules: &[IgnoreRule]) -> bool {
    let candidate = path.to_string_lossy().replace('\\', "/");
    let mut ignored = false;

    for rule in rules {
        if rule.matcher.is_match(&candidate) {
            ignored = rule.is_ignore;
        }
    }

    ignored
}

#[cfg(test)]
mod tests {
    use super::{
        compile_ignore_rules, exceeds_size_cap, is_ignored_by_extra_rules, validate_relative_path,
    };
    use std::path::Path;

    #[test]
    fn extra_ignore_rules_support_negation_and_ordering() {
        let rules = compile_ignore_rules(&[
            "src/**".to_string(),
            "!src/main.rs".to_string(),
            "src/generated/**".to_string(),
        ])
        .expect("compile rules");

        assert!(!is_ignored_by_extra_rules(Path::new("src/main.rs"), &rules));
        assert!(is_ignored_by_extra_rules(Path::new("src/lib.rs"), &rules));
        assert!(is_ignored_by_extra_rules(
            Path::new("src/generated/file.rs"),
            &rules
        ));
    }

    #[test]
    fn empty_or_comment_rules_are_ignored() {
        let rules = compile_ignore_rules(&[
            "".to_string(),
            "   ".to_string(),
            "# ignore comments".to_string(),
        ])
        .expect("compile rules");

        assert!(rules.is_empty());
    }

    #[test]
    fn invalid_ignore_rule_is_rejected() {
        let err = compile_ignore_rules(&["[".to_string()]).expect_err("invalid rule should fail");
        assert!(err.to_string().contains("invalid ignore rule"));
    }

    #[test]
    fn basename_ignore_rule_matches_nested_paths() {
        let rules = compile_ignore_rules(&["*.log".to_string()]).expect("compile rules");
        assert!(is_ignored_by_extra_rules(Path::new("logs/run.log"), &rules));
        assert!(is_ignored_by_extra_rules(Path::new("run.log"), &rules));
    }

    #[test]
    fn traversal_components_are_rejected() {
        let err = validate_relative_path(Path::new("../outside.txt"))
            .expect_err("path traversal should fail");
        assert!(err.to_string().contains("unsafe traversal"));
    }

    #[test]
    fn size_cap_is_strictly_greater_than_limit() {
        assert!(!exceeds_size_cap(5, Some(5)));
        assert!(exceeds_size_cap(6, Some(5)));
    }
}
