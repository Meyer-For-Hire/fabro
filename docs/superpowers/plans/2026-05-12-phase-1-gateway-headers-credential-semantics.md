# Phase 1 Gateway Headers And Credential Semantics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the Phase 1 settings surface for gateway providers that authenticate or route through custom headers, without switching production LLM routing yet.

**Architecture:** Extend `fabro-config`'s trusted `[llm.providers.<id>]` schema with typed `extra_headers` values that distinguish literal routing metadata from secret references. Keep runtime provider registration unchanged, but strengthen `fabro-llm` adapter-registry tests to prove resolved headers already flow through `AdapterConfig.extra_headers`. Attribute the PR motivation to `@haroldolivieri`'s Portkey/Bedrock field report from https://github.com/fabro-sh/fabro/pull/207#issuecomment-4377929769.

**Tech Stack:** Rust, serde/TOML settings layers, `fabro_macros::Combine`, `HashMap`, `cargo nextest`, pinned nightly rustfmt/clippy.

---

## PR Framing

**Suggested branch:** `fabro/run/phase-1-gateway-headers`

**Suggested PR title:** `Add gateway extra_headers settings for LLM providers`

**Suggested PR description attribution:**

```markdown
## Summary

Adds the Phase 1 settings surface for gateway-backed LLM providers. This was prompted by @haroldolivieri's Portkey/Bedrock field report on PR #207, which showed that gateway auth and routing often live in custom headers rather than the adapter's primary API-key header.

This PR is schema and seam work only. It does not make settings-defined providers runnable yet; later phases still own ProviderId migration, catalog construction, auth resolution, and production adapter registration.
```

## Scope

In scope:

- Add typed `extra_headers` to `[llm.providers.<id>]`.
- Allow literal non-secret header values and secret-bearing `env:` / `credential:` references.
- Preserve sparse provider field-merge behavior.
- Make `extra_headers` replace as a whole map when a higher layer sets it.
- Allow `credentials` to be absent at the schema layer when auth is supplied by headers.
- Add parse, merge, and redaction-oriented tests.
- Add adapter-registry tests proving `AdapterConfig.extra_headers` reaches constructed provider adapters.
- Update the local settings-driven LLM plan with the Phase 1 completion note.

Out of scope:

- Do not migrate `fabro_model::Provider` to `ProviderId`.
- Do not regenerate OpenAPI or TypeScript clients.
- Do not change `CredentialResolver`, `EnvCredentialSource`, or vault semantics.
- Do not make TOML-defined providers usable at runtime.
- Do not introduce provider/model capability overrides beyond documenting the handoff to future phases.
- Do not allow Codex OAuth credentials to route to arbitrary `base_url` values.

## File Structure

- Modify `lib/crates/fabro-config/src/layers/llm.rs`
  - Add `HeaderValueRef`.
  - Add `ProviderSettings.extra_headers`.
  - Add parsing/display tests and provider merge tests.
- Modify `lib/crates/fabro-config/src/layers/combine.rs`
  - Add `Combine` impl for `Option<HashMap<String, HeaderValueRef>>`.
- Modify `lib/crates/fabro-config/src/layers/mod.rs`
  - Re-export `HeaderValueRef`.
- Modify `lib/crates/fabro-config/src/lib.rs`
  - Publicly re-export `HeaderValueRef`.
- Modify `lib/crates/fabro-llm/src/adapter_registry.rs`
  - Add tests proving factories pass `extra_headers` through to adapter HTTP defaults.
- Modify `docs/superpowers/plans/2026-05-02-settings-driven-llm-providers-models.md`
  - Mark Phase 1 schema work as planned/completed by this PR, and record the attribution.

## Design Decision: Header Value Shape

Use an explicit tagged TOML table per header value:

```toml
[llm.providers.portkey]
display_name = "Portkey Bedrock"
adapter = "anthropic"
base_url = "https://api.portkey.ai/v1"

[llm.providers.portkey.extra_headers]
x-portkey-api-key = { env = "PORTKEY_API_KEY" }
x-portkey-provider = { literal = "@bedrock-prod" }
x-portkey-config = { credential = "portkey_config" }
```

Accepted forms:

- `{ literal = "..." }` for non-secret routing metadata.
- `{ env = "NAME" }` for secret-bearing or deployment-specific values read later from environment or vault.
- `{ credential = "id" }` for structured/raw vault references resolved later.

