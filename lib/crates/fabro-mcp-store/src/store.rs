use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::str::FromStr as _;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use fabro_db::DbPool;
use fabro_types::settings::run::{McpHttpProtocol, McpServerSettings, McpTransport};
use fabro_types::{
    McpServerDefinition, McpServerDraft, McpServerId, McpServerReplace, McpServerRevision,
    McpServerValidationError, McpServerView,
};
use serde::de::DeserializeOwned;
use sqlx::query::Query;
use sqlx::sqlite::{SqliteArguments, SqliteRow};
use sqlx::{Row as _, Sqlite, Transaction};
use strum::{Display, EnumString, IntoStaticStr};
use tokio::fs;
use tokio::sync::Mutex;
use tracing::info;

use crate::error::McpServerStoreError;
use crate::model;

/// SQLite-backed durable store for server-managed MCP server definitions.
///
/// Reads use a synchronous in-memory catalog because manifest resolution is
/// synchronous. Mutations are serialized within this process and use
/// revision-guarded SQL so SQLite remains authoritative for concurrency.
pub struct McpServerStore {
    pool:      DbPool,
    mutations: Mutex<()>,
    defs:      RwLock<HashMap<McpServerId, McpServerDefinition>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub source_path:    PathBuf,
    pub backup_path:    PathBuf,
    pub imported_rows:  usize,
    pub skipped_rows:   usize,
    pub mcp_server_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, Display, EnumString, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum TransportType {
    Stdio,
    Http,
    Sandbox,
}

impl TransportType {
    fn as_str(self) -> &'static str {
        self.into()
    }
}

impl std::fmt::Debug for McpServerStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServerStore").finish_non_exhaustive()
    }
}

impl McpServerStore {
    /// Load every persisted definition and build the synchronous catalog.
    pub async fn load(pool: DbPool) -> Result<Self, McpServerStoreError> {
        let defs = load_definitions(&pool).await?;
        Ok(Self {
            pool,
            mutations: Mutex::new(()),
            defs: RwLock::new(defs),
        })
    }

    /// Import a legacy `mcps/*.toml` directory when present, then load the
    /// SQLite-backed catalog.
    pub async fn open(
        pool: DbPool,
        legacy_dir: impl AsRef<Path>,
    ) -> Result<Self, McpServerStoreError> {
        import_legacy_directory_once(&pool, legacy_dir).await?;
        Self::load(pool).await
    }

    fn read_defs(
        &self,
    ) -> std::sync::RwLockReadGuard<'_, HashMap<McpServerId, McpServerDefinition>> {
        self.defs.read().expect("mcp server store lock poisoned")
    }

    fn write_defs(
        &self,
    ) -> std::sync::RwLockWriteGuard<'_, HashMap<McpServerId, McpServerDefinition>> {
        self.defs.write().expect("mcp server store lock poisoned")
    }

    pub fn list(&self) -> Vec<McpServerDefinition> {
        let defs = self.read_defs();
        let mut values = defs.values().cloned().collect::<Vec<_>>();
        values.sort_by(|left, right| left.id.cmp(&right.id));
        values
    }

    pub fn list_views(&self) -> Vec<McpServerView> {
        let defs = self.read_defs();
        let mut values = defs.values().map(McpServerView::from).collect::<Vec<_>>();
        values.sort_by(|left, right| left.id.cmp(&right.id));
        values
    }

    pub fn catalog_settings(&self) -> HashMap<String, McpServerSettings> {
        let defs = self.read_defs();
        defs.iter()
            .map(|(id, definition)| (id.to_string(), server_settings_from_definition(definition)))
            .collect()
    }

    /// Sorted ids only, without cloning the potentially sensitive env/header
    /// maps carried by full definitions.
    pub fn ids(&self) -> Vec<McpServerId> {
        let defs = self.read_defs();
        let mut ids = defs.keys().cloned().collect::<Vec<_>>();
        ids.sort();
        ids
    }

    pub fn get(&self, id: &McpServerId) -> Option<McpServerDefinition> {
        self.read_defs().get(id).cloned()
    }

    pub async fn create(
        &self,
        draft: McpServerDraft,
    ) -> Result<McpServerDefinition, McpServerStoreError> {
        let (id, replace) = draft.into();
        let (definition, _) = model::definition_from_replace(id.clone(), replace)?;
        let _mutation = self.mutations.lock().await;
        let mut transaction = self.pool.begin().await?;
        if !insert_definition_ignoring_conflict(&mut transaction, &definition).await? {
            return Err(McpServerStoreError::AlreadyExists { id });
        }
        transaction.commit().await?;
        self.write_defs().insert(id, definition.clone());
        Ok(definition)
    }

    pub async fn replace(
        &self,
        id: &McpServerId,
        expected: &McpServerRevision,
        replace: McpServerReplace,
    ) -> Result<McpServerDefinition, McpServerStoreError> {
        let (definition, _) = model::definition_from_replace(id.clone(), replace)?;
        let _mutation = self.mutations.lock().await;
        let mut transaction = self.pool.begin().await?;
        update_definition(&mut transaction, &definition, expected).await?;
        transaction.commit().await?;
        self.write_defs().insert(id.clone(), definition.clone());
        Ok(definition)
    }

    pub async fn delete(
        &self,
        id: &McpServerId,
        expected: &McpServerRevision,
    ) -> Result<(), McpServerStoreError> {
        let _mutation = self.mutations.lock().await;
        let mut transaction = self.pool.begin().await?;
        let result = sqlx::query("DELETE FROM mcp_servers WHERE id = ? AND revision = ?")
            .bind(id.as_str())
            .bind(expected.as_str())
            .execute(&mut *transaction)
            .await?;
        if result.rows_affected() == 0 {
            return Err(revision_mismatch_error(&mut transaction, id, expected).await?);
        }
        transaction.commit().await?;
        self.write_defs().remove(id);
        Ok(())
    }
}

