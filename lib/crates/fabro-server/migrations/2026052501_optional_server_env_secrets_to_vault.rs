//! Temporary compatibility shim for optional secrets that used to live in
//! `server.env`.
//!
//! Delete this migration after 2026-08-18, once supported installs have had a
//! release window to move optional integration secrets into the vault.

#![expect(
    clippy::disallowed_methods,
    reason = "Temporary startup migration uses synchronous file I/O before serving requests."
)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use fabro_config::envfile::{self, EnvFileRemoval};
use fabro_static::{EnvVars, optional_vault_secrets};
use fabro_vault::{SecretStore, SecretStoreWrite, SecretType};

pub(crate) const REMOVAL_DEADLINE: &str = "2026-08-18";

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct OptionalServerEnvSecretsMigrationReport {
    pub(crate) migrated_secrets:      usize,
    pub(crate) removed_env_entries:   usize,
    pub(crate) preserved_env_entries: usize,
    pub(crate) backup_path:           Option<PathBuf>,
    pub(crate) warnings:              Vec<String>,
}

impl OptionalServerEnvSecretsMigrationReport {
    pub(crate) fn changed(&self) -> bool {
        self.migrated_secrets > 0 || self.removed_env_entries > 0
    }
}

pub(crate) async fn migrate_to_store(
    store: &SecretStore,
    server_env_path: &Path,
    env_entries: &HashMap<String, String>,
) -> anyhow::Result<OptionalServerEnvSecretsMigrationReport> {
    let server_env_entries = envfile::read_env_file(server_env_path)
        .with_context(|| format!("read server env file {}", server_env_path.display()))?;
    let stored = store.snapshot().await?;
    let mut writes = Vec::new();
    let mut env_removals = Vec::new();
    let mut warnings = Vec::new();
    let mut preserved_env_entries = 0;

    for &name in optional_vault_secrets() {
        let process_value = env_entries.get(name);
        let file_value = server_env_entries.get(name);
        if let Some(entry) = stored.get_entry(name) {
            if let Some(file_value) = file_value {
                if file_value == &entry.value {
                    env_removals.push(env_removal(name));
                } else {
                    preserved_env_entries += 1;
                    warnings.push(format!(
                        "Preserved {name} in server.env because the secret store already contains a different value"
                    ));
                }
            }
            continue;
        }

        let value = match (process_value, file_value) {
            (Some(value), Some(file_value)) => {
                if value == file_value {
                    env_removals.push(env_removal(name));
                } else {
                    preserved_env_entries += 1;
                    warnings.push(format!(
                        "Preserved {name} in server.env because process env takes precedence and the file value differs"
                    ));
                }
                Some(value)
            }
            (Some(value), None) => Some(value),
            (None, Some(value)) => {
                env_removals.push(env_removal(name));
                Some(value)
            }
            (None, None) => None,
        };
        if let Some(value) = value {
            writes.push(SecretStoreWrite {
                name:        name.to_string(),
                value:       value.clone(),
                secret_type: secret_type_for(name),
                description: None,
            });
        }
    }

    let mut report = OptionalServerEnvSecretsMigrationReport {
        migrated_secrets: writes.len(),
        removed_env_entries: 0,
        preserved_env_entries,
        backup_path: None,
        warnings,
    };
    if writes.is_empty() && env_removals.is_empty() {
        return Ok(report);
    }

    store.apply(&[], &writes).await?;
    if !env_removals.is_empty() {
        let backup_path = backup_server_env_file(server_env_path)?;
        let update_report =
            envfile::update_env_file_with_report(server_env_path, env_removals, Vec::new())
                .with_context(|| {
                    format!(
                        "remove migrated optional secrets from {}",
                        server_env_path.display()
                    )
                })?;
        report.removed_env_entries = update_report.removed_keys.len();
        report.backup_path = Some(backup_path);
    }
    Ok(report)
}

fn secret_type_for(name: &str) -> SecretType {
    if name == EnvVars::GITHUB_APP_PRIVATE_KEY {
        SecretType::File
    } else {
        SecretType::Token
    }
}

fn env_removal(name: &str) -> EnvFileRemoval {
    EnvFileRemoval {
        key:     name.to_string(),
        comment: None,
    }
}

fn backup_server_env_file(path: &Path) -> anyhow::Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("server.env");
    let backup_path = parent.join(format!(
        ".{file_name}.optional-server-env-secrets-to-vault-migration-{}.bak",
        ulid::Ulid::new()
    ));
    std::fs::copy(path, &backup_path).with_context(|| {
        format!(
            "copy server env {} to backup {}",
            path.display(),
            backup_path.display()
        )
    })?;
    set_private_permissions(&backup_path)?;
    Ok(backup_path)
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("set permissions on {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}
