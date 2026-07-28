use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use fabro_model::{Catalog, ProviderId};
use fabro_static::EnvVars;
use fabro_vault::Vault;
use tokio::sync::RwLock as AsyncRwLock;

use crate::resolve::apply_openai_codex_api_context;
use crate::{CredentialSource, EnvLookup, ResolvedCredentials, VaultCredentialSource};

/// A credential source for provider credentials declared as `env:<NAME>`.
///
/// This public SDK facade does not resolve `{{ env.NAME }}` settings
/// interpolation. Provider extra headers can use literals, but secret
/// interpolation requires a vault-backed source.
#[derive(Clone)]
pub struct EnvCredentialSource {
    inner:      VaultCredentialSource,
    env_lookup: EnvLookup,
}

impl EnvCredentialSource {
    #[must_use]
    #[expect(
        clippy::disallowed_methods,
        reason = "EnvCredentialSource is the provider credential process-env facade."
    )]
    pub fn new() -> Self {
        Self::with_env_lookup(Arc::new(|name| std::env::var(name).ok()))
    }

    #[must_use]
    pub fn with_env_lookup(env_lookup: EnvLookup) -> Self {
        let vault = Arc::new(AsyncRwLock::new(Vault::from_entries(HashMap::new())));
        let inner_lookup = Arc::clone(&env_lookup);
        let inner = VaultCredentialSource::with_env_lookup(vault, move |name| inner_lookup(name));
        Self { inner, env_lookup }
    }

    fn lookup(&self, name: &str) -> Option<String> {
        (self.env_lookup)(name)
    }
}

impl std::fmt::Debug for EnvCredentialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnvCredentialSource")
            .finish_non_exhaustive()
    }
}

impl Default for EnvCredentialSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CredentialSource for EnvCredentialSource {
    async fn resolve(&self, catalog: &Catalog) -> anyhow::Result<ResolvedCredentials> {
        let mut resolved = self.inner.resolve(catalog).await?;
        if let (Some(account_id), Some(credential)) = (
            self.lookup(EnvVars::CHATGPT_ACCOUNT_ID),
            resolved
                .credentials
                .iter_mut()
                .find(|credential| credential.provider == ProviderId::openai()),
        ) {
            apply_openai_codex_api_context(credential, Some(&account_id), self.env_lookup.as_ref());
        }
        Ok(resolved)
    }

    async fn configured_providers(&self, catalog: &Catalog) -> Vec<ProviderId> {
        self.inner.configured_providers(catalog).await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use fabro_model::catalog::LlmCatalogSettings;
    use fabro_model::{Catalog, ProviderId};
    use fabro_types::settings::interp::Namespace;

    use super::EnvCredentialSource;
    use crate::CredentialSource;

    fn test_source(entries: &[(&str, &str)]) -> EnvCredentialSource {
        let entries: HashMap<String, String> = entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect();
        EnvCredentialSource::with_env_lookup(Arc::new(move |name| entries.get(name).cloned()))
    }

    #[tokio::test]
    async fn configured_providers_reads_injected_provider_env() {
        let source = test_source(&[("ANTHROPIC_API_KEY", "anthropic-key")]);
        let catalog = Catalog::from_builtin().unwrap();

        assert_eq!(source.configured_providers(&catalog).await, vec![
            ProviderId::anthropic()
        ]);
    }

    #[tokio::test]
    async fn resolve_builds_openai_codex_env_credential() {
        let source = test_source(&[
            ("OPENAI_API_KEY", "openai-key"),
            ("CHATGPT_ACCOUNT_ID", "acct_123"),
            ("OPENAI_PROJECT_ID", "project_123"),
        ]);
        let catalog = Catalog::from_builtin().unwrap();

        let resolved = source.resolve(&catalog).await.unwrap();
        let credential = resolved.credentials.first().unwrap();

        assert_eq!(credential.provider, ProviderId::openai());
        assert!(credential.codex_mode);
        assert_eq!(
            credential.base_url.as_deref(),
            Some("https://chatgpt.com/backend-api/codex")
        );
        assert_eq!(
            credential.extra_headers.get("ChatGPT-Account-Id"),
            Some(&"acct_123".to_string())
        );
        assert_eq!(credential.project_id.as_deref(), Some("project_123"));
    }

    #[tokio::test]
    async fn env_settings_interpolation_remains_unsupported() {
        let settings: LlmCatalogSettings = toml::from_str(
            r#"
[providers.acme]
display_name = "Acme"
adapter = "openai_compatible"
agent_profile = "openai"
base_url = "https://api.acme.test/v1"

[providers.acme.auth]
credentials = ["env:ACME_API_KEY"]

[providers.acme.extra_headers]
x-account = "{{ env.ACME_ACCOUNT }}"
"#,
        )
        .unwrap();
        let catalog = Catalog::from_builtin_with_overrides(&settings).unwrap();
        let source = test_source(&[("ACME_API_KEY", "acme-key"), ("ACME_ACCOUNT", "account-id")]);

        let resolved = source.resolve(&catalog).await.unwrap();

        assert!(
            resolved
                .credentials
                .iter()
                .all(|credential| credential.provider != ProviderId::new("acme"))
        );
        assert!(resolved.auth_issues.iter().any(|(provider, issue)| {
            provider == &ProviderId::new("acme")
                && matches!(
                    issue,
                    crate::ResolveError::Interpolation { source, .. }
                        if source.namespace == Namespace::Env
                )
        }));
    }

    #[tokio::test]
    async fn modal_env_vars_do_not_replace_vault_secrets() {
        let settings: LlmCatalogSettings = toml::from_str(
            r#"
[providers.modal]
enabled = true
base_url = "https://example--kimi-k3.modal.run/v1"
"#,
        )
        .unwrap();
        let catalog = Catalog::from_builtin_with_overrides(&settings).unwrap();
        let source = test_source(&[
            ("MODAL_TOKEN_ID", "wk-test"),
            ("MODAL_TOKEN_SECRET", "ws-test"),
        ]);
        let modal = ProviderId::new("modal");

        assert!(!source.configured_providers(&catalog).await.contains(&modal));

        let resolved = source.resolve(&catalog).await.unwrap();

        assert!(
            resolved
                .credentials
                .iter()
                .all(|credential| credential.provider != modal)
        );
        assert!(resolved.auth_issues.iter().any(|(provider, issue)| {
            provider == &modal
                && matches!(
                    issue,
                    crate::ResolveError::Interpolation { source, .. }
                        if source.namespace == Namespace::Secrets
                )
        }));
    }
}
