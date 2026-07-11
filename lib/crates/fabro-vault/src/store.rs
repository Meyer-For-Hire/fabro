use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::str::FromStr as _;

use chrono::{DateTime, Utc};
use fabro_db::{Database, DbPool};
use fabro_types::{OAuthCredential, SecretMetadata, SecretType};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row as _, Sqlite, Transaction};
use tokio::fs;
use tracing::info;

use crate::{SecretEntry, Vault};

#[derive(Debug, thiserror::Error)]
pub enum SecretStoreError {
    #[error("invalid secret name: {0}")]
    InvalidName(String),

    #[error("secret not found: {0}")]
    NotFound(String),

    #[error("secret revision is stale for {name}: expected {expected}, actual {actual}")]
    StaleRevision {
        name:     String,
        expected: i64,
        actual:   i64,
    },

    #[error("secret {name} is not valid OAuth JSON")]
    InvalidOauth {
        name:   String,
        #[source]
        source: serde_json::Error,
    },

    #[error("database error")]
    Db(#[from] sqlx::Error),

    #[error("stored secret {name} has invalid type {value:?}")]
    StoredType { name: String, value: String },

    #[error("stored secret has invalid name {name:?}")]
    StoredName { name: String },

    #[error("stored secret {name} has invalid revision {revision}")]
    StoredRevision { name: String, revision: i64 },

    #[error("stored secret {name} is not a valid OAuth credential")]
    StoredOauth {
        name:   String,
        #[source]
        source: serde_json::Error,
    },

    #[error("parsing secret timestamp for {name}.{column}")]
    Timestamp {
        name:   String,
        column: &'static str,
        #[source]
        source: chrono::ParseError,
    },

    #[error("reading legacy secrets file {path}")]
    LegacyRead {
        path:   PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("parsing legacy secrets file {path}")]
    LegacyParse {
        path:   PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("legacy secrets file {path} contains invalid secret {name}")]
    LegacyInvalid { path: PathBuf, name: String },

    #[error("renaming legacy secrets file {source_path} to backup {backup_path}")]
    LegacyBackup {
        source_path: PathBuf,
        backup_path: PathBuf,
        #[source]
        source:      std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub source_path:   PathBuf,
    pub backup_path:   PathBuf,
    pub imported_rows: usize,
    pub skipped_rows:  usize,
    pub secret_names:  Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretStoreWrite {
    pub name:        String,
    pub value:       String,
    pub secret_type: SecretType,
    pub description: Option<String>,
}

impl std::fmt::Debug for SecretStoreWrite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretStoreWrite")
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .field("secret_type", &self.secret_type)
            .field("description", &self.description)
            .finish()
    }
}

#[derive(Clone)]
pub struct SecretStore {
    pool: DbPool,
}

#[derive(Clone)]
pub struct SecretSnapshot(Vault);

impl SecretSnapshot {
    #[must_use]
    pub fn into_vault(self) -> Vault {
        self.0
    }
}

impl std::fmt::Debug for SecretSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::ops::Deref for SecretSnapshot {
    type Target = Vault;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Debug for SecretStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretStore").finish_non_exhaustive()
    }
}

impl SecretStore {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Opens the Fabro database at `sqlite_path`, runs migrations, imports any
    /// legacy secrets JSON at `legacy_secrets_path`, and returns the store.
    pub async fn open(
        sqlite_path: impl AsRef<Path>,
        legacy_secrets_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let database = Database::connect(sqlite_path).await?;
        database.migrate().await?;
        import_legacy_json_once(database.pool(), legacy_secrets_path).await?;
        Ok(Self::new(database.clone_pool()))
    }

