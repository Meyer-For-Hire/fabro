//! Helpers shared by the one-time imports that seed SQLite tables from
//! pre-SQLite on-disk stores.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use tokio::fs;

/// Failure to move an imported legacy source aside, carrying the backup path
/// the caller needs for its own error variant.
#[derive(Debug)]
pub struct LegacyBackupError {
    pub backup_path: PathBuf,
    pub source:      std::io::Error,
}

/// Move an imported legacy file or directory aside to
/// `<name>.imported-<timestamp>.bak` next to the original. `fallback_name` is
/// used when the source path has no final component.
pub async fn rename_to_legacy_backup(
    source: &Path,
    fallback_name: &str,
) -> Result<PathBuf, LegacyBackupError> {
    let backup_path = legacy_backup_path(source, fallback_name, Utc::now());
    fs::rename(source, &backup_path)
        .await
        .map_err(|source| LegacyBackupError {
            backup_path: backup_path.clone(),
            source,
        })?;
    Ok(backup_path)
}

fn legacy_backup_path(source: &Path, fallback_name: &str, imported_at: DateTime<Utc>) -> PathBuf {
    let timestamp = imported_at.format("%Y%m%dT%H%M%S%fZ");
    let mut file_name = source
        .file_name()
        .map_or_else(|| OsString::from(fallback_name), OsString::from);
    file_name.push(format!(".imported-{timestamp}.bak"));
    source.with_file_name(file_name)
}

/// True when `path` has a `.toml` extension (legacy per-item store files).
pub fn is_toml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension == "toml")
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    #[test]
    fn backup_path_appends_timestamped_suffix() {
        let imported_at = Utc.with_ymd_and_hms(2026, 7, 11, 1, 2, 3).unwrap();
        let backup =
            legacy_backup_path(Path::new("/data/secrets.json"), "secrets.json", imported_at);
        assert_eq!(
            backup,
            Path::new("/data/secrets.json.imported-20260711T010203000000000Z.bak")
        );
    }

    #[test]
    fn backup_path_uses_fallback_when_source_has_no_file_name() {
        let imported_at = Utc.with_ymd_and_hms(2026, 7, 11, 1, 2, 3).unwrap();
        let backup = legacy_backup_path(Path::new("/"), "mcps", imported_at);
        assert!(
            backup
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("mcps.imported-"))
        );
    }
}
