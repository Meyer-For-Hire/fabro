use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryRef {
    pub name:       String,
    #[serde(default)]
    pub origin_url: Option<String>,
    pub provider:   RepositoryProvider,
}

impl RepositoryRef {
    pub fn from_origin_and_source(
        origin_url: Option<String>,
        source_directory: Option<&str>,
    ) -> Self {
        Self {
            name: repository_name(origin_url.as_deref(), source_directory),
            provider: repository_provider(origin_url.as_deref()),
            origin_url,
        }
    }
}

/// A validated GitHub `owner/repo` coordinate.
///
/// Construction enforces GitHub's owner and repository name syntax on the
/// exact submitted bytes; no trimming, case folding, or other normalization
/// is performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepositorySlug {
    owner: String,
    repo:  String,
}

impl GitHubRepositorySlug {
    /// Validates `value` as a GitHub `owner/repo` slug, returning `None` when
    /// it is not exactly one `/`-separated pair of a valid owner and
    /// repository name.
    #[must_use]
    pub fn try_new(value: &str) -> Option<Self> {
        let (owner, repo) = value.split_once('/')?;
        if repo.contains('/') || !valid_github_owner(owner) || !valid_github_repo(repo) {
            return None;
        }
        Some(Self {
            owner: owner.to_string(),
            repo:  repo.to_string(),
        })
    }

    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }
}

fn valid_github_owner(value: &str) -> bool {
    if value.is_empty() || value.len() > 39 {
        return false;
    }
    let bytes = value.as_bytes();
    let first = bytes[0];
    let last = bytes[bytes.len() - 1];
    (first.is_ascii_alphanumeric() && last.is_ascii_alphanumeric())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

fn valid_github_repo(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Reports whether `value` is a safe GitHub git ref selector.
///
/// The grammar checks the exact untrimmed ASCII byte content; no
/// normalization is performed.
#[must_use]
pub fn is_valid_github_ref_selector(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value.trim() == value
        && !value.starts_with(['/', '-', '.'])
        && !value.ends_with(['/', '.'])
        && !has_lock_suffix(value)
        && value != "@"
        && !value.contains("..")
        && !value.contains("//")
        && !value.contains("@{")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
        && value
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !has_lock_suffix(part))
}

fn has_lock_suffix(value: &str) -> bool {
    value
        .rsplit_once('.')
        .is_some_and(|(_, extension)| extension == "lock")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryProvider {
    Github,
    Git,
    Unknown,
}

fn repository_provider(origin_url: Option<&str>) -> RepositoryProvider {
    let Some(origin) = origin_url.filter(|origin| !origin.trim().is_empty()) else {
        return RepositoryProvider::Unknown;
    };
    if is_github_origin(origin) {
        RepositoryProvider::Github
    } else {
        RepositoryProvider::Git
    }
}

fn is_github_origin(origin: &str) -> bool {
    origin.starts_with("git@github.com:")
        || origin.starts_with("https://github.com/")
        || origin.starts_with("http://github.com/")
        || origin.starts_with("ssh://git@github.com/")
}

fn repository_name(origin_url: Option<&str>, source_directory: Option<&str>) -> String {
    origin_url
        .and_then(repository_name_from_origin)
        .or_else(|| {
            source_directory
                .and_then(path_basename)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[expect(
    clippy::disallowed_types,
    reason = "Run summaries parse the origin only to extract an owner/repo label; raw URLs are not logged."
)]
fn repository_name_from_origin(origin: &str) -> Option<String> {
    if let Some(path) = origin
        .strip_prefix("git@")
        .and_then(|url| url.split_once(':').map(|(_, path)| path))
    {
        return repository_name_from_path(path).map(ToOwned::to_owned);
    }

    let parsed = url::Url::parse(origin).ok()?;
    let path = parsed.path().trim_matches('/');
    repository_name_from_path(path).map(ToOwned::to_owned)
}

fn repository_name_from_path(path: &str) -> Option<&str> {
    let stripped = path.strip_suffix(".git").unwrap_or(path);
    let mut segments = stripped.rsplit('/').filter(|segment| !segment.is_empty());
    let repo = segments.next()?;
    let owner = segments.next();
    if let Some(owner) = owner {
        let start = stripped.len() - owner.len() - repo.len() - 1;
        stripped.get(start..)
    } else {
        Some(repo)
    }
}

fn path_basename(path: &str) -> Option<&str> {
    path.rsplit(['/', '\\']).find(|segment| !segment.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_slugs_preserve_exact_parts() {
        let cases = [
            ("fabro-sh/fabro", "fabro-sh", "fabro"),
            ("owner/.github", "owner", ".github"),
        ];
        for (input, owner, repo) in cases {
            let slug = GitHubRepositorySlug::try_new(input).expect(input);
            assert_eq!(slug.owner(), owner, "{input}");
            assert_eq!(slug.repo(), repo, "{input}");
        }

        let max_owner = "a".repeat(39);
        let max_repo = "b".repeat(100);
        let boundary = format!("{max_owner}/{max_repo}");
        let slug = GitHubRepositorySlug::try_new(&boundary).expect("boundary-length slug");
        assert_eq!(slug.owner(), max_owner);
        assert_eq!(slug.repo(), max_repo);
    }

    #[test]
    fn invalid_slugs_are_rejected() {
        let cases = [
            "",
            "fabro",
            "not/github/slug",
            "/repo",
            "owner/",
            "-owner/repo",
            "owner-/repo",
            "own_er/repo",
            "owner/.",
            "owner/..",
            "owner/re po",
            "öwner/repo",
            "owner/rëpo",
        ];
        for input in cases {
            assert!(GitHubRepositorySlug::try_new(input).is_none(), "{input}");
        }

        let over_owner = format!("{}/repo", "a".repeat(40));
        let over_repo = format!("owner/{}", "b".repeat(101));
        assert!(GitHubRepositorySlug::try_new(&over_owner).is_none());
        assert!(GitHubRepositorySlug::try_new(&over_repo).is_none());
    }

    #[test]
    fn valid_ref_selectors_are_accepted() {
        let max = "a".repeat(255);
        let cases = [
            "main",
            "feature/release-1.2_rc",
            "refs/tags/v1.0.0",
            max.as_str(),
        ];
        for input in cases {
            assert!(is_valid_github_ref_selector(input), "{input}");
        }
    }

    #[test]
    fn invalid_ref_selectors_are_rejected() {
        let cases = [
            "",
            " main",
            "main ",
            "/main",
            "-main",
            ".main",
            "main/",
            "main.",
            "feature//x",
            "feature/.hidden",
            "main.lock",
            "branch.lock/x",
            "@",
            "a..b",
            "a@{b",
            "main;rm",
            "ma\tin",
            "mäin",
        ];
        for input in cases {
            assert!(!is_valid_github_ref_selector(input), "{input:?}");
        }

        let over = "a".repeat(256);
        assert!(!is_valid_github_ref_selector(&over));
    }
}
