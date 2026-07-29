# Align Concrete Stores Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align automation, secret, and variable storage around concrete product-facing store types that own their own synchronization and persistence.

**Architecture:** `AutomationStore` is already the desired shape and remains the reference. Collapse the public `Vault` type into `SecretStore`, convert `VariableStore` to the same internally synchronized concrete-store pattern, and update server/auth/CLI call sites to depend on those concrete stores directly. Do not introduce storage traits, SQL dispatch, backend enums, or file-format migrations in this pass.

**Tech Stack:** Rust, Tokio `Mutex`/`RwLock`, JSON file persistence for secrets and variables, existing automation TOML storage, cargo-nextest.

---

## File Structure

- `lib/crates/fabro-vault/src/lib.rs`: rename `Vault` to `SecretStore`, add internal synchronization, keep `SecretEntry`, `SecretType`, validation, and JSON file format compatible.
- `lib/crates/fabro-variable/src/lib.rs`: make `VariableStore` internally synchronized and keep its existing JSON file format compatible.
- `lib/crates/fabro-auth/src/vault_source.rs`, `lib/crates/fabro-auth/src/vault_ext.rs`, `lib/crates/fabro-auth/src/resolve.rs`: update secret-store naming and remove external `AsyncRwLock<Vault>` assumptions from auth resolution.
- `lib/crates/fabro-server/src/server.rs`, `lib/crates/fabro-server/src/startup.rs`, `lib/crates/fabro-server/src/migrations.rs`, and server handlers: update state wiring and call sites from externally locked `Vault`/`VariableStore` to direct `Arc<SecretStore>`/`Arc<VariableStore>`.
- CLI and tests that load local secrets directly: update imports and names from `Vault` to `SecretStore`.

## Task 1: Convert `Vault` Into Concrete `SecretStore`

**Files:**
- Modify: `lib/crates/fabro-vault/src/lib.rs`

- [ ] **Step 1: Update the store shape**

Change the public store struct from `Vault` to `SecretStore`, and give it the same synchronization shape as `AutomationStore`:

```rust
use tokio::sync::{Mutex, RwLock};

#[derive(Debug)]
pub struct SecretStore {
    path:      PathBuf,
    mutations: Mutex<()>,
    entries:   RwLock<HashMap<String, SecretEntry>>,
}
```

Add `tokio.workspace = true` to `lib/crates/fabro-vault/Cargo.toml`.

- [ ] **Step 2: Make load asynchronous**

Replace `Vault::load` with:

```rust
impl SecretStore {
    pub async fn load(path: PathBuf) -> Result<Self, Error> {
        let entries = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => serde_json::from_str(&contents)?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(err) => return Err(io_context("read secrets", &path, &err).into()),
        };

        Ok(Self {
            path,
            mutations: Mutex::new(()),
            entries: RwLock::new(entries),
        })
    }
}
```

- [ ] **Step 3: Convert read methods to internally locked methods**

Use these signatures and preserve existing sort/order behavior:

```rust
pub async fn list(&self) -> Vec<SecretMetadata>;
pub async fn get(&self, name: &str) -> Option<String>;
pub async fn get_entry(&self, name: &str) -> Option<SecretEntry>;
pub async fn file_secrets(&self) -> Vec<(String, String)>;
```

`get` must return an owned `String`, and `get_entry` must return a cloned `SecretEntry`, because callers no longer hold a read guard over the store internals.

- [ ] **Step 4: Convert mutation methods to internally locked methods**

Use these signatures:

```rust
pub async fn set(
    &self,
    name: &str,
    value: &str,
    secret_type: SecretType,
    description: Option<&str>,
) -> Result<SecretMetadata, Error>;

pub async fn remove(&self, name: &str) -> Result<(), Error>;
```

Implementation order for `set`:

```rust
Self::validate_name(name, secret_type)?;
let _mutation = self.mutations.lock().await;
let mut entries = self.entries.write().await;
let now = Utc::now();
let (created_at, description) = entries.get(name).map_or_else(
    || (now, description.map(str::to_string)),
    |entry| {
        (
            entry.created_at,
            description.map(str::to_string).or_else(|| entry.description.clone()),
        )
    },
);
let entry = SecretEntry {
    value: value.to_string(),
    secret_type,
    description: description.clone(),
    created_at,
    updated_at: now,
};
let mut next_entries = entries.clone();
next_entries.insert(name.to_string(), entry);
self.write_atomic(&next_entries).await?;
*entries = next_entries;
Ok(SecretMetadata {
    name: name.to_string(),
    secret_type,
    description,
    created_at,
    updated_at: now,
})
```