    pub async fn get(&self, name: &str) -> Result<Option<SecretEntry>, SecretStoreError> {
        let row = sqlx::query(
            "SELECT name, secret_type, value, description, revision, created_at, updated_at \
             FROM secrets WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref()
            .map(entry_from_row)
            .transpose()
            .map(|entry| entry.map(|(_, entry)| entry))
    }

    pub async fn list(&self) -> Result<Vec<SecretMetadata>, SecretStoreError> {
        let rows = sqlx::query(
            "SELECT name, secret_type, description, created_at, updated_at \
             FROM secrets ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(metadata_from_row).collect()
    }

    pub async fn set(
        &self,
        name: &str,
        value: &str,
        secret_type: SecretType,
        description: Option<&str>,
    ) -> Result<SecretMetadata, SecretStoreError> {
        validate_write(name, value, secret_type)?;
        let now = Utc::now().to_rfc3339();
        let mut transaction = self.pool.begin().await?;
        let metadata = upsert_secret(
            &mut transaction,
            name,
            value,
            secret_type,
            description,
            &now,
        )
        .await?;
        transaction.commit().await?;
        Ok(metadata)
    }

    pub async fn remove(&self, name: &str) -> Result<(), SecretStoreError> {
        let result = sqlx::query("DELETE FROM secrets WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(SecretStoreError::NotFound(name.to_string()));
        }
        Ok(())
    }

    pub async fn apply(
        &self,
        removals: &[String],
        writes: &[SecretStoreWrite],
    ) -> Result<(), SecretStoreError> {
        for write in writes {
            validate_write(&write.name, &write.value, write.secret_type)?;
        }

        let mut transaction = self.pool.begin().await?;
        for name in removals {
            sqlx::query("DELETE FROM secrets WHERE name = ?")
                .bind(name)
                .execute(&mut *transaction)
                .await?;
        }
        let now = Utc::now().to_rfc3339();
        for write in writes {
            upsert_secret(
                &mut transaction,
                &write.name,
                &write.value,
                write.secret_type,
                write.description.as_deref(),
                &now,
            )
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn replace_if_revision(
        &self,
        name: &str,
        expected_revision: i64,
        value: &str,
        secret_type: SecretType,
    ) -> Result<SecretEntry, SecretStoreError> {
        validate_write(name, value, secret_type)?;
        let now = Utc::now().to_rfc3339();
        let row = sqlx::query(
            r"
            UPDATE secrets SET
                secret_type = ?,
                value = ?,
                revision = revision + 1,
                updated_at = ?
            WHERE name = ? AND revision = ?
            RETURNING name, secret_type, value, description, revision, created_at, updated_at
            ",
        )
        .bind(secret_type.as_str())
        .bind(value)
        .bind(now)
        .bind(name)
        .bind(expected_revision)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            return entry_from_row(&row).map(|(_, entry)| entry);
        }
        match self.get(name).await? {
            Some(entry) => Err(SecretStoreError::StaleRevision {
                name:     name.to_string(),
                expected: expected_revision,
                actual:   entry.revision,
            }),
            None => Err(SecretStoreError::NotFound(name.to_string())),
        }
    }

    pub async fn snapshot(&self) -> Result<SecretSnapshot, SecretStoreError> {
        let rows = sqlx::query(
            "SELECT name, secret_type, value, description, revision, created_at, updated_at \
             FROM secrets",
        )
        .fetch_all(&self.pool)
        .await?;
        let entries = rows
            .iter()
            .map(entry_from_row)
            .collect::<Result<HashMap<_, _>, _>>()?;
        Ok(SecretSnapshot(Vault::from_entries(entries)))
    }
}

async fn upsert_secret(
    transaction: &mut Transaction<'_, Sqlite>,
    name: &str,
    value: &str,
    secret_type: SecretType,
    description: Option<&str>,
    now: &str,
) -> Result<SecretMetadata, SecretStoreError> {
    let row = sqlx::query(
        r"
        INSERT INTO secrets (
            name, secret_type, value, description, revision, created_at, updated_at
        ) VALUES (?, ?, ?, ?, 1, ?, ?)
        ON CONFLICT(name) DO UPDATE SET
            secret_type = excluded.secret_type,
            value = excluded.value,
            description = COALESCE(excluded.description, secrets.description),
            revision = secrets.revision + 1,
            updated_at = excluded.updated_at
        RETURNING name, secret_type, description, created_at, updated_at
        ",
    )
    .bind(name)
    .bind(secret_type.as_str())
    .bind(value)
    .bind(description)
    .bind(now)
    .bind(now)
    .fetch_one(&mut **transaction)
    .await?;
    metadata_from_row(&row)
}

pub async fn import_legacy_json_once(
    pool: &DbPool,
    source_path: impl AsRef<Path>,
) -> Result<Option<ImportReport>, SecretStoreError> {
    let source_path = source_path.as_ref();
    let contents = match fs::read_to_string(source_path).await {
        Ok(contents) => contents,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(SecretStoreError::LegacyRead {
                path: source_path.to_path_buf(),
                source,
            });
        }
    };
    let entries: HashMap<String, SecretEntry> =
        serde_json::from_str(&contents).map_err(|source| SecretStoreError::LegacyParse {
            path: source_path.to_path_buf(),
            source,
        })?;
    let mut names = entries.keys().cloned().collect::<Vec<_>>();
    names.sort();
    for name in &names {
        let entry = &entries[name];
        if Vault::validate_name(name, entry.secret_type).is_err()
            || (entry.secret_type == SecretType::Oauth
                && validate_oauth_json(&entry.value).is_err())
        {
            return Err(SecretStoreError::LegacyInvalid {
                path: source_path.to_path_buf(),
                name: name.clone(),
            });
        }
    }

    let mut transaction = pool.begin().await?;
    let mut imported_names = Vec::new();
    let mut skipped_rows = 0usize;
    for name in &names {
        let entry = &entries[name];
        let inserted = insert_legacy_entry(&mut transaction, name, entry).await?;
        if inserted {
            imported_names.push(name.clone());
        } else {
            skipped_rows += 1;
        }
    }
    transaction.commit().await?;
    let backup_path = rename_imported_legacy_file(source_path).await?;
    let report = ImportReport {
        source_path: source_path.to_path_buf(),
        backup_path,
        imported_rows: imported_names.len(),
        skipped_rows,
        secret_names: imported_names,
    };
    info!(
        source_path = %report.source_path.display(),
        backup_path = %report.backup_path.display(),
        imported_rows = report.imported_rows,
        skipped_rows = report.skipped_rows,
        secret_names = ?report.secret_names,
        "Imported legacy secrets JSON into SQLite"
    );
    Ok(Some(report))
}

async fn insert_legacy_entry(
    transaction: &mut Transaction<'_, Sqlite>,
    name: &str,
    entry: &SecretEntry,
) -> Result<bool, SecretStoreError> {
    let result = sqlx::query(
        r"
        INSERT INTO secrets (
            name, secret_type, value, description, revision, created_at, updated_at
        ) VALUES (?, ?, ?, ?, 1, ?, ?)
        ON CONFLICT(name) DO NOTHING
        ",
    )
    .bind(name)
    .bind(entry.secret_type.as_str())
    .bind(&entry.value)
    .bind(entry.description.as_deref())
    .bind(entry.created_at.to_rfc3339())
    .bind(entry.updated_at.to_rfc3339())
    .execute(&mut **transaction)
    .await?;
    Ok(result.rows_affected() == 1)
}

fn entry_from_row(row: &SqliteRow) -> Result<(String, SecretEntry), SecretStoreError> {
    let metadata = metadata_from_row(row)?;
    let name = metadata.name;
    let value = row.try_get::<String, _>("value")?;
    if metadata.secret_type == SecretType::Oauth {
        validate_oauth_json(&value).map_err(|source| SecretStoreError::StoredOauth {
            name: name.clone(),
            source,
        })?;
    }
    let revision = row.try_get::<i64, _>("revision")?;
    if revision <= 0 {
        return Err(SecretStoreError::StoredRevision { name, revision });
    }
    let entry = SecretEntry {
        value,
        secret_type: metadata.secret_type,
        description: metadata.description,
        created_at: metadata.created_at,
        updated_at: metadata.updated_at,
        revision,
    };
    Ok((name, entry))
}

fn metadata_from_row(row: &SqliteRow) -> Result<SecretMetadata, SecretStoreError> {
    let name = row.try_get::<String, _>("name")?;
    let type_value = row.try_get::<String, _>("secret_type")?;
    let secret_type =
        SecretType::from_str(&type_value).map_err(|_| SecretStoreError::StoredType {
            name:  name.clone(),
            value: type_value,
        })?;
    validate_stored_name(&name, secret_type)?;
    Ok(SecretMetadata {
        name: name.clone(),
        secret_type,
        description: row.try_get("description")?,
        created_at: parse_timestamp(
            &name,
            "created_at",
            &row.try_get::<String, _>("created_at")?,
        )?,
        updated_at: parse_timestamp(
            &name,
            "updated_at",
            &row.try_get::<String, _>("updated_at")?,
        )?,
    })
}

fn parse_timestamp(
    name: &str,
    column: &'static str,
    value: &str,
) -> Result<DateTime<Utc>, SecretStoreError> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|source| SecretStoreError::Timestamp {
            name: name.to_string(),
            column,
            source,
        })
}

