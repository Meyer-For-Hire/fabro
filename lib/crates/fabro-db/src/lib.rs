use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context as _;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use tokio::fs;
use tracing::info;

pub type DbPool = sqlx::SqlitePool;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct Database {
    pool: DbPool,
    path: PathBuf,
}

impl Database {
    pub async fn connect(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.with_context(|| {
                format!("creating SQLite database directory {}", parent.display())
            })?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .with_context(|| format!("opening SQLite database {}", path.display()))?;

        Ok(Self {
            pool,
            path: path.to_path_buf(),
        })
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        self.snapshot_before_new_migrations()
            .await
            .context("snapshotting SQLite database before migrations")?;
        MIGRATOR
            .run(&self.pool)
            .await
            .context("running SQLite migrations")
    }

    /// Copy the database aside before applying migrations it has not seen.
    ///
    /// A binary downgrade after new migrations have been applied fails sqlx's
    /// startup validation (`migration N was previously applied but is missing
    /// in the resolved migrations`), so the snapshot written to
    /// [`pre_migration_snapshot_path`] is the operator's rollback artifact:
    /// stop the server, replace the database file with the snapshot (and
    /// delete any `-wal`/`-shm` siblings), and the previous binary boots
    /// again. Writes made after the upgrade are lost on rollback, as with any
    /// point-in-time restore.
    ///
    /// The snapshot is only taken when the database has applied migrations
    /// before (a fresh database has nothing worth preserving) and at least
    /// one bundled migration is pending, so the file always holds the state
    /// from immediately before the most recent schema change. Failing to
    /// write the snapshot fails the migration: no rollback artifact, no
    /// schema change.
    async fn snapshot_before_new_migrations(&self) -> anyhow::Result<()> {
        let applied = applied_migration_versions(&self.pool).await?;
        if applied.is_empty() {
            return Ok(());
        }
        if !MIGRATOR
            .iter()
            .any(|migration| !applied.contains(&migration.version))
        {
            return Ok(());
        }

        let snapshot_path = pre_migration_snapshot_path(&self.path);
        match fs::remove_file(&snapshot_path).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "removing stale pre-migration snapshot {}",
                        snapshot_path.display()
                    )
                });
            }
        }

        // VACUUM INTO produces a consistent single-file copy from the live
        // pool, so the snapshot needs no -wal/-shm siblings to restore.
        let snapshot_target = snapshot_path
            .to_str()
            .context("database path is not valid UTF-8")?;
        sqlx::query("VACUUM INTO ?")
            .bind(snapshot_target)
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!("writing pre-migration snapshot {}", snapshot_path.display())
            })?;
        set_private_permissions(&snapshot_path).await?;

        info!(
            database = %self.path.display(),
            snapshot = %snapshot_path.display(),
            "Snapshotted SQLite database before applying new migrations"
        );
        Ok(())
    }

    pub async fn health_check(&self) -> anyhow::Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .context("checking SQLite database health")?;
        Ok(())
    }

    pub fn pool(&self) -> &DbPool {
        &self.pool
    }

    pub fn clone_pool(&self) -> DbPool {
        self.pool.clone()
    }
}

/// Rollback artifact written by [`Database::migrate`] before applying new
/// migrations: the database file name with `.pre-migration.bak` appended,
/// next to the database.
pub fn pre_migration_snapshot_path(database_path: &Path) -> PathBuf {
    let mut file_name = database_path
        .file_name()
        .map_or_else(|| OsString::from("fabro.sqlite3"), OsString::from);
    file_name.push(".pre-migration.bak");
    database_path.with_file_name(file_name)
}

async fn applied_migration_versions(pool: &DbPool) -> anyhow::Result<HashSet<i64>> {
    let table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await
    .context("checking for the sqlx migrations table")?;
    if table_count == 0 {
        return Ok(HashSet::new());
    }
    let versions: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations")
        .fetch_all(pool)
        .await
        .context("listing applied migration versions")?;
    Ok(versions.into_iter().collect())
}

#[cfg(unix)]
async fn set_private_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .await
        .with_context(|| format!("setting permissions on {}", path.display()))
}

#[cfg(not(unix))]
async fn set_private_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}