Implementation order for `remove`:

```rust
let _mutation = self.mutations.lock().await;
let mut entries = self.entries.write().await;
if !entries.contains_key(name) {
    return Err(Error::NotFound(name.to_string()));
}
let mut next_entries = entries.clone();
next_entries.remove(name);
self.write_atomic(&next_entries).await?;
*entries = next_entries;
Ok(())
```

This preserves in-memory state when the durable write fails.

- [ ] **Step 5: Convert `write_atomic` to async**

Use `tokio::fs` and `tokio::io::AsyncWriteExt` like `AutomationStore`, and keep private permissions for secret files:

```rust
async fn write_atomic(&self, entries: &HashMap<String, SecretEntry>) -> Result<(), Error> {
    let parent = self
        .path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    tokio::fs::create_dir_all(&parent)
        .await
        .map_err(|err| io_context("create secrets directory", &parent, &err))?;

    let file_name = self
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("secrets.json");
    let tmp_path = parent.join(format!(".{file_name}.tmp-{}", ulid::Ulid::new()));
    let json = serde_json::to_vec_pretty(entries)?;

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .await
        .map_err(|err| io_context("create secrets temp file", &tmp_path, &err))?;
    file.write_all(&json)
        .await
        .map_err(|err| io_context("write secrets temp file", &tmp_path, &err))?;
    file.sync_all()
        .await
        .map_err(|err| io_context("sync secrets temp file", &tmp_path, &err))?;
    drop(file);
    set_private_permissions(&tmp_path)?;
    tokio::fs::rename(&tmp_path, &self.path).await.map_err(|err| {
        io_context(
            &format!("rename secrets temp file to {}", self.path.display()),
            &tmp_path,
            &err,
        )
    })?;
    Ok(())
}
```

- [ ] **Step 6: Run focused secret-store tests**

Run:

```bash
cargo nextest run -p fabro-vault
```

Expected: tests initially fail until test imports and async calls are updated in Task 2.

## Task 2: Update Secret Store Tests

**Files:**
- Modify: `lib/crates/fabro-vault/src/lib.rs`

- [ ] **Step 1: Convert existing tests to Tokio tests**

Use `#[tokio::test]` for tests that call `SecretStore::load`, `set`, `remove`, `list`, `get`, `get_entry`, or `file_secrets`.

Example conversion:

```rust
#[tokio::test]
async fn set_creates_entry_and_writes_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secrets.json");
    let store = SecretStore::load(path.clone()).await.unwrap();

    let meta = store
        .set("OPENAI_API_KEY", "secret", SecretType::Token, None)
        .await
        .unwrap();

    assert_eq!(meta.name, "OPENAI_API_KEY");
    assert_eq!(meta.secret_type, SecretType::Token);
    assert_eq!(store.get("OPENAI_API_KEY").await.as_deref(), Some("secret"));
    assert!(path.exists());
}
```

- [ ] **Step 2: Keep serde compatibility test coverage**

Preserve tests that prove:

```rust
let reloaded = SecretStore::load(path).await.unwrap();
assert_eq!(
    reloaded.file_secrets().await,
    vec![("/tmp/key.pem".to_string(), "pem".to_string())]
);
```

- [ ] **Step 3: Run focused tests**

Run:

```bash
cargo nextest run -p fabro-vault
```

Expected: PASS.

## Task 3: Convert `VariableStore` to the Same Concrete Store Pattern

**Files:**
- Modify: `lib/crates/fabro-variable/src/lib.rs`
- Modify: `lib/crates/fabro-variable/Cargo.toml`

- [ ] **Step 1: Add internal synchronization**

Add `tokio.workspace = true` to `lib/crates/fabro-variable/Cargo.toml`.

Change the store shape to:

```rust
use tokio::sync::{Mutex, RwLock};

#[derive(Debug)]
pub struct VariableStore {
    path:      PathBuf,
    mutations: Mutex<()>,
    entries:   RwLock<HashMap<String, VariableEntry>>,
}
```

