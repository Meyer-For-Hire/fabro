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
                if credential
                    .extra_headers
                    .keys()
                    .any(|existing| existing.eq_ignore_ascii_case(name))
                {
                    continue;
                }
                credential.extra_headers.insert(name.clone(), value.clone());
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
        credentials:          Vec<ApiCredential>,
        auth_issue_provider:  Option<ProviderId>,
        configured_providers: Vec<ProviderId>,
    }

    #[async_trait]
    impl CredentialSource for StubSource {
        async fn resolve(&self, _catalog: &Catalog) -> anyhow::Result<ResolvedCredentials> {
            Ok(ResolvedCredentials {
                credentials: self.credentials.clone(),
                auth_issues: self
                    .auth_issue_provider
                    .iter()
                    .map(|provider| {
                        (
                            provider.clone(),
                            ResolveError::RefreshTokenMissing(provider.clone()),
                        )
                    })
                    .collect(),
            })
        }

        async fn configured_providers(&self, _catalog: &Catalog) -> Vec<ProviderId> {
            self.configured_providers.clone()
        }
    }

    fn credential(provider: ProviderId, extra_headers: HashMap<String, String>) -> ApiCredential {
        ApiCredential {
            provider,
            auth_header: None,
            extra_headers,
            base_url: None,
            codex_mode: false,
            org_id: None,
            project_id: None,
        }
    }

    #[tokio::test]
    async fn appends_headers_to_every_resolved_credential() {
        let source = ExtraHeadersCredentialSource::new(
            Arc::new(StubSource {
                credentials:          vec![
                    credential(ProviderId::anthropic(), HashMap::new()),
                    credential(ProviderId::openai(), HashMap::new()),
                ],
                auth_issue_provider:  None,
                configured_providers: Vec::new(),
            }),
            HashMap::from([("x-session-id".to_string(), "run-123".to_string())]),
        );

        let resolved = source.resolve(Catalog::builtin()).await.unwrap();

        assert_eq!(resolved.credentials.len(), 2);
        for credential in &resolved.credentials {
            assert_eq!(
                credential
                    .extra_headers
                    .get("x-session-id")
                    .map(String::as_str),
                Some("run-123")
            );
        }
    }

    #[tokio::test]
    async fn preserves_case_insensitive_headers_already_set_on_a_credential() {
        let source = ExtraHeadersCredentialSource::new(
            Arc::new(StubSource {
                credentials:          vec![credential(
                    ProviderId::new("openrouter"),
                    HashMap::from([("X-Session-Id".to_string(), "configured".to_string())]),
                )],
                auth_issue_provider:  None,
                configured_providers: Vec::new(),
            }),
            HashMap::from([("x-session-id".to_string(), "run-123".to_string())]),
        );

        let resolved = source.resolve(Catalog::builtin()).await.unwrap();

        assert_eq!(
            resolved.credentials[0]
                .extra_headers
                .get("X-Session-Id")
                .map(String::as_str),
            Some("configured")
        );
        assert_eq!(resolved.credentials[0].extra_headers.len(), 1);
    }

    #[tokio::test]
    async fn passes_through_auth_issues_and_configured_providers() {
        let auth_issue_provider = ProviderId::anthropic();
        let configured_provider = ProviderId::gemini();
        let source = ExtraHeadersCredentialSource::new(
            Arc::new(StubSource {
                credentials:          vec![credential(ProviderId::openai(), HashMap::new())],
                auth_issue_provider:  Some(auth_issue_provider.clone()),
                configured_providers: vec![configured_provider.clone()],
            }),
            HashMap::from([("x-session-id".to_string(), "run-123".to_string())]),
        );

        let resolved = source.resolve(Catalog::builtin()).await.unwrap();
        let [(reported_provider, ResolveError::RefreshTokenMissing(error_provider))] =
            resolved.auth_issues.as_slice()
        else {
            panic!("expected the inner source's refresh-token issue");
        };
        assert_eq!(reported_provider, &auth_issue_provider);
        assert_eq!(error_provider, &auth_issue_provider);

        let providers = source.configured_providers(Catalog::builtin()).await;
        assert_eq!(providers, vec![configured_provider]);
    }
}
