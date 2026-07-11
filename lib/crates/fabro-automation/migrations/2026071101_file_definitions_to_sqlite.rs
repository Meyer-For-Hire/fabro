//! Imports the pre-SQLite `automations/*.toml` directory into the relational
//! automation store. Remove this compatibility migration after 2026-10-11,
//! once supported upgrades no longer span a file-backed Fabro release.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use fabro_db::DbPool;
use tokio::fs;
use tracing::info;

use crate::{Automation, AutomationId, AutomationStoreError, store};

pub(crate) const REMOVAL_DEADLINE: &str = "2026-10-11";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub source_path:    PathBuf,
    pub backup_path:    PathBuf,
    pub imported_rows:  usize,
    pub skipped_rows:   usize,
    pub automation_ids: Vec<String>,
}

pub async fn import_legacy_directory_once(
    pool: &DbPool,
    source_dir: impl AsRef<Path>,
) -> Result<Option<ImportReport>, AutomationStoreError> {
    let source_dir = source_dir.as_ref();
    let Some(paths) = legacy_automation_paths(source_dir).await? else {
        return Ok(None);
    };
    let mut automations = Vec::with_capacity(paths.len());
    for (id, path) in paths {
        let bytes = fs::read(&path)
            .await
            .map_err(|source| AutomationStoreError::io(&path, source))?;
        automations.push(Automation::from_persisted_path(id, &bytes, path)?);
    }

    let mut transaction = pool.begin().await?;
    let mut imported_ids = Vec::new();
    let mut skipped_rows = 0;
    for automation in &automations {
        if store::insert_automation_ignoring_conflict(&mut transaction, automation).await? {
            imported_ids.push(automation.id.to_string());
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
        automation_ids: imported_ids,
    };
    info!(
        source_path = %report.source_path.display(),
        backup_path = %report.backup_path.display(),
        imported_rows = report.imported_rows,
        skipped_rows = report.skipped_rows,
        automation_ids = ?report.automation_ids,
        removal_deadline = REMOVAL_DEADLINE,
        "Imported legacy automations directory into SQLite"
    );
    Ok(Some(report))
}

async fn legacy_automation_paths(
    source_dir: &Path,
) -> Result<Option<Vec<(AutomationId, PathBuf)>>, AutomationStoreError> {
    let mut entries = match fs::read_dir(source_dir).await {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(AutomationStoreError::io(source_dir, source)),
    };
    let mut paths = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|source| AutomationStoreError::io(source_dir, source))?
    {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .await
            .map_err(|source| AutomationStoreError::io(&path, source))?;
        if file_type.is_file() && is_toml_file(&path) {
            paths.push((id_from_path(&path)?, path));
        }
    }
    paths.sort_by(|left, right| left.1.cmp(&right.1));
    Ok(Some(paths))
}

fn id_from_path(path: &Path) -> Result<AutomationId, AutomationStoreError> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| AutomationStoreError::InvalidFilename {
            path:   path.to_path_buf(),
            reason: "filename is not valid UTF-8".to_string(),
        })?;
    AutomationId::new(stem).map_err(|source| AutomationStoreError::InvalidFilename {
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
) -> Result<PathBuf, AutomationStoreError> {
    let backup_path = legacy_backup_path(source_dir, Utc::now());
    fs::rename(source_dir, &backup_path)
        .await
        .map_err(|source| AutomationStoreError::LegacyBackup {
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
        .map_or_else(|| OsString::from("automations"), OsString::from);
    file_name.push(format!(".imported-{timestamp}.bak"));
    source_dir.with_file_name(file_name)
}