Rejected forms:

- Bare strings: `x-portkey-api-key = "sk-live"` must fail so secrets are not accidentally stored as successful settings values.
- Empty values: `{ env = "" }`, `{ credential = "" }`, and `{ literal = "" }` must fail.
- Ambiguous tables: `{ env = "A", literal = "B" }` must fail.
- Unknown keys: `{ secret = "A" }` must fail.

`HeaderValueRef::Display` must never resolve or expose secret material. It may print only typed reference forms such as `env:PORTKEY_API_KEY`, `credential:portkey_config`, or `literal:<redacted>`.

## Merge Semantics

`extra_headers` replaces as a whole map when present on a higher layer. If a higher layer omits it, lower-layer headers are inherited. If a higher layer sets an empty table, it explicitly clears lower-layer headers.

Rationale: gateway headers are usually a coherent auth/routing bundle. Per-header inheritance can accidentally keep a lower-layer auth or route header when a higher layer changes `base_url`.

---

### Task 1: Add `HeaderValueRef` Failing Tests

**Files:**

- Modify: `lib/crates/fabro-config/src/layers/llm.rs`

- [ ] **Step 1: Add failing `HeaderValueRef` unit tests**

Add these tests inside the existing `#[cfg(test)] mod tests` in `llm.rs`, after the `CredentialRef` tests and before `// ---- LlmLayer parsing`.

```rust
    // ---- HeaderValueRef --------------------------------------------------

    #[test]
    fn header_value_ref_parses_literal_form() {
        let parsed: HeaderValueRef = toml::from_str(r#"value = { literal = "@bedrock-prod" }"#)
            .map(|v: toml::Value| {
                v.as_table()
                    .unwrap()
                    .get("value")
                    .unwrap()
                    .clone()
                    .try_into()
                    .unwrap()
            })
            .unwrap();

        assert_eq!(parsed, HeaderValueRef::Literal("@bedrock-prod".to_string()));
        assert_eq!(parsed.to_string(), "literal:<redacted>");
    }

    #[test]
    fn header_value_ref_parses_env_form() {
        let parsed: HeaderValueRef = toml::from_str(r#"value = { env = "PORTKEY_API_KEY" }"#)
            .map(|v: toml::Value| {
                v.as_table()
                    .unwrap()
                    .get("value")
                    .unwrap()
                    .clone()
                    .try_into()
                    .unwrap()
            })
            .unwrap();

        assert_eq!(parsed, HeaderValueRef::Env("PORTKEY_API_KEY".to_string()));
        assert_eq!(parsed.to_string(), "env:PORTKEY_API_KEY");
    }

    #[test]
    fn header_value_ref_parses_credential_form() {
        let parsed: HeaderValueRef =
            toml::from_str(r#"value = { credential = "portkey_config" }"#)
                .map(|v: toml::Value| {
                    v.as_table()
                        .unwrap()
                        .get("value")
                        .unwrap()
                        .clone()
                        .try_into()
                        .unwrap()
                })
                .unwrap();

        assert_eq!(
            parsed,
            HeaderValueRef::Credential("portkey_config".to_string())
        );
        assert_eq!(parsed.to_string(), "credential:portkey_config");
    }

    #[test]
    fn header_value_ref_rejects_bare_string() {
        #[derive(Deserialize)]
        struct Wrap {
            value: HeaderValueRef,
        }

        let err = toml::from_str::<Wrap>(r#"value = "sk-portkey-literal""#).unwrap_err();

        assert!(err.to_string().contains("header value"));
        assert!(
            !err.to_string().contains("sk-portkey-literal"),
            "error must not echo a possible literal secret",
        );
    }

    #[test]
    fn header_value_ref_rejects_ambiguous_table() {
        #[derive(Deserialize)]
        struct Wrap {
            value: HeaderValueRef,
        }

        let err = toml::from_str::<Wrap>(
            r#"value = { env = "PORTKEY_API_KEY", literal = "@bedrock-prod" }"#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("exactly one"));
    }

    #[test]
    fn header_value_ref_rejects_empty_values() {
        #[derive(Deserialize)]
        struct Wrap {
            value: HeaderValueRef,
        }

        for source in [
            r#"value = { literal = "" }"#,
            r#"value = { env = "" }"#,
            r#"value = { credential = "" }"#,
        ] {
            let err = toml::from_str::<Wrap>(source).unwrap_err();
            assert!(err.to_string().contains("must not be empty"));
        }
    }
```

