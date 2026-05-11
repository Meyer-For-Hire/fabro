use std::path::PathBuf;
use std::sync::Arc;

#[cfg(feature = "docker")]
use anyhow::Context as _;
#[cfg(any(feature = "docker", feature = "daytona"))]
use fabro_github::GitHubCredentials;
#[allow(
    unused_imports,
    reason = "Daytona-enabled builds persist RunId in the sandbox spec."
)]
use fabro_types::{RunId, RunSandbox, RunSandboxRuntime, SandboxProvider};

#[cfg(any(feature = "docker", feature = "daytona"))]
use crate::clone_source;
#[cfg(feature = "daytona")]
use crate::daytona::{DaytonaConfig, DaytonaSandbox, DaytonaSnapshotConfig};
#[cfg(feature = "docker")]
use crate::docker::{DockerSandbox, DockerSandboxOptions};
use crate::local::LocalSandbox;
use crate::{Sandbox, SandboxEventCallback};

/// Options for sandbox initialization and construction.
pub enum SandboxSpec {
    Local {
        working_directory: PathBuf,
    },
    #[cfg(feature = "docker")]
    Docker {
        config:           DockerSandboxOptions,
        github_app:       Option<GitHubCredentials>,
        run_id:           Option<RunId>,
        clone_origin_url: Option<String>,
        clone_branch:     Option<String>,
    },
    #[cfg(feature = "daytona")]
    Daytona {
        config:           Box<DaytonaConfig>,
        github_app:       Option<GitHubCredentials>,
        run_id:           Option<RunId>,
        clone_origin_url: Option<String>,
        clone_branch:     Option<String>,
        api_key:          Option<String>,
    },
}

impl SandboxSpec {
    pub fn provider(&self) -> SandboxProvider {
        match self {
            Self::Local { .. } => SandboxProvider::Local,
            #[cfg(feature = "docker")]
            Self::Docker { .. } => SandboxProvider::Docker,
            #[cfg(feature = "daytona")]
            Self::Daytona { .. } => SandboxProvider::Daytona,
        }
    }

    pub fn provider_name(&self) -> &'static str {
        match self.provider() {
            SandboxProvider::Local => "local",
            SandboxProvider::Docker => "docker",
            SandboxProvider::Daytona => "daytona",
        }
    }

    /// Build a RunSandbox for persistence.
    pub fn to_run_sandbox(&self, sandbox: &dyn Sandbox, run_id: RunId) -> RunSandbox {
        let working_directory = sandbox.working_directory().to_string();
        let id = {
            let info = sandbox.sandbox_info();
            if info.is_empty() {
                format!("local:{run_id}")
            } else {
                info
            }
        };

        match self {
            #[cfg(feature = "docker")]
            Self::Docker {
                config,
                clone_origin_url,
                clone_branch,
                ..
            } => RunSandbox {
                provider: self.provider(),
                image:    (!config.image.is_empty()).then(|| config.image.clone()),
                snapshot: None,
                runtime:  Some(RunSandboxRuntime {
                    id,
                    working_directory: working_directory.clone(),
                    repo_cloned: clone_source::repo_cloned_for_record(
                        config.skip_clone,
                        clone_origin_url.as_deref(),
                    ),
                    clone_origin_url: clone_source::clean_clone_origin_for_record(
                        clone_origin_url.as_deref(),
                    ),
                    clone_branch: clone_branch.clone(),
                }),
            },
            #[cfg(feature = "daytona")]
            Self::Daytona {
                config,
                clone_origin_url,
                clone_branch,
                ..
            } => RunSandbox {
                provider: self.provider(),
                image:    None,
                snapshot: config
                    .snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.name.clone()),
                runtime:  Some(RunSandboxRuntime {
                    id,
                    working_directory: working_directory.clone(),
                    repo_cloned: clone_source::repo_cloned_for_record(
                        config.skip_clone,
                        clone_origin_url.as_deref(),
                    ),
                    clone_origin_url: clone_source::clean_clone_origin_for_record(
                        clone_origin_url.as_deref(),
                    ),
                    clone_branch: clone_branch.clone(),
                }),
            },
            _ => RunSandbox {
                provider: self.provider(),
                image:    None,
                snapshot: None,
                runtime:  Some(RunSandboxRuntime {
                    id,
                    working_directory,
                    repo_cloned: None,
                    clone_origin_url: None,
                    clone_branch: None,
                }),
            },
        }
    }

    /// Apply devcontainer snapshot config. Only Daytona uses this.
    #[cfg(feature = "daytona")]
    pub fn apply_devcontainer_snapshot(&mut self, snapshot: DaytonaSnapshotConfig) {
        if let Self::Daytona { config, .. } = self {
            config.snapshot = Some(snapshot);
        }
    }

    #[allow(
        clippy::unused_async,
        reason = "Only Daytona construction awaits; local and Docker builds share the async API."
    )]
    pub async fn build(
        &self,
        event_callback: Option<SandboxEventCallback>,
    ) -> Result<Arc<dyn Sandbox>, anyhow::Error> {
        match self {
            Self::Local { working_directory } => {
                let mut sandbox = LocalSandbox::new(working_directory.clone());
                if let Some(callback) = event_callback {
                    sandbox.set_event_callback(callback);
                }
                Ok(Arc::new(sandbox))
            }
            #[cfg(feature = "docker")]
            Self::Docker {
                config,
                github_app,
                run_id,
                clone_origin_url,
                clone_branch,
            } => {
                let mut sandbox = DockerSandbox::new(
                    config.clone(),
                    github_app.clone(),
                    *run_id,
                    clone_origin_url.clone(),
                    clone_branch.clone(),
                )
                .context("Failed to create Docker sandbox")?;
                if let Some(callback) = event_callback {
                    sandbox.set_event_callback(callback);
                }
                Ok(Arc::new(sandbox))
            }
            #[cfg(feature = "daytona")]
            Self::Daytona {
                config,
                github_app,
                run_id,
                clone_origin_url,
                clone_branch,
                api_key,
            } => {
                let mut sandbox = DaytonaSandbox::new(
                    config.as_ref().clone(),
                    github_app.clone(),
                    *run_id,
                    clone_origin_url.clone(),
                    clone_branch.clone(),
                    api_key.clone(),
                )
                .await
                .map_err(anyhow::Error::new)?;
                if let Some(callback) = event_callback {
                    sandbox.set_event_callback(callback);
                }
                Ok(Arc::new(sandbox))
            }
        }
    }
}
