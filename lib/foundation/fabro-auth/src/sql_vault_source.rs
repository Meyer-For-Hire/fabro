use std::sync::Arc;

use async_trait::async_trait;
use fabro_model::{Catalog, ProviderId};
use fabro_types::SecretType;
use fabro_vault::{SecretSnapshot, SecretStore, SecretStoreError, Vault};
use tokio::sync::RwLock;
use tracing::error;

use crate::credential_source::{CredentialSource, ResolvedCredentials};
use crate::{EnvLookup, VaultCredentialSource};

#[derive(Clone)]
pub struct SqlVaultCredentialSource {
    store:      Arc<SecretStore>,
    env_lookup: EnvLookup,
}

impl SqlVaultCredentialSource {
    #[must_use]
    #[expect(
        clippy::disallowed_methods,
        reason = "SqlVaultCredentialSource::new owns the process-env fallback used after vault \
                  lookup."
    )]
    pub fn new(store: Arc<SecretStore>) -> Self {
        Self::with_env_lookup(store, |name| std::env::var(name).ok())
    }

    #[must_use]
    pub fn vault_only(store: Arc<SecretStore>) -> Self {
        Self::with_env_lookup(store, |_| None)
    }

    #[must_use]
    pub fn with_env_lookup<F>(store: Arc<SecretStore>, env_lookup: F) -> Self
    where
        F: Fn(&str) -> Option<String> + Send + Sync + 'static,
    {
        Self {
            store,
            env_lookup: Arc::new(env_lookup),
        }
    }

    fn source_for_snapshot(&self, snapshot: SecretSnapshot) -> VaultCredentialSource {
        let env_lookup = Arc::clone(&self.env_lookup);
        VaultCredentialSource::with_env_lookup(
            Arc::new(RwLock::new(snapshot.into_vault())),
            move |name| env_lookup(name),
        )
    }

    async fn persist_oauth_refreshes(
        &self,
        before: &Vault,
        after: &Vault,
    ) -> Result<bool, SecretStoreError> {
        for (name, after_entry) in after.entries() {
            if after_entry.secret_type != SecretType::Oauth {
                continue;
            }
            let Some(before_entry) = before.get_entry(name) else {
                continue;
            };
            if before_entry.value == after_entry.value {
                continue;
            }
            match self
                .store
                .replace_if_revision(
                    name,
                    before_entry.revision,
                    &after_entry.value,
                    SecretType::Oauth,
                )
                .await
            {
                Ok(_) => {}
                Err(SecretStoreError::StaleRevision { .. }) => return Ok(false),
                Err(err) => return Err(err),
            }
        }
        Ok(true)
    }
}

impl std::fmt::Debug for SqlVaultCredentialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlVaultCredentialSource")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl CredentialSource for SqlVaultCredentialSource {
    async fn resolve(&self, catalog: &Catalog) -> anyhow::Result<ResolvedCredentials> {
        for _ in 0..2 {
            let before = self.store.snapshot().await?;
            let has_oauth = before
                .entries()
                .values()
                .any(|entry| entry.secret_type == SecretType::Oauth);
            if !has_oauth {
                // Only OAuth resolution can write back (token refresh); with no
                // OAuth secrets, skip the snapshot clones and CAS machinery.
                return self.source_for_snapshot(before).resolve(catalog).await;
            }
            let source = self.source_for_snapshot(before.clone());
            let resolved = source.resolve(catalog).await?;
            let after = source.snapshot().await;
            if self.persist_oauth_refreshes(&before, &after).await? {
                return Ok(resolved);
            }
        }
        anyhow::bail!("OAuth credential changed concurrently during refresh")
    }

    async fn configured_providers(&self, catalog: &Catalog) -> Vec<ProviderId> {
        let snapshot = match self.store.snapshot().await {
            Ok(snapshot) => snapshot,
            Err(err) => {
                error!(error = ?err, "Failed to load configured providers from secret store");
                return Vec::new();
            }
        };
        self.source_for_snapshot(snapshot)
            .configured_providers(catalog)
            .await
    }
}