- [ ] **Step 2: Run the tests and verify they fail**

Run:

```bash
cargo nextest run -p fabro-config header_value_ref
```

Expected: compile failure because `HeaderValueRef` does not exist.

---

### Task 2: Implement `HeaderValueRef`

**Files:**

- Modify: `lib/crates/fabro-config/src/layers/llm.rs`
- Modify: `lib/crates/fabro-config/src/layers/mod.rs`
- Modify: `lib/crates/fabro-config/src/lib.rs`

- [ ] **Step 1: Add `HeaderValueRef` and parse error**

Add this code after the `CredentialRef` impl block and before `#[cfg(test)] mod tests`.

```rust
// ---------------------------------------------------------------------------
// HeaderValueRef — typed extra header value
// ---------------------------------------------------------------------------

/// A typed provider extra-header value.
///
/// Literal values are intended for non-secret routing metadata. Secret-bearing
/// values must use `env` or `credential` references so settings never need to
/// carry raw API keys as successful values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "HeaderValueRefSerde", try_from = "HeaderValueRefSerde")]
pub enum HeaderValueRef {
    Literal(String),
    Env(String),
    Credential(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct HeaderValueRefSerde {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    literal:   Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    env:       Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    credential: Option<String>,
}

impl From<HeaderValueRef> for HeaderValueRefSerde {
    fn from(value: HeaderValueRef) -> Self {
        match value {
            HeaderValueRef::Literal(literal) => Self {
                literal: Some(literal),
                env: None,
                credential: None,
            },
            HeaderValueRef::Env(env) => Self {
                literal: None,
                env: Some(env),
                credential: None,
            },
            HeaderValueRef::Credential(credential) => Self {
                literal: None,
                env: None,
                credential: Some(credential),
            },
        }
    }
}

impl TryFrom<HeaderValueRefSerde> for HeaderValueRef {
    type Error = HeaderValueRefParseError;

    fn try_from(value: HeaderValueRefSerde) -> Result<Self, Self::Error> {
        let populated = [
            value.literal.as_ref(),
            value.env.as_ref(),
            value.credential.as_ref(),
        ]
        .into_iter()
        .filter(|value| value.is_some())
        .count();

        if populated != 1 {
            return Err(HeaderValueRefParseError::WrongFieldCount);
        }

        if let Some(literal) = value.literal {
            if literal.is_empty() {
                return Err(HeaderValueRefParseError::EmptyValue);
            }
            return Ok(Self::Literal(literal));
        }
        if let Some(env) = value.env {
            if env.is_empty() {
                return Err(HeaderValueRefParseError::EmptyValue);
            }
            return Ok(Self::Env(env));
        }
        if let Some(credential) = value.credential {
            if credential.is_empty() {
                return Err(HeaderValueRefParseError::EmptyValue);
            }
            return Ok(Self::Credential(credential));
        }

        unreachable!("populated field count was already checked");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderValueRefParseError {
    WrongFieldCount,
    EmptyValue,
}

impl std::fmt::Display for HeaderValueRefParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongFieldCount => f.write_str(
                "header value must be a table with exactly one of `literal`, `env`, or `credential`; bare strings are rejected",
            ),
            Self::EmptyValue => {
                f.write_str("header value reference must not be empty")
            }
        }
    }
}

impl std::error::Error for HeaderValueRefParseError {}

impl std::fmt::Display for HeaderValueRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Literal(_) => f.write_str("literal:<redacted>"),
            Self::Env(name) => write!(f, "env:{name}"),
            Self::Credential(id) => write!(f, "credential:{id}"),
        }
    }
}
```

- [ ] **Step 2: Re-export from `layers/mod.rs`**

Find the `pub use llm::{ ... }` block in `lib/crates/fabro-config/src/layers/mod.rs` and include `HeaderValueRef`.

Expected shape:

```rust
pub(crate) use llm::{
    CostRates, CredentialRef, CredentialRefParseError, HeaderValueRef, LlmLayer, ModelControls,
    ModelCostTable, ModelFeatures as LlmModelFeatures, ModelLimits as LlmModelLimits,
    ModelSettings, ProviderSettings,
};
```