- [ ] **Step 2: Make load asynchronous**

Use:

```rust
pub async fn load(path: PathBuf) -> Result<Self, Error> {
    let entries = match tokio::fs::read_to_string(&path).await {
        Ok(contents) => serde_json::from_str(&contents)?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
        Err(err) => return Err(io_context("read variables", &path, &err).into()),
    };

    Ok(Self {
        path,
        mutations: Mutex::new(()),
        entries: RwLock::new(entries),
    })
}
```

- [ ] **Step 3: Convert read and mutation methods**

Use these signatures:

```rust
pub async fn set(&self, name: &str, value: &str, description: Option<&str>) -> Result<Variable, Error>;
pub async fn update_existing(&self, name: &str, value: &str, description: Option<&str>) -> Result<Variable, Error>;
pub async fn get(&self, name: &str) -> Option<Variable>;
pub async fn get_value(&self, name: &str) -> Option<String>;
pub async fn list(&self) -> Vec<Variable>;
pub async fn remove(&self, name: &str) -> Result<(), Error>;
```

Use the same copy-then-write-then-swap mutation pattern from `SecretStore` so failed durable writes do not change memory.

- [ ] **Step 4: Convert variable file writes to async**

Replace synchronous write calls with `tokio::fs` and `tokio::io::AsyncWriteExt`, using the same temp-file and rename pattern as `SecretStore`. The output JSON shape must remain `HashMap<String, VariableEntry>`.

- [ ] **Step 5: Update variable tests**

Convert tests in `lib/crates/fabro-variable/tests/store.rs` to `#[tokio::test]` and add `.await` to store calls.

Example:

```rust
#[tokio::test]
async fn update_existing_requires_existing_variable() {
    let dir = tempfile::tempdir().unwrap();
    let store = VariableStore::load(dir.path().join("variables.json")).await.unwrap();

    let err = store
        .update_existing("MISSING", "value", None)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::NotFound(name) if name == "MISSING"));
}
```

- [ ] **Step 6: Run focused variable-store tests**

Run:

```bash
cargo nextest run -p fabro-variable
```

Expected: PASS.

## Task 4: Update Auth Secret Helpers and Credential Source Naming

**Files:**
- Modify: `lib/crates/fabro-auth/src/lib.rs`
- Modify: `lib/crates/fabro-auth/src/vault_ext.rs`
- Modify: `lib/crates/fabro-auth/src/vault_source.rs`
- Modify: `lib/crates/fabro-auth/src/resolve.rs`

- [ ] **Step 1: Rename public auth helper concepts**

Keep file names for this task if that minimizes churn, but update public symbols:

```rust
pub use secret_ext::{
    SecretLookupError, secret_get_oauth, secret_get_token, secret_set_oauth, secret_set_token,
};
pub use secret_source::SecretCredentialSource;
```

If files are renamed, update `mod vault_ext;` to `mod secret_ext;` and `mod vault_source;` to `mod secret_source;`.

- [ ] **Step 2: Update helper signatures**

Replace `Vault` references with `SecretStore` and async store calls. Use:

```rust
pub async fn secret_get_token(
    secrets: &SecretStore,
    name: &str,
) -> Result<Option<String>, SecretLookupError>;

pub async fn secret_get_oauth(
    secrets: &SecretStore,
    name: &str,
) -> Result<Option<OAuthCredential>, SecretLookupError>;

pub async fn secret_set_token(
    secrets: &SecretStore,
    name: &str,
    value: &str,
) -> Result<SecretMetadata, SecretStoreError>;

pub async fn secret_set_oauth(
    secrets: &SecretStore,
    name: &str,
    credential: &OAuthCredential,
) -> Result<SecretMetadata, SecretStoreError>;
```

- [ ] **Step 3: Update `CredentialResolver`**

Change:

```rust
vault: Arc<AsyncRwLock<Vault>>,
```

to:

```rust
secrets: Arc<SecretStore>,
```

Remove explicit `read().await` / `blocking_write()` usage and call `SecretStore` methods directly. `configured_providers_from_process_env` should accept `Option<&Arc<SecretStore>>`.

- [ ] **Step 4: Update OAuth refresh persistence**

Replace the `spawn_blocking` refresh write with:

