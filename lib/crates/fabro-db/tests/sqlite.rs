use sqlx::Row as _;

#[tokio::test]
async fn connect_creates_parent_directory_and_migrate_is_idempotent() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("nested").join("fabro.sqlite3");

    let database = fabro_db::Database::connect(&db_path).await?;
    database.migrate().await?;
    database.migrate().await?;
    database.health_check().await?;

    assert!(db_path.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        assert_eq!(
            std::fs::metadata(&db_path)?.permissions().mode() & 0o777,
            0o600
        );
    }
    let variable_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'variables'",
    )
    .fetch_one(database.pool())
    .await?;
    assert_eq!(variable_table_count, 1);

    let environments_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'environments'",
    )
    .fetch_one(database.pool())
    .await?;
    assert_eq!(environments_table_count, 1);

    let secrets_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'secrets'",
    )
    .fetch_one(database.pool())
    .await?;
    assert_eq!(secrets_table_count, 1);

    let legacy_import_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'legacy_imports'",
    )
    .fetch_one(database.pool())
    .await?;
    assert_eq!(legacy_import_table_count, 0);

    let foreign_keys: i64 = sqlx::query("PRAGMA foreign_keys")
        .fetch_one(database.pool())
        .await?
        .get(0);
    assert_eq!(foreign_keys, 1);

    Ok(())
}

#[tokio::test]
async fn environments_schema_rejects_invalid_rows() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let database = fabro_db::Database::connect(dir.path().join("fabro.sqlite3")).await?;
    database.migrate().await?;

    insert_minimal_environment(database.pool(), "valid", "docker", "allow_all").await?;

    for (id, provider, network_mode) in [
        ("Bad", "docker", "allow_all"),
        ("local", "docker", "allow_all"),
        ("bad-provider", "bogus", "allow_all"),
        ("bad-network", "docker", "bogus"),
    ] {
        let result = insert_minimal_environment(database.pool(), id, provider, network_mode).await;
        assert!(
            result.is_err(),
            "environment row should be rejected: id={id}, provider={provider}, network_mode={network_mode}"
        );
    }

    Ok(())
}

async fn insert_minimal_environment(
    pool: &fabro_db::DbPool,
    id: &str,
    provider: &str,
    network_mode: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO environments (
            id,
            revision,
            provider,
            network_mode,
            lifecycle_preserve,
            lifecycle_stop_on_terminal
        )
        VALUES (?, ?, ?, ?, ?, ?)
        ",
    )
    .bind(id)
    .bind("a".repeat(64))
    .bind(provider)
    .bind(network_mode)
    .bind(false)
    .bind(true)
    .execute(pool)
    .await?;

    Ok(())
}

#[tokio::test]
async fn variables_schema_enforces_env_style_names() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let database = fabro_db::Database::connect(dir.path().join("fabro.sqlite3")).await?;
    database.migrate().await?;

    sqlx::query("INSERT INTO variables (name, value, created_at, updated_at) VALUES (?, ?, ?, ?)")
        .bind("OK_123")
        .bind("")
        .bind("2026-06-30T00:00:00Z")
        .bind("2026-06-30T00:00:00Z")
        .execute(database.pool())
        .await?;

    let invalid = sqlx::query(
        "INSERT INTO variables (name, value, created_at, updated_at) VALUES (?, ?, ?, ?)",
    )
    .bind("1BAD")
    .bind("value")
    .bind("2026-06-30T00:00:00Z")
    .bind("2026-06-30T00:00:00Z")
    .execute(database.pool())
    .await;
    assert!(invalid.is_err());

    Ok(())
}

#[tokio::test]
async fn fresh_database_migrate_takes_no_snapshot() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("fabro.sqlite3");

    let database = fabro_db::Database::connect(&db_path).await?;
    database.migrate().await?;

    assert!(
        !fabro_db::pre_migration_snapshot_path(&db_path).exists(),
        "a fresh database has no pre-migration state worth snapshotting"
    );
    Ok(())
}

// Simulates a binary upgrade: a database whose `_sqlx_migrations` table is
// missing an entry for a bundled migration is exactly what an older binary
// leaves behind for a newer one. The environments migration is pure CREATE
// TABLE, so dropping the table and deleting its version row makes it pending
// again without violating checksums.
#[tokio::test]
async fn migrate_snapshots_database_before_applying_new_migrations() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("fabro.sqlite3");
    let snapshot_path = fabro_db::pre_migration_snapshot_path(&db_path);

    let database = fabro_db::Database::connect(&db_path).await?;
    database.migrate().await?;
    sqlx::query(
        "INSERT INTO variables (name, value, created_at, updated_at) \
         VALUES ('SNAPSHOT_MARKER', 'kept', '2026-07-22T00:00:00Z', '2026-07-22T00:00:00Z')",
    )
    .execute(database.pool())
    .await?;
    sqlx::query("DROP TABLE environments")
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 2026063002")
        .execute(database.pool())
        .await?;

    database.migrate().await?;

    assert!(
        snapshot_path.exists(),
        "pending migration must snapshot first"
    );
    let snapshot = connect_read_only(&snapshot_path).await?;
    assert!(
        !table_exists(&snapshot, "environments").await?,
        "snapshot must hold the pre-migration schema"
    );
    let snapshot_marker: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM variables WHERE name = 'SNAPSHOT_MARKER'")
            .fetch_one(&snapshot)
            .await?;
    assert_eq!(snapshot_marker, 1, "snapshot must preserve row data");
    snapshot.close().await;

    assert!(
        table_exists(database.pool(), "environments").await?,
        "migration must still apply"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&snapshot_path)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "snapshot must be private");
    }

    // With nothing pending, migrate must not rewrite the snapshot: it still
    // holds the state from before the most recent schema change.
    database.migrate().await?;
    let snapshot = connect_read_only(&snapshot_path).await?;
    assert!(
        !table_exists(&snapshot, "environments").await?,
        "no-pending migrate must leave the snapshot untouched"
    );
    snapshot.close().await;
    Ok(())
}

async fn connect_read_only(path: &std::path::Path) -> anyhow::Result<sqlx::SqlitePool> {
    Ok(sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=ro", path.display())).await?)
}

async fn table_exists(pool: &sqlx::SqlitePool, table: &str) -> anyhow::Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await?;
    Ok(count == 1)
}