- [ ] **Step 3: Re-export from `lib.rs`**

Add `HeaderValueRef` to the public `pub use layers::{ ... }` list in `lib/crates/fabro-config/src/lib.rs`.

Expected nearby excerpt:

```rust
    DockerSandboxLayer, FeaturesLayer, GitAuthorLayer, GithubIntegrationLayer, HeaderValueRef,
    HookAgentMarker,
```

- [ ] **Step 4: Run the focused tests**

Run:

```bash
cargo nextest run -p fabro-config header_value_ref
```

Expected: all `header_value_ref_*` tests pass.

- [ ] **Step 5: Commit**

```bash
git add lib/crates/fabro-config/src/layers/llm.rs lib/crates/fabro-config/src/layers/mod.rs lib/crates/fabro-config/src/lib.rs
git commit -m "feat(config): add typed LLM provider header values"
```

---

### Task 3: Add `ProviderSettings.extra_headers`

**Files:**

- Modify: `lib/crates/fabro-config/src/layers/llm.rs`
- Modify: `lib/crates/fabro-config/src/layers/combine.rs`

- [ ] **Step 1: Add `extra_headers` parse and optional-credentials tests**

Add these tests inside `llm.rs` after `parses_minimal_provider_entry`.

```rust
    #[test]
    fn parses_provider_extra_headers() {
        let toml = r#"
[providers.portkey]
display_name = "Portkey Bedrock"
adapter = "anthropic"
base_url = "https://api.portkey.ai/v1"

[providers.portkey.extra_headers]
x-portkey-api-key = { env = "PORTKEY_API_KEY" }
x-portkey-provider = { literal = "@bedrock-prod" }
x-portkey-config = { credential = "portkey_config" }
"#;

        let layer: LlmLayer = toml::from_str(toml).unwrap();
        let portkey = layer.providers.get("portkey").unwrap();

        assert!(portkey.credentials.is_none());
        let headers = portkey.extra_headers.as_ref().unwrap();
        assert_eq!(
            headers.get("x-portkey-api-key"),
            Some(&HeaderValueRef::Env("PORTKEY_API_KEY".to_string())),
        );
        assert_eq!(
            headers.get("x-portkey-provider"),
            Some(&HeaderValueRef::Literal("@bedrock-prod".to_string())),
        );
        assert_eq!(
            headers.get("x-portkey-config"),
            Some(&HeaderValueRef::Credential("portkey_config".to_string())),
        );
    }

    #[test]
    fn provider_extra_headers_reject_bare_string_values() {
        let toml = r#"
[providers.portkey.extra_headers]
x-portkey-api-key = "sk-portkey-literal"
"#;

        let err = toml::from_str::<LlmLayer>(toml).unwrap_err();

        assert!(err.to_string().contains("header value"));
        assert!(
            !err.to_string().contains("sk-portkey-literal"),
            "error must not echo a possible literal secret",
        );
    }
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```bash
cargo nextest run -p fabro-config parses_provider_extra_headers provider_extra_headers_reject_bare_string_values
```

Expected: compile failure because `ProviderSettings.extra_headers` does not exist.

- [ ] **Step 3: Add `extra_headers` field**

In `ProviderSettings`, add the field after `credentials`:

```rust
    /// Extra HTTP headers attached to every outgoing provider request after
    /// credential resolution. Header values are typed so secret-bearing values
    /// stay as references until a later resolution phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_headers: Option<HashMap<String, HeaderValueRef>>,