```rust
secret_set_oauth(&self.secrets, &vault_name_for_store, &refreshed_for_store)
    .await
    .map(|_| ())
    .map_err(|source| ResolveError::RefreshFailed {
        provider: provider_id.clone(),
        source: anyhow::Error::from(source),
    })?;
```

- [ ] **Step 5: Run auth tests**

Run:

```bash
cargo nextest run -p fabro-auth
```

Expected: PASS.

## Task 5: Update Server State and HTTP Handlers

**Files:**
- Modify: `lib/crates/fabro-server/src/server.rs`
- Modify: `lib/crates/fabro-server/src/server/handler/secrets.rs`
- Modify: `lib/crates/fabro-server/src/server/handler/variables.rs`
- Modify: `lib/crates/fabro-server/src/server/handler/runs.rs`

- [ ] **Step 1: Update `AppState` fields**

Change:

```rust
pub(crate) vault: Arc<AsyncRwLock<Vault>>,
pub(crate) variables: Arc<AsyncRwLock<VariableStore>>,
```

to:

```rust
pub(crate) secrets: Arc<SecretStore>,
pub(crate) variables: Arc<VariableStore>,
```

- [ ] **Step 2: Update app-state construction**

Use:

```rust
let variables = Arc::new(VariableStore::load(variables_path).await.context("load variables")?);
let secrets = match preloaded_secrets {
    Some(secrets) => secrets,
    None => load_startup_secrets(&vault_path).await?,
};
let daytona_api_key = secrets.get(EnvVars::DAYTONA_API_KEY).await;
let secrets = Arc::new(secrets);
let llm_source: Arc<dyn CredentialSource> =
    Arc::new(SecretCredentialSource::secrets_only(Arc::clone(&secrets)));
```

Make `build_app_state` async if needed, and update callers to `.await`.

- [ ] **Step 3: Rename the secret lookup helper**

Replace `vault_secret` with:

```rust
pub(crate) async fn secret_value(&self, name: &str) -> Option<String> {
    self.secrets.get(name).await
}
```

Update call sites to await this method. If a call site is synchronous and cannot be made async without large churn, use `self.secrets.try_get(name)` only if a non-blocking read helper is added to `SecretStore`; otherwise convert the caller to async.

- [ ] **Step 4: Update secrets handler**

Remove handler-level `spawn_blocking` and external lock access. Use:

```rust
let data = state.secrets.list().await;
let result = state
    .secrets
    .set(&name, &value, secret_type, description.as_deref())
    .await;
let result = state.secrets.remove(&name).await;
```

Keep existing validation for bootstrap secrets, OAuth JSON, and Daytona key checks.

- [ ] **Step 5: Update variables handler**

Remove handler-level `spawn_blocking` and external lock access. Use:

```rust
let data = state.variables.list().await;
let result = state.variables.set(&name, &value, description.as_deref()).await;
let result = state
    .variables
    .update_existing(&name, &value, description.as_deref())
    .await;
let result = state.variables.remove(&name).await;
```

Update run variable substitution to call:

```rust
let variables = Arc::clone(&state.variables);
settings.substitute_variables_async(|name| {
    let variables = Arc::clone(&variables);
    async move { variables.get_value(name).await }
})
```

If `substitute_variables_async` does not exist, add a small local snapshot helper:

```rust
let variable_values = state.variables.values_map().await;
settings.substitute_variables(|name| variable_values.get(name).cloned())
```

Prefer the snapshot helper because it keeps variable interpolation synchronous after one async read.

- [ ] **Step 6: Run focused server tests**

Run:

```bash
cargo nextest run -p fabro-server variables secrets
```

Expected: PASS for matching test filters. If nextest reports no tests for one filter, run the full crate command in Task 8.

## Task 6: Update Startup, Migrations, Install, and CLI Call Sites

**Files:**
- Modify: `lib/crates/fabro-server/src/startup.rs`
- Modify: `lib/crates/fabro-server/src/migrations.rs`
- Modify: `lib/crates/fabro-server/migrations/2026052501_optional_server_env_secrets_to_vault.rs`
- Modify: CLI/server tests and direct secret-store imports found by `rg -n "Vault|vault" lib/crates`.

- [ ] **Step 1: Rename startup functions**

Use these names:

```rust
pub async fn load_startup_secrets(vault_path: impl AsRef<Path>) -> anyhow::Result<SecretStore>;
pub async fn prepare_startup_secrets(
    vault_path: impl AsRef<Path>,
    server_env_path: impl AsRef<Path>,
    env_entries: &HashMap<String, String>,
) -> anyhow::Result<SecretStore>;
```

Keep the storage path variable name `vault_path` only where it refers to the existing on-disk `vaults/default/secrets.json` path.

- [ ] **Step 2: Update optional env migration**

Change migration input from `&mut Vault` to `&SecretStore`, and call async `set`/`get` methods. Make the migration function async:

```rust
pub(crate) async fn migrate(
    secrets: &SecretStore,
    server_env_path: &Path,
    env_entries: &HashMap<String, String>,
) -> anyhow::Result<OptionalServerEnvSecretsMigrationReport>
```

- [ ] **Step 3: Update direct CLI/test imports**

Replace direct imports:

```rust
use fabro_vault::{SecretType, Vault};
```

with:

```rust
use fabro_vault::{SecretStore, SecretType};
```

Replace `Vault::load(...).unwrap()` with `SecretStore::load(...).await.unwrap()` in async tests. For synchronous tests that only inspect a file after a command, convert the test to `#[tokio::test]` if it is not already async.

- [ ] **Step 4: Update worker runner secret loading**

Change worker storage from `Option<Arc<AsyncRwLock<Vault>>>` to `Option<Arc<SecretStore>>`. Update GitHub credential helpers to accept `Option<&SecretStore>` and call async secret access. If the surrounding function is synchronous, make it async and update its callers.

- [ ] **Step 5: Search until legacy public names are gone**

Run:

```bash
rg -n "\bVault\b|VaultCredentialSource|vault_get_|vault_set_|state\.vault|vault_secret" lib/crates
```

Expected: no matches outside migration names, comments that explicitly describe historical vault files, or file paths containing `vaults/default/secrets.json`.

## Task 7: Preserve Compatibility and Naming Boundaries

**Files:**
- Modify: docs or comments only where old public names appear in changed code.

- [ ] **Step 1: Keep file paths and wire names stable**

Do not rename:

```rust
Storage::secrets_path()
```

Do not change the path:

```text
vaults/default/secrets.json
```

Do not change JSON field names in `SecretEntry`:

```rust
#[serde(rename = "type", default)]
pub secret_type: SecretType,
```

- [ ] **Step 2: Keep existing API response shapes stable**

Secrets list response remains:

```json
{ "data": [ { "name": "...", "type": "token", "created_at": "...", "updated_at": "..." } ] }
```

Variables list response remains:

```json
{ "data": [ { "name": "...", "value": "...", "created_at": "...", "updated_at": "..." } ] }
```

- [ ] **Step 3: Avoid introducing future abstractions**

Do not add:

```rust
trait SecretStore
trait VariableStore
enum SecretStoreBackend
enum VariableStoreBackend
SqlSecretStore
SqlVariableStore
```

This plan is only for concrete local stores.

## Task 8: Full Verification

**Files:**
- No source edits in this task.

- [ ] **Step 1: Run focused crates**

Run:

```bash
cargo nextest run -p fabro-vault
cargo nextest run -p fabro-variable
cargo nextest run -p fabro-auth
cargo nextest run -p fabro-server
```

Expected: PASS.

- [ ] **Step 2: Run formatting check**

Run:

```bash
cargo +nightly-2026-04-14 fmt --check --all
```

Expected: PASS.

- [ ] **Step 3: Run workspace clippy**

Run:

```bash
cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings
```

Expected: PASS.

- [ ] **Step 4: Run name-boundary search**

Run:

```bash
rg -n "\bVault\b|VaultCredentialSource|vault_get_|vault_set_|state\.vault|vault_secret" lib/crates
```

Expected: only intentional historical references remain, such as migration comments or on-disk path references. Any active Rust type/function references should be renamed before completion.

## Self-Review Checklist

- [ ] No storage trait, backend enum, SQL type, or feature flag was added.
- [ ] `AutomationStore` behavior and file layout were not changed.
- [ ] Secrets and variables keep their current JSON file formats.
- [ ] Server handlers no longer take external write locks around secret/variable stores.
- [ ] Public code uses `SecretStore` terminology instead of `Vault`.
- [ ] Tests cover local file compatibility for both stores.
