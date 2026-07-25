use std::borrow::Cow;
use std::path::Path;

const MATCH_OPTIONS: glob::MatchOptions = glob::MatchOptions {
    case_sensitive:              true,
    require_literal_separator:   true,
    require_literal_leading_dot: false,
};

/// A validated glob matched against normalized paths relative to a workspace
/// root.
///
/// Patterns always use `/` as their separator. `*` and `?` stay within one
/// path segment; `**` crosses directory boundaries.
#[derive(Clone, Debug)]
pub struct WorkspaceGlob {
    source:         String,
    pattern:        glob::Pattern,
    traversal_root: String,
}

impl WorkspaceGlob {
    pub fn try_new(source: &str) -> Result<Self, WorkspaceGlobError> {
        let source = strip_current_dir_prefix(source);
        if source.is_empty() {
            return Err(WorkspaceGlobError::Empty);
        }
        if is_absolute(source) {
            return Err(WorkspaceGlobError::Absolute {
                pattern: source.to_string(),
            });
        }
        if source.split('/').any(|segment| segment == "..") {
            return Err(WorkspaceGlobError::ParentTraversal {
                pattern: source.to_string(),
            });
        }

        let pattern =
            glob::Pattern::new(source).map_err(|source_error| WorkspaceGlobError::Syntax {
                pattern: source.to_string(),
                source:  source_error,
            })?;

        Ok(Self {
            source: source.to_string(),
            pattern,
            traversal_root: literal_traversal_root(source),
        })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn is_match(&self, relative_path: &str) -> bool {
        let relative_path = normalize_candidate(relative_path);
        let relative_path = strip_current_dir_prefix(&relative_path);
        !is_absolute(relative_path)
            && !relative_path.split('/').any(|segment| segment == "..")
            && self.pattern.matches_with(relative_path, MATCH_OPTIONS)
    }

    /// A literal directory prefix that can reduce traversal work.
    ///
    /// This is only an optimization. Callers must still apply
    /// [`Self::is_match`] to every returned candidate.
    #[must_use]
    pub fn traversal_root(&self) -> &str {
        &self.traversal_root
    }
}

/// A compiled union of [`WorkspaceGlob`] patterns.
#[derive(Clone, Debug, Default)]
pub struct WorkspaceGlobSet {
    patterns: Vec<WorkspaceGlob>,
}

impl WorkspaceGlobSet {
    pub fn try_new<I, S>(patterns: I) -> Result<Self, WorkspaceGlobError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let patterns = patterns
            .into_iter()
            .map(|pattern| WorkspaceGlob::try_new(pattern.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { patterns })
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    #[must_use]
    pub fn is_match(&self, relative_path: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| pattern.is_match(relative_path))
    }