```

Ensure `llm.rs` already imports `HashMap`:

```rust
use std::collections::{BTreeMap, HashMap};
```

- [ ] **Step 4: Add combine implementation**

In `lib/crates/fabro-config/src/layers/combine.rs`, change the `llm` import:

```rust
use super::llm::{CostRates, CredentialRef, HeaderValueRef};
```

Then add this impl near the existing `Option<HashMap<String, toml::Value>>` impl:

```rust
impl Combine for Option<HashMap<String, HeaderValueRef>> {
    fn combine(self, other: Self) -> Self {
        self.or(other)
    }
}
```

This gives whole-map replacement when the higher layer sets `extra_headers`, inheritance when it is unset, and explicit clearing when it is `Some(HashMap::new())`.

- [ ] **Step 5: Run focused tests**

Run:

```bash
cargo nextest run -p fabro-config parses_provider_extra_headers provider_extra_headers_reject_bare_string_values
```

Expected: both tests pass.

- [ ] **Step 6: Commit**

```bash
git add lib/crates/fabro-config/src/layers/llm.rs lib/crates/fabro-config/src/layers/combine.rs
git commit -m "feat(config): add LLM provider extra headers"
```

---

### Task 4: Add Merge Tests For `extra_headers`

**Files:**

- Modify: `lib/crates/fabro-config/src/layers/llm.rs`

- [ ] **Step 1: Add merge tests**

Add these tests near the existing provider credentials merge tests.

```rust
    #[test]
    fn provider_extra_headers_map_replaces_wholesale() {
        let high = ProviderSettings {
            extra_headers: Some(HashMap::from([(
                "x-portkey-provider".to_string(),
                HeaderValueRef::Literal("@bedrock-prod".to_string()),
            )])),
            ..ProviderSettings::default()
        };
        let low = ProviderSettings {
            extra_headers: Some(HashMap::from([
                (
                    "x-portkey-api-key".to_string(),
                    HeaderValueRef::Env("PORTKEY_API_KEY".to_string()),
                ),
                (
                    "x-portkey-provider".to_string(),
                    HeaderValueRef::Literal("@bedrock-default".to_string()),
                ),
            ])),
            ..ProviderSettings::default()
        };

        let merged = high.combine(low);

        let headers = merged.extra_headers.unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(
            headers.get("x-portkey-provider"),
            Some(&HeaderValueRef::Literal("@bedrock-prod".to_string())),
        );
        assert!(!headers.contains_key("x-portkey-api-key"));
    }

    #[test]
    fn provider_extra_headers_inherit_when_unset() {
        let high = ProviderSettings::default();
        let low = ProviderSettings {
            extra_headers: Some(HashMap::from([(
                "x-portkey-api-key".to_string(),
                HeaderValueRef::Env("PORTKEY_API_KEY".to_string()),
            )])),
            ..ProviderSettings::default()
        };

        let merged = high.combine(low);

        assert_eq!(
            merged.extra_headers.unwrap().get("x-portkey-api-key"),
            Some(&HeaderValueRef::Env("PORTKEY_API_KEY".to_string())),
        );
    }

    #[test]
    fn provider_extra_headers_empty_map_clears_lower_layer() {
        let high = ProviderSettings {
            extra_headers: Some(HashMap::new()),
            ..ProviderSettings::default()
        };
        let low = ProviderSettings {
            extra_headers: Some(HashMap::from([(
                "x-portkey-api-key".to_string(),
                HeaderValueRef::Env("PORTKEY_API_KEY".to_string()),
            )])),
            ..ProviderSettings::default()
        };

        let merged = high.combine(low);

        assert!(merged.extra_headers.unwrap().is_empty());
    }
```

- [ ] **Step 2: Run focused merge tests**

Run:

```bash
cargo nextest run -p fabro-config provider_extra_headers
```

Expected: all provider extra-header tests pass.

- [ ] **Step 3: Commit**

```bash
git add lib/crates/fabro-config/src/layers/llm.rs
git commit -m "test(config): cover LLM provider header merging"
```

---

### Task 5: Prove Adapter Factories Pass `extra_headers`

**Files:**

- Modify: `lib/crates/fabro-llm/src/adapter_registry.rs`

- [ ] **Step 1: Split concrete builder helpers out of factory functions**

Change the private factory functions so tests can inspect concrete adapters without adding new runtime trait API to `ProviderAdapter`.

```rust
fn build_anthropic_adapter(config: AdapterConfig) -> providers::AnthropicAdapter {
    let mut adapter = providers::AnthropicAdapter::new(auth_value(&config.auth_header));
    if let Some(base_url) = config.base_url {
        adapter = adapter.with_base_url(base_url);
    }
    if !config.extra_headers.is_empty() {
        adapter = adapter.with_default_headers(config.extra_headers);
    }
    adapter
}

fn build_anthropic(config: AdapterConfig) -> Arc<dyn ProviderAdapter> {
    Arc::new(build_anthropic_adapter(config))
}
```

Add the matching concrete helpers and wrappers for the other private factories:

```rust
fn build_openai_adapter(config: AdapterConfig) -> providers::OpenAiAdapter {
    let mut adapter = providers::OpenAiAdapter::new(
        auth_value(&config.auth_header),
        config.org_id,
        config.project_id,
    );
    if let Some(base_url) = config.base_url {
        adapter = adapter.with_base_url(base_url);
    }
    if !config.extra_headers.is_empty() {
        adapter = adapter.with_default_headers(config.extra_headers);
    }
    adapter
}

