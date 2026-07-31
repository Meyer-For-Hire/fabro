//! Detects when an upgrade replaces the executable that launched this MCP
//! server.
//!
//! MCP hosts can keep stdio servers alive for days. Without this check, an old
//! process keeps its old API response decoder after the `fabro` file on disk is
//! upgraded.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use std::{env, io};

use fabro_static::EnvVars;
use tokio::time::Instant;
use tokio::{fs, time};

const CHECK_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) struct ExecutableMonitor {
    path:     PathBuf,
    identity: ExecutableIdentity,
}

impl ExecutableMonitor {
    pub(crate) async fn current() -> io::Result<Self> {
        let path = invoked_executable_path().await?;
        Self::new(path).await
    }

    async fn new(path: PathBuf) -> io::Result<Self> {
        let identity = ExecutableIdentity::from_metadata(&fs::metadata(&path).await?);
        Ok(Self { path, identity })
    }

    pub(crate) async fn wait_until_replaced(self) {
        let mut interval = time::interval_at(Instant::now() + CHECK_INTERVAL, CHECK_INTERVAL);
        loop {
            interval.tick().await;
            if self.was_replaced().await {
                return;
            }
        }
    }

    async fn was_replaced(&self) -> bool {
        fs::metadata(&self.path).await.map_or(true, |metadata| {
            ExecutableIdentity::from_metadata(&metadata) != self.identity
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExecutableIdentity {
    len:      u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    device:   u64,
    #[cfg(unix)]
    inode:    u64,
}

impl ExecutableIdentity {
    fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt as _;

        Self {
            len:                 metadata.len(),
            modified:            metadata.modified().ok(),
            #[cfg(unix)]
            device:              metadata.dev(),
            #[cfg(unix)]
            inode:               metadata.ino(),
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "MCP startup resolves its invoked executable through the process PATH so it can detect Homebrew symlink updates"
)]
async fn invoked_executable_path() -> io::Result<PathBuf> {
    let invoked = env::args_os()
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "process argv[0] is unavailable"))?;

    if invoked.components().count() > 1 {
        return absolute_path(invoked);
    }

    if let Some(path) = env::var_os(EnvVars::PATH) {
        for directory in env::split_paths(&path) {
            let candidate = absolute_path(directory.join(&invoked))?;
            if fs::metadata(&candidate)
                .await
                .is_ok_and(|metadata| is_executable_file(&metadata))
            {
                return Ok(candidate);
            }
        }
    }

    env::current_exe()
}

fn absolute_path(path: PathBuf) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        env::current_dir().map(|cwd| cwd.join(path))
    }
}

fn is_executable_file(metadata: &std::fs::Metadata) -> bool {
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unchanged_executable_is_current() {
        let directory = tempfile::tempdir().expect("temp directory should exist");
        let executable = directory.path().join("fabro");
        fs::write(&executable, b"current")
            .await
            .expect("fixture executable should be written");
        let monitor = ExecutableMonitor::new(executable).await.unwrap();

        assert!(!monitor.was_replaced().await);
    }

    #[tokio::test]
    async fn atomic_executable_replacement_is_detected() {
        let directory = tempfile::tempdir().expect("temp directory should exist");
        let executable = directory.path().join("fabro");
        let replacement = directory.path().join("fabro-new");
        fs::write(&executable, b"old")
            .await
            .expect("old fixture executable should be written");
        fs::write(&replacement, b"new executable")
            .await
            .expect("new fixture executable should be written");
        let monitor = ExecutableMonitor::new(executable.clone()).await.unwrap();

        fs::rename(&replacement, &executable)
            .await
            .expect("fixture executable should be replaced");

        assert!(monitor.was_replaced().await);
    }

    #[tokio::test]
    async fn removed_executable_is_detected() {
        let directory = tempfile::tempdir().expect("temp directory should exist");
        let executable = directory.path().join("fabro");
        fs::write(&executable, b"current")
            .await
            .expect("fixture executable should be written");
        let monitor = ExecutableMonitor::new(executable.clone()).await.unwrap();

        fs::remove_file(executable)
            .await
            .expect("fixture executable should be removed");

        assert!(monitor.was_replaced().await);
    }

    #[test]
    fn executable_check_rejects_directories() {
        let directory = tempfile::tempdir().expect("temp directory should exist");
        let metadata = std::fs::metadata(directory.path()).unwrap();

        assert!(!is_executable_file(&metadata));
    }
}
