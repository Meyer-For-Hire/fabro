//! Test-only credential sources.
//!
//! Feature-gated so they never link into production builds. Production code
//! resolves credentials through [`VaultCredentialSource`] over a real vault;
//! these helpers exist so tests can supply a source without one.

use std::collections::HashMap;
use std::sync::Arc;

use fabro_vault::Vault;
use tokio::sync::RwLock as AsyncRwLock;

use crate::credential_source::CredentialSource;
use crate::vault_source::VaultCredentialSource;

/// A detached in-memory vault holding no secrets.
#[must_use]
pub fn empty_vault() -> Arc<AsyncRwLock<Vault>> {
    Arc::new(AsyncRwLock::new(Vault::from_entries(HashMap::new())))
}

/// A vault-backed source whose credentials come only from `env_lookup`.
///
/// Tests that inject fake provider keys use this instead of reading the real
/// process environment, which would make them order-dependent.
#[must_use]
pub fn env_credential_source<F>(env_lookup: F) -> Arc<dyn CredentialSource>
where
    F: Fn(&str) -> Option<String> + Send + Sync + 'static,
{
    Arc::new(VaultCredentialSource::with_env_lookup(
        empty_vault(),
        env_lookup,
    ))
}

/// A vault-backed source over an empty vault with no process-env fallback.
#[must_use]
pub fn vault_only_credential_source() -> Arc<dyn CredentialSource> {
    Arc::new(VaultCredentialSource::vault_only(empty_vault()))
}