    /// Return the smallest non-overlapping set of literal traversal roots.
    #[must_use]
    pub fn traversal_roots(&self) -> Vec<&str> {
        let mut roots = self
            .patterns
            .iter()
            .map(WorkspaceGlob::traversal_root)
            .collect::<Vec<_>>();
        roots.sort_by(|left, right| {
            segment_count(left)
                .cmp(&segment_count(right))
                .then_with(|| left.cmp(right))
        });
        roots.dedup();

        let mut selected: Vec<&str> = Vec::new();
        for root in roots {
            if !selected
                .iter()
                .any(|ancestor| is_same_or_ancestor(ancestor, root))
            {
                selected.push(root);
            }
        }
        selected
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceGlobError {
    #[error("workspace glob cannot be empty")]
    Empty,

    #[error("workspace glob must be relative: {pattern:?}")]
    Absolute { pattern: String },

    #[error("workspace glob cannot traverse to a parent directory: {pattern:?}")]
    ParentTraversal { pattern: String },

    #[error("invalid workspace glob {pattern:?}: {source}")]
    Syntax {
        pattern: String,
        #[source]
        source:  glob::PatternError,
    },
}

fn strip_current_dir_prefix(mut path: &str) -> &str {
    while let Some(stripped) = path.strip_prefix("./") {
        path = stripped;
    }
    path
}

fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with("//")
        || Path::new(path).is_absolute()
        || matches!(bytes, [drive, b':', ..] if drive.is_ascii_alphabetic())
}

fn literal_traversal_root(pattern: &str) -> String {
    let mut literal_segments = Vec::new();
    let mut saw_meta = false;

    for segment in pattern.split('/').filter(|segment| !segment.is_empty()) {
        if has_glob_meta(segment) {
            saw_meta = true;
            break;
        }
        literal_segments.push(segment);
    }

    if !saw_meta {
        literal_segments.pop();
    }
    literal_segments.join("/")
}

fn has_glob_meta(segment: &str) -> bool {
    segment
        .chars()
        .any(|character| matches!(character, '*' | '?' | '['))
}

fn segment_count(path: &str) -> usize {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .count()
}

fn is_same_or_ancestor(ancestor: &str, candidate: &str) -> bool {
    ancestor.is_empty()
        || ancestor == candidate
        || candidate
            .strip_prefix(ancestor)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn normalize_candidate(path: &str) -> Cow<'_, str> {
    #[cfg(windows)]
    {
        Cow::Owned(path.replace('\\', "/"))
    }
    #[cfg(not(windows))]
    {
        Cow::Borrowed(path)
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkspaceGlob, WorkspaceGlobError, WorkspaceGlobSet};

    #[test]
    fn workspace_glob_has_root_relative_segment_semantics() {
        let cases = [
            ("*.md", "README.md", true),
            ("*.md", "docs/README.md", false),
            ("**/*.md", "README.md", true),
            ("**/*.md", "docs/README.md", true),
            (".ai/reports/*.md", ".ai/reports/result.md", true),
            (".ai/reports/*.md", ".ai/reports/nested/result.md", false),
            (".ai/reports/**/*.md", ".ai/reports/nested/result.md", true),
            (
                ".ai/plans/????-??-??-*.md",
                ".ai/plans/2026-07-25-globbing.md",
                true,
            ),
            (".ai/plans/????-??-??-*.md", ".ai/plans/DRAFTING.md", false),
            ("*/SKILL.md", "rust/SKILL.md", true),
            ("*/SKILL.md", "rust/review/SKILL.md", false),
            ("src/[lm]ib.rs", "src/lib.rs", true),
            ("src/[!m]ib.rs", "src/lib.rs", true),
            ("**/.env", ".env", true),
            ("**/.env", "nested/.env", true),
        ];

        for (pattern, candidate, expected) in cases {
            let glob = WorkspaceGlob::try_new(pattern).unwrap();
            assert_eq!(
                glob.is_match(candidate),
                expected,
                "pattern {pattern:?}, candidate {candidate:?}"
            );
        }
    }

    #[test]
    fn workspace_glob_normalizes_a_leading_current_directory() {
        let glob = WorkspaceGlob::try_new("./src/*.rs").unwrap();

        assert_eq!(glob.as_str(), "src/*.rs");
        assert!(glob.is_match("./src/lib.rs"));
        assert_eq!(glob.traversal_root(), "src");
    }

    #[test]
    fn workspace_glob_rejects_paths_outside_the_root() {
        assert!(matches!(
            WorkspaceGlob::try_new(""),
            Err(WorkspaceGlobError::Empty)
        ));
        assert!(matches!(
            WorkspaceGlob::try_new("/tmp/*.md"),
            Err(WorkspaceGlobError::Absolute { .. })
        ));
        assert!(matches!(
            WorkspaceGlob::try_new("C:/tmp/*.md"),
            Err(WorkspaceGlobError::Absolute { .. })
        ));
        assert!(matches!(
            WorkspaceGlob::try_new("../*.md"),
            Err(WorkspaceGlobError::ParentTraversal { .. })
        ));
        assert!(matches!(
            WorkspaceGlob::try_new("src/[abc"),
            Err(WorkspaceGlobError::Syntax { .. })
        ));
        assert!(matches!(
            WorkspaceGlob::try_new("src**/*.rs"),
            Err(WorkspaceGlobError::Syntax { .. })
        ));
    }

    #[test]
    fn workspace_glob_set_minimizes_traversal_roots() {
        let globs = WorkspaceGlobSet::try_new([
            ".ai/reports/*.md",
            ".ai/reports/nested/*.json",
            ".ai/plans/*.md",
            ".other/targeted.txt",
        ])
        .unwrap();

        assert_eq!(globs.traversal_roots(), vec![
            ".other",
            ".ai/plans",
            ".ai/reports"
        ]);
    }

    #[test]
    fn recursive_pattern_supersedes_narrower_traversal_roots() {
        let globs = WorkspaceGlobSet::try_new(["**/*.md", ".ai/reports/*.json"]).unwrap();

        assert_eq!(globs.traversal_roots(), vec![""]);
        assert!(globs.is_match(".ai/reports/result.md"));
        assert!(globs.is_match(".ai/reports/result.json"));
        assert!(!globs.is_match(".ai/reports/result.txt"));
    }
}
