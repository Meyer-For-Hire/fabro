# Secrets to SQLite sketch

## Goal

Move optional integration secrets from `vaults/default/secrets.json` to the shared SQL database. Preserve the REST contract and secret-type behavior. Make reads and OAuth refresh safe across nodes. Keep bootstrap secrets in process env or `server.env`.

Encryption-at-rest is out of scope. This migration preserves the current plaintext-at-rest semantics; application-level encryption will be a separate PR.

## Boundary

- SQL: token, OAuth, and file secrets currently owned by `fabro-vault`.
- Not SQL: `SESSION_SECRET`, dev token, database/object-store bootstrap credentials, worker bearer tokens.
- `fabro-vault` remains the owning shared crate. Replace file persistence with `SecretStore { pool }`.
- No process-wide entry cache. Fetch on each operation; allow short-lived snapshots for one run/config-resolution operation.

## Schema

Schema:

```sql
CREATE TABLE secrets (
    name TEXT PRIMARY KEY NOT NULL,
    secret_type TEXT NOT NULL,
    value TEXT NOT NULL,
    description TEXT,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,

    CHECK (length(name) > 0),
    CHECK (secret_type IN ('token', 'oauth', 'file')),
    CHECK (revision > 0),
    CHECK (length(created_at) > 0),
    CHECK (length(updated_at) > 0),
    CHECK (
        (secret_type IN ('token', 'oauth')
            AND substr(name, 1, 1) GLOB '[A-Za-z_]'
            AND name NOT GLOB '*[^A-Za-z0-9_]*')
        OR
        (secret_type = 'file'
            AND substr(name, 1, 1) = '/'
            AND substr(name, -1, 1) <> '/')
    )
);
```

Notes:

- `revision` is internal optimistic concurrency. Ordinary API upserts increment it. OAuth refresh writes `WHERE name = ? AND revision = ?`; a loser reloads the winning credential.
- Keep timestamps as RFC 3339 text to match current SQLite tables. PostgreSQL migration can map them to `timestamptz` behind the store API.
- Keep full semantic validation in Rust and revalidate decoded database rows. SQLite checks provide defense in depth; PostgreSQL gets equivalent backend-specific constraints later.
- `value` deliberately preserves current plaintext-at-rest behavior. A follow-up encryption PR should migrate it to a versioned ciphertext envelope and introduce key provisioning/rotation without changing the caller-facing API.

No secondary index: secret sets are small and every supported query is primary-key lookup or full sorted listing.

## Store API

```rust
pub struct SecretStore { /* pool */ }

pub async fn get(&self, name: &str) -> Result<Option<SecretEntry>, Error>;
pub async fn list(&self) -> Result<Vec<SecretMetadata>, Error>;
pub async fn set(
    &self,
    name: &str,
    value: &str,
    secret_type: SecretType,
    description: Option<&str>,
) -> Result<SecretMetadata, Error>;
pub async fn remove(&self, name: &str) -> Result<(), Error>;
pub async fn replace_if_revision(
    &self,
    expected_revision: i64,
    entry: SecretReplacement<'_>,
) -> Result<SecretEntry, Error>;
pub async fn snapshot(&self) -> Result<SecretSnapshot, Error>;
```

- Preserve created time and existing description when an upsert omits description.
- `SecretEntry` carries internal revision; `SecretMetadata` and OpenAPI remain unchanged.
- Derive `EnumString` and `IntoStaticStr` for `SecretType`, aligned with serde/strum names.
- Typed errors: invalid name, not found, stale revision, invalid stored row, database failure, legacy import failure. Preserve sources; API handlers return curated messages.
- Never implement `Debug` for a value-bearing type by deriving it. Redact or omit values and OAuth JSON.

## Read model and concurrency

- Replace `Arc<RwLock<Vault>>` in `AppStores` with `Arc<SecretStore>`.
- Convert `AppState::vault_secret` to async and fallible. Do not use `try_read()` or map database failure to “missing.”
- Refactor `VaultCredentialSource`/`CredentialResolver` to load an operation-scoped snapshot. Credential resolution stays internally synchronous over that snapshot; storage I/O is async at the boundary.
- OAuth refresh: load entry + revision, refresh remotely, CAS the new credential. On stale revision, reload and use the winner if valid; never overwrite a newer refresh token.
- Workflow/run creation and worker startup may take one snapshot for consistent interpolation and credential selection. A running worker does not observe secret rotation; the next request or worker sees later writes from any node.

## Startup and legacy import

New order:

1. Load settings and bootstrap `ServerSecrets`.
2. Connect SQL and run schema migrations.
3. Normalize any supported legacy vault JSON shape.
4. Parse and validate the complete `secrets.json`; insert transactionally with `ON CONFLICT(name) DO NOTHING`. SQL wins.
5. Rename source to `secrets.json.imported-<timestamp>.bak`, preserving private permissions.
6. Move the temporary optional-`server.env` migration to target `SecretStore`; back up and clean `server.env` only after SQL contains the intended value.
7. Query required vault-only startup secrets, then resolve auth and integrations.

Import is state-driven and retry-safe. Missing source is a no-op. Invalid JSON/name/type/OAuth payload leaves the source untouched. Logs contain paths, counts, and names only—never values or serialized rows.

## Call-site migration

1. Add schema, `SecretStore`, row validation, CAS, and store tests in `fabro-vault`.
2. Add legacy importer and reorder server startup before auth resolution.
3. Update secrets handlers and all server secret lookups to async/fallible access.
4. Refactor `fabro-auth` credential resolution and OAuth refresh around snapshots + CAS.
5. Update install persistence to write SQL. Make direct persistence async; preserve file rollback behavior around settings/`server.env` and use a SQL transaction for the secret batch.
6. Update CLI/local-run and worker startup to open the same database instead of loading JSON. Keep test helpers behind `test-support` boundaries.
7. Remove live JSON writes and `Storage::secrets_path()` production consumers. Retain only the legacy import path until its removal deadline.
8. Update operator docs: backup behavior, plaintext-at-rest scope, and SQLite/PostgreSQL boundary. Track application-level encryption as a separate follow-up.

## Tests

- Schema constraints and invalid stored-row decoding.
- CRUD parity: sorting, empty values, all types, description preservation, timestamps.
- Two independent pools observe writes immediately.
- Concurrent upsert and delete behavior.
- OAuth CAS: one winner; loser reloads without clobbering rotated refresh token.
- Redacted `Debug`/errors and no secret values in diagnostics.
- Import: absent, success, second-run no-op, SQL wins, invalid file unchanged, backup permissions, rename failure retry.
- Startup auth reads imported GitHub client secret before auth validation.
- Install, CLI local run, worker, API persistence, restart, and multi-pool integration coverage.
- Assert logs/errors/API bodies never contain fixture secret values.

Verification:

```text
cargo nextest run -p fabro-db -p fabro-vault -p fabro-auth
cargo nextest run -p fabro-server --features test-support secrets
cargo nextest run -p fabro-server --features test-support install
cargo nextest run -p fabro-cli --features test-support
cargo build --workspace
cargo +nightly-2026-04-14 fmt --check --all
cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings
```

## Unresolved questions

None.