async fn load_definitions(
    pool: &DbPool,
) -> Result<HashMap<McpServerId, McpServerDefinition>, McpServerStoreError> {
    let rows = sqlx::query(
        r"
        SELECT
            id,
            revision,
            display_name,
            description,
            transport_type,
            protocol,
            command_json,
            url,
            port,
            env_json,
            headers_json,
            startup_timeout_secs,
            tool_timeout_secs
        FROM mcp_servers
        ORDER BY id
        ",
    )
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(definition_from_row)
        .map(|result| result.map(|definition| (definition.id.clone(), definition)))
        .collect()
}

fn definition_from_row(row: &SqliteRow) -> Result<McpServerDefinition, McpServerStoreError> {
    let id = McpServerId::new(row.try_get::<String, _>("id")?)?;
    let revision_text = row.try_get::<String, _>("revision")?;
    let revision = McpServerRevision::from_str(&revision_text).map_err(|source| {
        McpServerStoreError::StoredRevision {
            id: id.clone(),
            source,
        }
    })?;
    let transport_type_text = row.try_get::<String, _>("transport_type")?;
    let transport_type = TransportType::from_str(&transport_type_text).map_err(|_| {
        McpServerStoreError::StoredTransport {
            id:     id.clone(),
            reason: format!("unknown transport type {transport_type_text:?}"),
        }
    })?;
    let protocol = row.try_get::<Option<String>, _>("protocol")?;
    let command_json = row.try_get::<Option<String>, _>("command_json")?;
    let url = row.try_get::<Option<String>, _>("url")?;
    let port = row.try_get::<Option<i64>, _>("port")?;
    let env_json = row.try_get::<Option<String>, _>("env_json")?;
    let headers_json = row.try_get::<Option<String>, _>("headers_json")?;
    let transport = match transport_type {
        TransportType::Stdio => {
            require_absent(&id, "protocol", protocol.as_ref())?;
            require_absent(&id, "url", url.as_ref())?;
            require_absent(&id, "port", port.as_ref())?;
            require_absent(&id, "headers_json", headers_json.as_ref())?;
            McpTransport::Stdio {
                command: decode_json(
                    &id,
                    "command_json",
                    &require_field(&id, "command_json", command_json)?,
                )?,
                env:     decode_string_map(
                    &id,
                    "env_json",
                    &require_field(&id, "env_json", env_json)?,
                )?,
            }
        }
        TransportType::Http => {
            require_absent(&id, "command_json", command_json.as_ref())?;
            require_absent(&id, "port", port.as_ref())?;
            require_absent(&id, "env_json", env_json.as_ref())?;
            McpTransport::Http {
                protocol: parse_protocol(&id, require_field(&id, "protocol", protocol)?.as_str())?,
                url:      require_field(&id, "url", url)?,
                headers:  decode_string_map(
                    &id,
                    "headers_json",
                    &require_field(&id, "headers_json", headers_json)?,
                )?,
            }
        }
        TransportType::Sandbox => {
            require_absent(&id, "url", url.as_ref())?;
            require_absent(&id, "headers_json", headers_json.as_ref())?;
            McpTransport::Sandbox {
                protocol: parse_protocol(&id, require_field(&id, "protocol", protocol)?.as_str())?,
                command:  decode_json(
                    &id,
                    "command_json",
                    &require_field(&id, "command_json", command_json)?,
                )?,
                port:     decode_port(&id, require_field(&id, "port", port)?)?,
                env:      decode_string_map(
                    &id,
                    "env_json",
                    &require_field(&id, "env_json", env_json)?,
                )?,
            }
        }
    };
    let replace = McpServerReplace {
        display_name: row.try_get("display_name")?,
        description: row.try_get("description")?,
        transport,
        startup_timeout_secs: decode_timeout(
            &id,
            "startup_timeout_secs",
            row.try_get("startup_timeout_secs")?,
        )?,
        tool_timeout_secs: decode_timeout(
            &id,
            "tool_timeout_secs",
            row.try_get("tool_timeout_secs")?,
        )?,
    };
    model::definition_from_stored_parts(id, revision, replace)
}