fn build_openai(config: AdapterConfig) -> Arc<dyn ProviderAdapter> {
    Arc::new(build_openai_adapter(config))
}

fn build_gemini_adapter(config: AdapterConfig) -> providers::GeminiAdapter {
    let mut adapter = providers::GeminiAdapter::new(auth_value(&config.auth_header));
    if let Some(base_url) = config.base_url {
        adapter = adapter.with_base_url(base_url);
    }
    if !config.extra_headers.is_empty() {
        adapter = adapter.with_default_headers(config.extra_headers);
    }
    adapter
}

fn build_gemini(config: AdapterConfig) -> Arc<dyn ProviderAdapter> {
    Arc::new(build_gemini_adapter(config))
}

fn build_openai_compatible_adapter(config: AdapterConfig) -> providers::OpenAiCompatibleAdapter {
    let mut adapter =
        providers::OpenAiCompatibleAdapter::new(config.provider_id, auth_value(&config.auth_header));
    if let Some(base_url) = config.base_url {
        adapter = adapter.with_base_url(base_url);
    }
    if !config.extra_headers.is_empty() {
        adapter = adapter.with_default_headers(config.extra_headers);
    }
    adapter
}

fn build_openai_compatible(config: AdapterConfig) -> Arc<dyn ProviderAdapter> {
    Arc::new(build_openai_compatible_adapter(config))
}
```

The public factory table must keep using `build_anthropic`, `build_openai`, `build_gemini`, and `build_openai_compatible`, so production behavior is unchanged.

- [ ] **Step 2: Add factory pass-through tests**

Add these tests near `openai_compatible_factory_uses_provider_id_for_name`.

```rust
    #[test]
    fn openai_compatible_factory_preserves_extra_headers() {
        let config = AdapterConfig {
            provider_id:   "portkey".to_string(),
            auth_header:   ApiKeyHeader::Bearer("unused-primary-key".to_string()),
            base_url:      Some("https://api.portkey.ai/v1".to_string()),
            extra_headers: HashMap::from([
                (
                    "x-portkey-api-key".to_string(),
                    "resolved-portkey-key".to_string(),
                ),
                (
                    "x-portkey-provider".to_string(),
                    "@bedrock-prod".to_string(),
                ),
            ]),
            codex_mode:    false,
            org_id:        None,
            project_id:    None,
        };

        let adapter = build_openai_compatible_adapter(config);

        assert_eq!(adapter.name(), "portkey");
        assert_eq!(
            adapter.http.default_headers.get("x-portkey-api-key"),
            Some(&"resolved-portkey-key".to_string()),
        );
        assert_eq!(
            adapter.http.default_headers.get("x-portkey-provider"),
            Some(&"@bedrock-prod".to_string()),
        );
    }

    #[test]
    fn anthropic_factory_preserves_extra_headers() {
        let config = AdapterConfig {
            provider_id:   "anthropic-through-portkey".to_string(),
            auth_header:   ApiKeyHeader::Custom {
                name:  "x-api-key".to_string(),
                value: "unused-primary-key".to_string(),
            },
            base_url:      Some("https://api.portkey.ai/v1".to_string()),
            extra_headers: HashMap::from([(
                "x-portkey-api-key".to_string(),
                "resolved-portkey-key".to_string(),
            )]),
            codex_mode:    false,
            org_id:        None,
            project_id:    None,
        };

        let adapter = build_anthropic_adapter(config);

        assert_eq!(adapter.name(), "anthropic");
        assert_eq!(
            adapter.http.default_headers.get("x-portkey-api-key"),
            Some(&"resolved-portkey-key".to_string()),
        );
    }