fn validate_oauth_json(value: &str) -> Result<(), serde_json::Error> {
    serde_json::from_str::<OAuthCredential>(value).map(|_| ())
}

fn validate_write(
    name: &str,
    value: &str,
    secret_type: SecretType,
) -> Result<(), SecretStoreError> {
    Vault::validate_name(name, secret_type)
        .map_err(|_| SecretStoreError::InvalidName(name.to_string()))?;
    if secret_type == SecretType::Oauth {
        validate_oauth_json(value).map_err(|source| SecretStoreError::InvalidOauth {
            name: name.to_string(),
            source,
        })?;
    }
    Ok(())
}

fn validate_stored_name(name: &str, secret_type: SecretType) -> Result<(), SecretStoreError> {
    Vault::validate_name(name, secret_type).map_err(|_| SecretStoreError::StoredName {
        name: name.to_string(),
    })
}

async fn rename_imported_legacy_file(source_path: &Path) -> Result<PathBuf, SecretStoreError> {
    let backup_path = legacy_backup_path(source_path, Utc::now());
    fs::rename(source_path, &backup_path)
        .await
        .map_err(|source| SecretStoreError::LegacyBackup {
            source_path: source_path.to_path_buf(),
            backup_path: backup_path.clone(),
            source,
        })?;
    Ok(backup_path)
}

fn legacy_backup_path(source_path: &Path, imported_at: DateTime<Utc>) -> PathBuf {
    let timestamp = imported_at.format("%Y%m%dT%H%M%S%fZ");
    let mut file_name = source_path
        .file_name()
        .map_or_else(|| OsString::from("secrets.json"), OsString::from);
    file_name.push(format!(".imported-{timestamp}.bak"));
    source_path.with_file_name(file_name)
}