fn require_field<T>(
    id: &McpServerId,
    field: &'static str,
    value: Option<T>,
) -> Result<T, McpServerStoreError> {
    value.ok_or_else(|| McpServerStoreError::StoredTransport {
        id:     id.clone(),
        reason: format!("{field} is missing"),
    })
}

fn require_absent<T>(
    id: &McpServerId,
    field: &'static str,
    value: Option<&T>,
) -> Result<(), McpServerStoreError> {
    if value.is_some() {
        return Err(McpServerStoreError::StoredTransport {
            id:     id.clone(),
            reason: format!("{field} must be absent"),
        });
    }
    Ok(())
}

fn parse_protocol(id: &McpServerId, value: &str) -> Result<McpHttpProtocol, McpServerStoreError> {
    McpHttpProtocol::from_str(value).map_err(|_| McpServerStoreError::StoredTransport {
        id:     id.clone(),
        reason: format!("unknown protocol {value:?}"),
    })
}

fn decode_port(id: &McpServerId, value: i64) -> Result<u16, McpServerStoreError> {
    u16::try_from(value).map_err(|_| McpServerStoreError::StoredInteger {
        id: id.clone(),
        column: "port",
        value,
    })
}

fn decode_timeout(
    id: &McpServerId,
    column: &'static str,
    value: i64,
) -> Result<u64, McpServerStoreError> {
    u64::try_from(value).map_err(|_| McpServerStoreError::StoredInteger {
        id: id.clone(),
        column,
        value,
    })
}

fn decode_json<T: DeserializeOwned>(
    id: &McpServerId,
    field: &'static str,
    value: &str,
) -> Result<T, McpServerStoreError> {
    serde_json::from_str(value).map_err(|source| McpServerStoreError::JsonDecode {
        id: id.clone(),
        field,
        source,
    })
}

fn decode_string_map(
    id: &McpServerId,
    field: &'static str,
    value: &str,
) -> Result<HashMap<String, String>, McpServerStoreError> {
    let ordered = decode_json::<BTreeMap<String, String>>(id, field, value)?;
    Ok(ordered.into_iter().collect())
}

fn server_settings_from_definition(definition: &McpServerDefinition) -> McpServerSettings {
    McpServerSettings {
        name:                 definition.id.to_string(),
        transport:            definition.transport.clone(),
        current_dir:          None,
        clear_env:            false,
        startup_timeout_secs: definition.startup_timeout_secs,
        tool_timeout_secs:    definition.tool_timeout_secs,
    }
}

