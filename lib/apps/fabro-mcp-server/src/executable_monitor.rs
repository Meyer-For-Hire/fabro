//! Detects when an upgrade replaces the executable that launched this MCP
//! server.
//!
//! MCP hosts can keep stdio servers alive for days. Without this check, an old
//! process keeps its old API response decoder after the `fabro` file on disk is
//! upgraded.

use std::fs::Metadata;
use std::path::{self, PathBuf};
use std::time::Duration;
use std::{env, fs, io};

use tokio::time::{self, Instant, MissedTickBehavior};

const CHECK_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) struct ExecutableMonitor {
    path:     PathBuf,
    identity: Identity,
}

impl ExecutableMonitor {
    pub(crate) fn current() -> io::Result<Self> {
        Self::new(invoked_executable_path()?)
    }

    fn new(path: PathBuf) -> io::Result<Self> {
        let identity = identity(&fs::metadata(&path)?);
        Ok(Self { path, identity })
    }

    pub(crate) async fn wait_until_replaced(self) {
        let mut interval = time::interval_at(Instant::now() + CHECK_INTERVAL, CHECK_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if self.was_replaced() {
                return;
            }
        }
    }

    /// Reads the identity synchronously. This is a `stat` of a page-cached
    /// inode once per second, so handing it to Tokio's blocking pool would
    /// cost more than the call itself and would keep a pool thread resident
    /// for the life of the server.
    fn was_replaced(&self) -> bool {
        !fs::metadata(&self.path).is_ok_and(|metadata| identity(&metadata) == self.identity)
    }
}

/// Identifies the file behind an executable path. Upgrades always swap a new
/// file into place — `fabro upgrade` renames over the old one and Homebrew
/// repoints a symlink — so the identity changes even though the path does not.
#[cfg(unix)]
type Identity = (u64, u64);

#[cfg(unix)]
fn identity(metadata: &Metadata) -> Identity {
    use std::os::unix::fs::MetadataExt as _;

    (metadata.dev(), metadata.ino())
}

#[cfg(not(unix))]
type Identity = (u64, Option<std::time::SystemTime>);

#[cfg(not(unix))]
fn identity(metadata: &Metadata) -> Identity {
    (metadata.len(), metadata.modified().ok())
}

/// Resolves the executable path to watch.
///
/// `argv[0]` wins when it carries a directory, because it names the path the
/// host actually launched, symlink included. MCP hosts normally launch a bare
/// `fabro` found on `PATH`, which leaves `current_exe`: it reports the symlink
/// on macOS, and the Homebrew symlink's own target on Linux.
fn invoked_executable_path() -> io::Result<PathBuf> {
    match env::args_os().next().map(PathBuf::from) {
        Some(invoked) if invoked.components().count() > 1 => path::absolute(invoked),
        _ => env::current_exe(),
    }
}

#[cfg(test)]
mod tests {
    use tokio::fs as async_fs;

    use super::*;

    #[tokio::test]
    async fn unchanged_executable_is_current() {
        let directory = tempfile::tempdir().expect("temp directory should exist");
        let executable = directory.path().join("fabro");
        async_fs::write(&executable, b"current")
            .await
            .expect("fixture executable should be written");
        let monitor = ExecutableMonitor::new(executable).unwrap();

        assert!(!monitor.was_replaced());
    }

    #[tokio::test]
    async fn atomic_executable_replacement_is_detected() {
        let directory = tempfile::tempdir().expect("temp directory should exist");
        let executable = directory.path().join("fabro");
        let replacement = directory.path().join("fabro-new");
        async_fs::write(&executable, b"old")
            .await
            .expect("old fixture executable should be written");
        async_fs::write(&replacement, b"new executable")
            .await
            .expect("new fixture executable should be written");
        let monitor = ExecutableMonitor::new(executable.clone()).unwrap();

        async_fs::rename(&replacement, &executable)
            .await
            .expect("fixture executable should be replaced");

        assert!(monitor.was_replaced());
    }

    #[tokio::test]
    async fn removed_executable_is_detected() {
        let directory = tempfile::tempdir().expect("temp directory should exist");
        let executable = directory.path().join("fabro");
        async_fs::write(&executable, b"current")
            .await
            .expect("fixture executable should be written");
        let monitor = ExecutableMonitor::new(executable.clone()).unwrap();

        async_fs::remove_file(executable)
            .await
            .expect("fixture executable should be removed");

        assert!(monitor.was_replaced());
    }
}
