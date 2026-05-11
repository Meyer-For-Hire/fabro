use serde::{Deserialize, Serialize};

/// Record of a pull request created for a workflow run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    #[serde(default = "github_provider")]
    pub provider:    String,
    pub html_url:    String,
    pub number:      u64,
    pub owner:       String,
    pub repo:        String,
    pub base_branch: String,
    pub head_branch: String,
    pub title:       String,
}

fn github_provider() -> String {
    "github".to_string()
}

/// GitHub user summary for a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestUser {
    pub login: String,
}

/// Git reference summary for a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
}

/// Fields mirrored directly from GitHub's pull request REST payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestGithubDetail {
    pub number:        u64,
    pub title:         String,
    pub body:          Option<String>,
    pub state:         String,
    pub draft:         bool,
    #[serde(default)]
    pub merged:        bool,
    #[serde(default)]
    pub merged_at:     Option<String>,
    pub mergeable:     Option<bool>,
    pub additions:     u64,
    pub deletions:     u64,
    pub changed_files: u64,
    pub html_url:      String,
    pub user:          PullRequestUser,
    pub head:          PullRequestRef,
    pub base:          PullRequestRef,
    pub created_at:    String,
    pub updated_at:    String,
}

/// Stored pull request record plus live GitHub fields, returned by the
/// `GET /runs/{id}/pull_request` endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestDetails {
    pub pull_request:  PullRequest,
    pub state:         String,
    pub draft:         bool,
    pub merged:        bool,
    pub merged_at:     Option<String>,
    pub mergeable:     Option<bool>,
    pub additions:     u64,
    pub deletions:     u64,
    pub changed_files: u64,
    pub comments:      u64,
    pub checks:        Vec<CheckRun>,
    pub author:        PullRequestUser,
    pub timestamps:    PullRequestTimestamps,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestTimestamps {
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRun {
    pub name:       String,
    pub status:     CheckRunStatus,
    pub conclusion: Option<String>,
    pub html_url:   Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckRunStatus {
    Queued,
    InProgress,
    Completed,
    Unknown,
}
