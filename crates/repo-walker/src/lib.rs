use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobMatcher};
use ignore::WalkBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct WalkerOptions {
    pub extra_ignore_rules: Vec<String>,
    pub include_git_dir: bool,
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

        if !options.include_git_dir && starts_with_git_dir(&relative) {
            continue;
        }

        if is_ignored_by_extra_rules(&relative, &extra_rules) {
            continue;
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
    use super::{compile_ignore_rules, is_ignored_by_extra_rules};
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
}
