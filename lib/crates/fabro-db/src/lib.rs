use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context as _;
use chrono::{DateTime, Utc};
use sqlx::migrate::{Migrate as _, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use tokio::fs;
#[cfg(unix)]
use tokio::task::spawn_blocking;
use tracing::info;

pub mod legacy;

pub type DbPool = sqlx::SqlitePool;

/// Parse an RFC 3339 TEXT column value into a UTC timestamp.
pub fn parse_rfc3339_utc(value: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    DateTime::parse_from_rfc3339(value).map(|timestamp| timestamp.with_timezone(&Utc))
}

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct Database {
    pool: DbPool,
}

impl Database {
    pub async fn connect(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.with_context(|| {
                format!("creating SQLite database directory {}", parent.display())
            })?;
        }

        prepare_private_database_file(path).await?;

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

        Ok(Self { pool })
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
        let has_pending = MIGRATOR
            .iter()
            .any(|migration| !applied.contains(&migration.version));
        if applied.is_empty() || !has_pending {
            return Ok(());
        }

        let connect_options = self.pool.connect_options();
        let database_path = connect_options.get_filename();
        let snapshot_path = pre_migration_snapshot_path(database_path);

        // VACUUM INTO produces a consistent single-file copy from the live
        // pool, so the snapshot needs no -wal/-shm siblings to restore. It
        // writes to a staging file that is renamed into place afterwards, so
        // a failure mid-copy never leaves a partial file at the snapshot
        // path.
        let staging_path = append_to_path(&snapshot_path, ".tmp");
        remove_file_if_exists(&staging_path)
            .await
            .with_context(|| {
                format!(
                    "removing stale snapshot staging file {}",
                    staging_path.display()
                )
            })?;
        let staging_target = staging_path
            .to_str()
            .context("snapshot staging path is not valid UTF-8")?;
        sqlx::query("VACUUM INTO ?")
            .bind(staging_target)
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!("writing pre-migration snapshot {}", staging_path.display())
            })?;
        set_private_permissions(&staging_path).await?;
        remove_file_if_exists(&snapshot_path)
            .await
            .with_context(|| {
                format!(
                    "removing stale pre-migration snapshot {}",
                    snapshot_path.display()
                )
            })?;
        fs::rename(&staging_path, &snapshot_path)
            .await
            .with_context(|| {
                format!(
                    "publishing pre-migration snapshot {}",
                    snapshot_path.display()
                )
            })?;

        info!(
            database = %database_path.display(),
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
/// migrations: the database path with `.pre-migration.bak` appended.
pub fn pre_migration_snapshot_path(database_path: &Path) -> PathBuf {
    append_to_path(database_path, ".pre-migration.bak")
}

fn append_to_path(path: &Path, suffix: &str) -> PathBuf {
    let mut path = path.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}

async fn applied_migration_versions(pool: &DbPool) -> anyhow::Result<HashSet<i64>> {
    let mut conn = pool
        .acquire()
        .await
        .context("acquiring a SQLite connection")?;
    // Migrator::run performs this same ensure + list as its first step, so
    // asking the Migrate trait (rather than querying sqlx's bookkeeping
    // table by hand) cannot drift from what it will actually apply.
    conn.ensure_migrations_table(&MIGRATOR.table_name)
        .await
        .context("ensuring the sqlx migrations table exists")?;
    let applied = conn
        .list_applied_migrations(&MIGRATOR.table_name)
        .await
        .context("listing applied migration versions")?;
    Ok(applied
        .into_iter()
        .map(|migration| migration.version)
        .collect())
}

async fn remove_file_if_exists(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
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

#[cfg(unix)]
#[expect(
    clippy::disallowed_methods,
    reason = "SQLite file permissions must be established synchronously before opening the pool"
)]
async fn prepare_private_database_file(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};

    let path = path.to_path_buf();
    spawn_blocking(move || {
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("creating private SQLite database {}", path.display()))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("setting private SQLite permissions on {}", path.display()))
    })
    .await
    .context("joining SQLite permission setup task")?
}

#[cfg(not(unix))]
async fn prepare_private_database_file(path: &Path) -> anyhow::Result<()> {
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .await
        .with_context(|| format!("creating SQLite database {}", path.display()))?;
    Ok(())
}