async fn current_revision(
    transaction: &mut Transaction<'_, Sqlite>,
    id: &McpServerId,
) -> Result<Option<McpServerRevision>, McpServerStoreError> {
    let revision: Option<String> =
        sqlx::query_scalar("SELECT revision FROM mcp_servers WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(&mut **transaction)
            .await?;
    revision
        .map(|revision| {
            McpServerRevision::from_str(&revision).map_err(|source| {
                McpServerStoreError::StoredRevision {
                    id: id.clone(),
                    source,
                }
            })
        })
        .transpose()
}

async fn revision_mismatch_error(
    transaction: &mut Transaction<'_, Sqlite>,
    id: &McpServerId,
    expected: &McpServerRevision,
) -> Result<McpServerStoreError, McpServerStoreError> {
    let Some(actual) = current_revision(transaction, id).await? else {
        return Ok(McpServerStoreError::NotFound { id: id.clone() });
    };
    Ok(McpServerStoreError::StaleRevision {
        id: id.clone(),
        expected: expected.clone(),
        actual,
    })
}

async fn insert_definition_ignoring_conflict(
    transaction: &mut Transaction<'_, Sqlite>,
    definition: &McpServerDefinition,
) -> Result<bool, McpServerStoreError> {
    let row = McpServerSqlRow::from_definition(definition)?;
    let result = bind_definition(sqlx::query(INSERT_DEFINITION_SQL), row)
        .execute(&mut **transaction)
        .await?;
    Ok(result.rows_affected() == 1)
}

async fn update_definition(
    transaction: &mut Transaction<'_, Sqlite>,
    definition: &McpServerDefinition,
    expected: &McpServerRevision,
) -> Result<(), McpServerStoreError> {
    let row = McpServerSqlRow::from_definition(definition)?;
    let result = sqlx::query(UPDATE_DEFINITION_SQL)
        .bind(row.revision)
        .bind(row.display_name)
        .bind(row.description)
        .bind(row.transport_type)
        .bind(row.protocol)
        .bind(row.command_json)
        .bind(row.url)
        .bind(row.port)
        .bind(row.env_json)
        .bind(row.headers_json)
        .bind(row.startup_timeout_secs)
        .bind(row.tool_timeout_secs)
        .bind(row.id)
        .bind(expected.as_str())
        .execute(&mut **transaction)
        .await?;
    if result.rows_affected() == 0 {
        return Err(revision_mismatch_error(transaction, &definition.id, expected).await?);
    }
    Ok(())
}

fn bind_definition(
    query: Query<'_, Sqlite, SqliteArguments>,
    row: McpServerSqlRow,
) -> Query<'_, Sqlite, SqliteArguments> {
    query
        .bind(row.id)
        .bind(row.revision)
        .bind(row.display_name)
        .bind(row.description)
        .bind(row.transport_type)
        .bind(row.protocol)
        .bind(row.command_json)
        .bind(row.url)
        .bind(row.port)
        .bind(row.env_json)
        .bind(row.headers_json)
        .bind(row.startup_timeout_secs)
        .bind(row.tool_timeout_secs)
}

struct McpServerSqlRow {
    id:                   String,
    revision:             String,
    display_name:         String,
    description:          Option<String>,
    transport_type:       &'static str,
    protocol:             Option<&'static str>,
    command_json:         Option<String>,
    url:                  Option<String>,
    port:                 Option<i64>,
    env_json:             Option<String>,
    headers_json:         Option<String>,
    startup_timeout_secs: i64,
    tool_timeout_secs:    i64,
}

impl McpServerSqlRow {
    fn from_definition(definition: &McpServerDefinition) -> Result<Self, McpServerStoreError> {
        let (transport_type, protocol, command_json, url, port, env_json, headers_json) =
            match &definition.transport {
                McpTransport::Stdio { command, env } => (
                    TransportType::Stdio,
                    None,
                    Some(encode_json("command_json", command)?),
                    None,
                    None,
                    Some(encode_string_map("env_json", env)?),
                    None,
                ),
                McpTransport::Http {
                    protocol,
                    url,
                    headers,
                } => (
                    TransportType::Http,
                    Some(protocol.as_str()),
                    None,
                    Some(url.clone()),
                    None,
                    None,
                    Some(encode_string_map("headers_json", headers)?),
                ),
                McpTransport::Sandbox {
                    protocol,
                    command,
                    port,
                    env,
                } => (
                    TransportType::Sandbox,
                    Some(protocol.as_str()),
                    Some(encode_json("command_json", command)?),
                    None,
                    Some(i64::from(*port)),
                    Some(encode_string_map("env_json", env)?),
                    None,
                ),
            };
        Ok(Self {
            id: definition.id.to_string(),
            revision: definition.revision.to_string(),
            display_name: definition.display_name.clone(),
            description: definition.description.clone(),
            transport_type: transport_type.as_str(),
            protocol,
            command_json,
            url,
            port,
            env_json,
            headers_json,
            startup_timeout_secs: encode_timeout(
                "startup_timeout_secs",
                definition.startup_timeout_secs,
            )?,
            tool_timeout_secs: encode_timeout("tool_timeout_secs", definition.tool_timeout_secs)?,
        })
    }
}

fn encode_timeout(field: &'static str, value: u64) -> Result<i64, McpServerStoreError> {
    i64::try_from(value)
        .map_err(|_| McpServerValidationError::TimeoutOutOfRange { field, value }.into())
}

fn encode_json<T: serde::Serialize>(
    field: &'static str,
    value: &T,
) -> Result<String, McpServerStoreError> {
    serde_json::to_string(value).map_err(|source| McpServerStoreError::JsonEncode { field, source })
}

fn encode_string_map(
    field: &'static str,
    map: &HashMap<String, String>,
) -> Result<String, McpServerStoreError> {
    let ordered = map
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect::<BTreeMap<_, _>>();
    encode_json(field, &ordered)
}

