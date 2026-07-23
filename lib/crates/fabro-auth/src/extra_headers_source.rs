use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use fabro_model::{Catalog, ProviderId};

use crate::credential_source::{CredentialSource, ResolvedCredentials};

/// Decorates another [`CredentialSource`] by appending fixed extra headers to
/// every credential it resolves.
///
/// Headers already present on a credential (for example from explicit
/// provider configuration) are left untouched.
pub struct ExtraHeadersCredentialSource {
    inner:   Arc<dyn CredentialSource>,
    headers: HashMap<String, String>,
}

impl ExtraHeadersCredentialSource {
    #[must_use]
    pub fn new(inner: Arc<dyn CredentialSource>, headers: HashMap<String, String>) -> Self {
        Self { inner, headers }
    }
}

#[async_trait]
impl CredentialSource for ExtraHeadersCredentialSource {
    async fn resolve(&self, catalog: &Catalog) -> anyhow::Result<ResolvedCredentials> {
        let mut resolved = self.inner.resolve(catalog).await?;
        for credential in &mut resolved.credentials {
            for (name, value) in &self.headers {
                credential
                    .extra_headers
                    .entry(name.clone())
                    .or_insert_with(|| value.clone());
            }
        }
        Ok(resolved)
    }

    async fn configured_providers(&self, catalog: &Catalog) -> Vec<ProviderId> {
        self.inner.configured_providers(catalog).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ApiCredential, ResolveError};

    struct StubSource {
        credentials: Vec<ApiCredential>,
        auth_issues: Vec<(ProviderId, ResolveError)>,
    }

    #[async_trait]
    impl CredentialSource for StubSource {
        async fn resolve(&self, _catalog: &Catalog) -> anyhow::Result<ResolvedCredentials> {
            Ok(ResolvedCredentials {
                credentials: self.credentials.clone(),
                auth_issues: self
                    .auth_issues
                    .iter()
                    .map(|(provider, _)| {
                        (
                            provider.clone(),
                            ResolveError::NotConfigured(provider.clone()),
                        )
                    })
                    .collect(),
            })
        }

        async fn configured_providers(&self, _catalog: &Catalog) -> Vec<ProviderId> {
            self.credentials
                .iter()
                .map(|c| c.provider.clone())
                .collect()
        }
    }

    fn credential(provider: &str, extra_headers: HashMap<String, String>) -> ApiCredential {
        ApiCredential {
            provider: ProviderId::new(provider),
            auth_header: None,
            extra_headers,
            base_url: None,
            codex_mode: false,
            org_id: None,
            project_id: None,
        }
    }

    fn catalog() -> Catalog {
        Catalog::from_builtin().unwrap()
    }

    #[tokio::test]
    async fn appends_headers_to_every_resolved_credential() {
        let source = ExtraHeadersCredentialSource::new(
            Arc::new(StubSource {
                credentials: vec![
                    credential("anthropic", HashMap::new()),
                    credential("openai", HashMap::new()),
                ],
                auth_issues: Vec::new(),
            }),
            HashMap::from([("x-session-id".to_string(), "run-123".to_string())]),
        );

        let resolved = source.resolve(&catalog()).await.unwrap();

        assert_eq!(resolved.credentials.len(), 2);
        for credential in &resolved.credentials {
            assert_eq!(
                credential.extra_headers.get("x-session-id"),
                Some(&"run-123".to_string())
            );
        }
    }

    #[tokio::test]
    async fn preserves_headers_already_set_on_a_credential() {
        let source = ExtraHeadersCredentialSource::new(
            Arc::new(StubSource {
                credentials: vec![credential(
                    "openrouter",
                    HashMap::from([("x-session-id".to_string(), "configured".to_string())]),
                )],
                auth_issues: Vec::new(),
            }),
            HashMap::from([("x-session-id".to_string(), "run-123".to_string())]),
        );

        let resolved = source.resolve(&catalog()).await.unwrap();

        assert_eq!(
            resolved.credentials[0].extra_headers.get("x-session-id"),
            Some(&"configured".to_string())
        );
    }

    #[tokio::test]
    async fn passes_through_auth_issues_and_configured_providers() {
        let provider = ProviderId::new("anthropic");
        let source = ExtraHeadersCredentialSource::new(
            Arc::new(StubSource {
                credentials: vec![credential("openai", HashMap::new())],
                auth_issues: vec![(
                    provider.clone(),
                    ResolveError::NotConfigured(provider.clone()),
                )],
            }),
            HashMap::from([("x-session-id".to_string(), "run-123".to_string())]),
        );

        let resolved = source.resolve(&catalog()).await.unwrap();
        assert_eq!(resolved.auth_issues.len(), 1);
        assert_eq!(resolved.auth_issues[0].0, provider);

        let providers = source.configured_providers(&catalog()).await;
        assert_eq!(providers, vec![ProviderId::new("openai")]);
    }
}