```

- [ ] **Step 3: Run the tests**

Run:

```bash
cargo nextest run -p fabro-llm extra_headers
```

Expected: tests pass.

- [ ] **Step 4: Commit**

```bash
git add lib/crates/fabro-llm/src/adapter_registry.rs
git commit -m "test(llm): prove adapter extra header passthrough"
```

---

### Task 6: Update Plan Documentation And Attribution

**Files:**

- Modify: `docs/superpowers/plans/2026-05-02-settings-driven-llm-providers-models.md`

- [ ] **Step 1: Add Phase 1 attribution note**

In the plan's summary/deferred-work section, add a short note:

```markdown
Phase 1 gateway header work is motivated by @haroldolivieri's Portkey/Bedrock report on PR #207:
https://github.com/fabro-sh/fabro/pull/207#issuecomment-4377929769
```

- [ ] **Step 2: Add Phase 1 checklist items**

Under the settings schema section, add:

```markdown
- [ ] Add provider-level `extra_headers` with typed literal/env/credential values.
- [ ] Make `extra_headers` replace as a whole map across settings layers.
- [ ] Keep this as schema/seam work only; runtime credential resolution and provider registration remain deferred to the resolved catalog/client phases.
```

If those items are added during implementation and completed in this same PR, check them off before merging.

- [ ] **Step 3: Run a documentation sanity check**

Run:

```bash
rg -n "haroldolivieri|extra_headers|Portkey" docs/superpowers/plans/2026-05-02-settings-driven-llm-providers-models.md
```

Expected: the attribution and Phase 1 scope are visible.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/plans/2026-05-02-settings-driven-llm-providers-models.md
git commit -m "docs: record gateway header phase attribution"
```

---

### Task 7: Final Verification

**Files:**

- No new files.
- Verifies all modified files.

- [ ] **Step 1: Run focused crate tests**

Run:

```bash
cargo nextest run -p fabro-config -p fabro-llm provider_extra_headers header_value_ref extra_headers
```

If that does not select all intended tests clearly, run the filters separately:

```bash
cargo nextest run -p fabro-config provider_extra_headers
cargo nextest run -p fabro-config header_value_ref
cargo nextest run -p fabro-llm extra_headers
```

Expected: all targeted tests pass.

- [ ] **Step 2: Run full affected crate tests**

Run:

```bash
cargo nextest run -p fabro-config -p fabro-llm
```

Expected: all tests pass.

- [ ] **Step 3: Run formatting**

Run:

```bash
cargo +nightly-2026-04-14 fmt --check --all
```

Expected: no formatting diffs. If it fails, run:

```bash
cargo +nightly-2026-04-14 fmt --all
```

Then re-run the check.

- [ ] **Step 4: Run clippy for affected crates**

Run:

```bash
cargo +nightly-2026-04-14 clippy -p fabro-config -p fabro-llm --all-targets -- -D warnings
```

Expected: no warnings.

- [ ] **Step 5: Inspect final diff**

Run:

```bash
git diff --stat origin/main...HEAD
git diff --check origin/main...HEAD
```

Expected: only the Phase 1 schema/test/docs files changed, and no whitespace errors.

- [ ] **Step 6: Prepare PR summary**

Use this PR summary:

```markdown
## Summary

- add typed `extra_headers` values to `[llm.providers.<id>]`
- support explicit literal/env/credential header value forms while rejecting bare strings
- cover whole-map header merge behavior and adapter header pass-through tests

Motivated by @haroldolivieri's Portkey/Bedrock report on PR #207:
https://github.com/fabro-sh/fabro/pull/207#issuecomment-4377929769

## Non-goals

- does not make settings-defined providers runnable yet
- does not migrate ProviderId/OpenAPI/auth resolver/runtime catalog plumbing
- does not route Codex OAuth through custom provider settings

## Tests

- `cargo nextest run -p fabro-config -p fabro-llm`
- `cargo +nightly-2026-04-14 fmt --check --all`
- `cargo +nightly-2026-04-14 clippy -p fabro-config -p fabro-llm --all-targets -- -D warnings`
```

## Self-Review

- Spec coverage: This plan covers Phase 1's `extra_headers` schema, header value shape, optional primary credentials at the schema layer, merge behavior, redaction behavior, adapter pass-through seam, documentation, and attribution to `@haroldolivieri`.
- Intentional deferrals: Runtime resolution of `HeaderValueRef` into concrete strings, custom provider registration, ProviderId migration, OpenAPI updates, and provider/model capability overrides remain in later phases.
- Placeholder scan: No task relies on unspecified test names or vague implementation steps. Factory testing uses concrete private builder helpers and does not add runtime trait inspection API.