const INSERT_DEFINITION_SQL: &str = r"
INSERT INTO mcp_servers (
    id,
    revision,
    display_name,
    description,
    transport_type,
    protocol,
    command_json,
    url,
    port,
    env_json,
    headers_json,
    startup_timeout_secs,
    tool_timeout_secs
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(id) DO NOTHING
";

const UPDATE_DEFINITION_SQL: &str = r"
UPDATE mcp_servers SET
    revision = ?,
    display_name = ?,
    description = ?,
    transport_type = ?,
    protocol = ?,
    command_json = ?,
    url = ?,
    port = ?,
    env_json = ?,
    headers_json = ?,
    startup_timeout_secs = ?,
    tool_timeout_secs = ?
WHERE id = ? AND revision = ?
";

pub async fn import_legacy_directory_once(
    pool: &DbPool,
    source_dir: impl AsRef<Path>,
) -> Result<Option<ImportReport>, McpServerStoreError> {
    let source_dir = source_dir.as_ref();
    let Some(paths) = legacy_definition_paths(source_dir).await? else {
        return Ok(None);
    };
    let definitions = read_legacy_definitions(paths).await?;

    let mut transaction = pool.begin().await?;
    let mut imported_ids = Vec::new();
    let mut skipped_rows = 0;
    for definition in &definitions {
        if insert_definition_ignoring_conflict(&mut transaction, definition).await? {
            imported_ids.push(definition.id.to_string());
        } else {
            skipped_rows += 1;
        }
    }
    transaction.commit().await?;

    let backup_path = rename_imported_legacy_directory(source_dir).await?;
    let report = ImportReport {
        source_path: source_dir.to_path_buf(),
        backup_path,
        imported_rows: imported_ids.len(),
        skipped_rows,
        mcp_server_ids: imported_ids,
    };
    info!(
        source_path = %report.source_path.display(),
        backup_path = %report.backup_path.display(),
        imported_rows = report.imported_rows,
        skipped_rows = report.skipped_rows,
        mcp_server_ids = ?report.mcp_server_ids,
        "Imported legacy MCP server directory into SQLite"
    );
    Ok(Some(report))
}

async fn legacy_definition_paths(
    source_dir: &Path,
) -> Result<Option<Vec<(McpServerId, PathBuf)>>, McpServerStoreError> {
    let mut entries = match fs::read_dir(source_dir).await {
        Ok(entries) => entries,
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(McpServerStoreError::io(source_dir, source)),
    };

    let mut paths = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|source| McpServerStoreError::io(source_dir, source))?
    {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .await
            .map_err(|source| McpServerStoreError::io(&path, source))?;
        if file_type.is_file() && is_toml_file(&path) {
            paths.push((id_from_path(&path)?, path));
        }
    }
    paths.sort_by(|left, right| left.1.cmp(&right.1));
    Ok(Some(paths))
}

async fn read_legacy_definitions(
    paths: Vec<(McpServerId, PathBuf)>,
) -> Result<Vec<McpServerDefinition>, McpServerStoreError> {
    let mut definitions = Vec::with_capacity(paths.len());
    for (id, path) in paths {
        let bytes = fs::read(&path)
            .await
            .map_err(|source| McpServerStoreError::io(&path, source))?;
        definitions.push(model::definition_from_persisted_path(id, &bytes, path)?);
    }
    definitions.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(definitions)
}

fn id_from_path(path: &Path) -> Result<McpServerId, McpServerStoreError> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| McpServerStoreError::InvalidFilename {
            path:   path.to_path_buf(),
            reason: "filename is not valid UTF-8".to_string(),
        })?;
    McpServerId::new(stem).map_err(|source| McpServerStoreError::InvalidFilename {
        path:   path.to_path_buf(),
        reason: source.to_string(),
    })
}

fn is_toml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension == "toml")
}

async fn rename_imported_legacy_directory(
    source_dir: &Path,
) -> Result<PathBuf, McpServerStoreError> {
    let backup_path = legacy_backup_path(source_dir, Utc::now());
    fs::rename(source_dir, &backup_path)
        .await
        .map_err(|source| McpServerStoreError::LegacyBackup {
            source_path: source_dir.to_path_buf(),
            backup_path: backup_path.clone(),
            source,
        })?;
    Ok(backup_path)
}

fn legacy_backup_path(source_dir: &Path, imported_at: DateTime<Utc>) -> PathBuf {
    let timestamp = imported_at.format("%Y%m%dT%H%M%S%fZ");
    let mut file_name = source_dir
        .file_name()
        .map_or_else(|| OsString::from("mcps"), OsString::from);
    file_name.push(format!(".imported-{timestamp}.bak"));
    source_dir.with_file_name(file_name)
}
