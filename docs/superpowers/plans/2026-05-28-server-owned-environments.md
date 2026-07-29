# Server-Owned Environments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move environment definitions from layered run settings into server-owned TOML resources with CRUD API management, matching the Automation store pattern.

**Architecture:** Add a concrete `EnvironmentStore` that loads one environment TOML file per id from a sibling `environments/` directory next to the active server settings file. Runs continue to select an environment by id through `[run.environment]` or `--environment`, but server-side run creation resolves the id from `EnvironmentStore`; project/workflow/user config can no longer define environment catalogs or environment field overrides. The web UI is intentionally deferred.

**Tech Stack:** Rust, Axum, serde/TOML, `toml_edit`, Tokio file I/O, OpenAPI/progenitor, generated TypeScript API client, cargo-nextest.

---

## File Structure

- Create `lib/crates/fabro-environment/`: environment ids, revisions, API/domain DTOs, TOML persistence, canonicalization, validation, and `EnvironmentStore`.
- Modify workspace manifests: root `Cargo.toml`, `lib/crates/fabro-server/Cargo.toml`, `lib/crates/fabro-api/build.rs`, and generated API/client package files.
- Modify `lib/crates/fabro-server/src/server.rs`, `lib/crates/fabro-server/src/server/handler/mod.rs`, and a new `lib/crates/fabro-server/src/server/handler/environments.rs` to wire the store and API.
- Modify `lib/crates/fabro-config/src/builders.rs`, `lib/crates/fabro-config/src/load.rs`, `lib/crates/fabro-config/src/migrations.rs`, and config tests to treat `[environments]` as migration-only, not runtime configuration.
- Modify `lib/crates/fabro-manifest/src/lib.rs`, `lib/crates/fabro-server/src/run_manifest.rs`, and CLI run/preflight/graph/validate paths so environment ids are resolved only by the server.
- Modify install/repo-init/docs/OpenAPI artifacts so new examples use server environment files and run configs only select ids.

## Decisions

- Environment definitions are server-owned operator policy. Project and workflow files may request an id but cannot define or override environment fields.
- `default`, `local`, `docker`, and `daytona` are seeded if missing. Existing files are never overwritten.
- `default` is protected from deletion. Other seeded files can be edited or deleted.
- Environment ids use `[a-z0-9][a-z0-9-]{0,62}`.
- Environment revisions are SHA-256 hashes of the persisted TOML bytes, returned in JSON as `revision` and in `ETag`.
- `PUT` and `DELETE` require `If-Match`, following `AutomationStore`.
- `image.dockerfile = { path = "Dockerfile" }` is accepted in persisted files and API input, resolved relative to the environment file or request context, and converted to inline content for runtime use. API writes canonical inline TOML.
- `--preserve-sandbox` remains a CLI/server argument override. TOML `[run.environment.lifecycle]` is rejected.
- `--docker-image` is rejected with a targeted message directing operators to create or update a server environment.
- Existing dense `WorkflowSettings.environments` stays in the API for compatibility and is populated from the server environment catalog during run resolution.

## Task 1: Add `fabro-environment` Store Crate

**Files:**
- Create: `lib/crates/fabro-environment/Cargo.toml`
- Create: `lib/crates/fabro-environment/src/lib.rs`
- Create: `lib/crates/fabro-environment/src/id.rs`
- Create: `lib/crates/fabro-environment/src/model.rs`
- Create: `lib/crates/fabro-environment/src/store.rs`
- Create: `lib/crates/fabro-environment/src/error.rs`
- Modify: root `Cargo.toml`

- [ ] Create a workspace crate named `fabro-environment`, modeled after `fabro-automation`.
- [ ] Define `EnvironmentId`, `EnvironmentRevision`, and parse/validation errors.
- [ ] Define public DTOs:
  - `Environment`: `id`, `revision`, `provider`, `image`, `resources`, `network`, `lifecycle`, `labels`, `volumes`, `env`.
  - `EnvironmentDraft`: `id` plus environment fields.
  - `EnvironmentReplace`: environment fields without id.
- [ ] Use the existing environment field types from `fabro_types::settings::run` for dense API fields.
- [ ] Use existing sparse `fabro_config::EnvironmentLayer` only for TOML input/output and conversion; do not create a second environment field vocabulary.
- [ ] Add conversion helpers that resolve an `EnvironmentLayer` into dense `EnvironmentSettings` using the same provider/network/image validation rules as `fabro-config`.
- [ ] Implement canonical TOML serialization for persisted files. Omit `id` and `revision`; the filename is the id and the file bytes determine revision.
- [ ] Implement `EnvironmentStore` with `load_or_seed(dir)`, `list`, `get`, `create`, `replace`, `delete`, and `catalog_layer`.
- [ ] Seed missing `default`, `local`, `docker`, and `daytona` files from the current built-in defaults. Do not overwrite existing files.
- [ ] Protect `default` from deletion with a typed store error.
- [ ] Resolve Dockerfile path references relative to the environment file directory during load and relative to the active settings directory during API create/replace. Store runtime values with inline Dockerfile content.
- [ ] Add unit tests for loading an absent directory, seeding built-ins, sorted listing, invalid ids, invalid provider, invalid network mode, missing Dockerfile path, create conflict, replace stale revision, default delete rejection, delete success, and canonical revision changes.

Run:

```bash
cargo nextest run -p fabro-environment
```

Expected: all `fabro-environment` tests pass.

## Task 2: Make Config Environments Migration-Only

**Files:**
- Modify: `lib/crates/fabro-config/src/parse.rs`
- Modify: `lib/crates/fabro-config/src/builders.rs`
- Modify: `lib/crates/fabro-config/src/load.rs`
- Modify: `lib/crates/fabro-config/src/migrations.rs`
- Create: `lib/crates/fabro-config/migrations/2026052801_settings_environments_to_server_files.rs`
- Modify: `lib/crates/fabro-config/src/defaults.toml`
- Modify: `lib/crates/fabro-config/src/tests/resolve_run.rs`
- Modify: `lib/crates/fabro-config/src/tests/resolve_root.rs`

- [ ] Keep `SettingsLayer.environments` in this pass so old files can parse and migrate, but remove environment catalog entries from `defaults.toml`.
- [ ] Add source-aware validation that rejects `SettingsLayer.environments` for project, workflow, and direct run config layers with this message shape: `[environments.<id>] is now server-managed; move this definition to the server environments directory`.
- [ ] Add validation that rejects TOML-provided `run.environment.image`, `resources`, `network`, `lifecycle`, `labels`, `volumes`, and `env`. Keep `run.environment.id`.
- [ ] Ensure CLI/server argument layers can still set `run.environment.lifecycle.preserve` for `--preserve-sandbox`; the rejection applies only to parsed TOML sources.
- [ ] Add a settings-file migration that extracts top-level `[environments.<id>]` entries from the active `settings.toml` into sibling `environments/<id>.toml` files.
- [ ] Migration must write a backup before editing `settings.toml`, preserve `[run.environment] id`, remove the top-level `[environments]` table, and fail without changing files if any target environment file already exists.
- [ ] Chain the existing legacy `[run.sandbox]` migration before the new extraction migration so legacy sandbox settings become a server `default` environment file.
- [ ] Update run settings tests to assert that `RunSettingsBuilder` no longer resolves a selected environment without an injected server catalog.
- [ ] Add tests proving project/workflow `[environments]` definitions produce targeted errors rather than silent ignores.

Run:

```bash
cargo nextest run -p fabro-config
```

Expected: config tests pass, including migration coverage.

## Task 3: Wire EnvironmentStore Into Server Run Resolution

**Files:**
- Modify: `lib/crates/fabro-server/src/server.rs`
- Modify: `lib/crates/fabro-server/src/serve.rs`
- Modify: `lib/crates/fabro-server/src/run_manifest.rs`
- Modify: `lib/crates/fabro-server/src/server/handler/runs.rs`
- Modify: `lib/crates/fabro-server/src/manifest_validation.rs`
- Modify: `lib/crates/fabro-server/src/test_support.rs`

- [ ] Add `environment_store: Arc<EnvironmentStore>` to `AppState`, loaded from `active_config_path.parent().join("environments")`.
- [ ] Replace `manifest_environment_defaults` from `ServerRuntimeSettings` with `environment_store.catalog_layer()` when preparing manifests on the server.
- [ ] Keep the dense run snapshot unchanged: `prepared.settings.run.environment` contains the resolved environment fields, and `prepared.settings.environments` contains the server catalog used for resolution.
- [ ] Convert unknown environment ids into `400 Bad Request` during run creation/preflight/graph preparation.
- [ ] Keep sandbox provider policy checks after environment resolution, so disabled providers still reject runs.
- [ ] Apply `--preserve-sandbox` after selected environment resolution.
- [ ] Remove server reliance on `[environments]` in `settings.toml`.
- [ ] Update server test support so tests can inject environment files or use seeded defaults.
- [ ] Add server tests for default environment run creation, custom server environment selection, unknown environment id, disabled provider policy, `--preserve-sandbox`, and rejected TOML environment field overrides.

Run:

```bash
cargo nextest run -p fabro-server
```

Expected: server API and run-manifest tests pass.

## Task 4: Add Environment CRUD API

**Files:**
- Modify: `docs/public/api-reference/fabro-api.yaml`
- Modify: `lib/crates/fabro-api/build.rs`
- Create: `lib/crates/fabro-server/src/server/handler/environments.rs`
- Modify: `lib/crates/fabro-server/src/server/handler/mod.rs`
- Add tests: `lib/crates/fabro-server/tests/it/api/environments.rs`
- Update generated Rust and TypeScript API artifacts after spec changes.

- [ ] Add OpenAPI tag `Environments`.
- [ ] Add schemas for `Environment`, `CreateEnvironmentRequest`, `ReplaceEnvironmentRequest`, and `EnvironmentListResponse`.
- [ ] Reuse existing environment schemas for provider/image/resources/network/lifecycle/volumes/env.
- [ ] Add endpoints:
  - `GET /api/v1/environments`
  - `POST /api/v1/environments`
  - `GET /api/v1/environments/{id}`
  - `PUT /api/v1/environments/{id}`
  - `DELETE /api/v1/environments/{id}`
- [ ] Return `ETag` on retrieve and replace.
- [ ] Require `If-Match` on replace and delete.
- [ ] Map store errors to API responses:
  - invalid id: `400`
  - duplicate create: `409`
  - stale revision: `409`
  - validation error: `422`
  - missing resource: `404`
  - protected default delete: `409`
  - persistence failure: `500`
- [ ] Add route tests for empty-seeded list, create, retrieve with ETag, replace, stale replace, missing `If-Match`, delete, protected default delete, invalid provider, invalid CIDR, and missing Dockerfile path.
- [ ] Regenerate `fabro-api` and TypeScript client artifacts.

Run:

```bash
cargo build -p fabro-api
cd lib/packages/fabro-api-client && bun run generate
cargo nextest run -p fabro-server --test it -- api::environments
```

Expected: generated artifacts are updated and environment API tests pass.

## Task 5: Adjust CLI And Manifest Behavior

**Files:**
- Modify: `lib/crates/fabro-cli/src/commands/run/overrides.rs`
- Modify: `lib/crates/fabro-cli/src/commands/preflight.rs`
- Modify: `lib/crates/fabro-cli/src/commands/graph.rs`
- Modify: `lib/crates/fabro-cli/src/commands/validate.rs`
- Modify: `lib/crates/fabro-cli/src/commands/repo/init.rs`
- Modify: `lib/crates/fabro-manifest/src/lib.rs`
- Modify CLI integration tests under `lib/crates/fabro-cli/tests/it/`

- [ ] Reject `--docker-image` in run, create, preflight, graph, and validate commands with this message shape: `--docker-image is no longer supported; create or update a server environment and select it with --environment`.
- [ ] Keep `--environment` as an id-only selector in manifest args.
- [ ] Stop collecting Dockerfile path references from `[environments.<id>]` in project/workflow config because those definitions are invalid.
- [ ] Keep collecting Dockerfile references for any remaining CLI-created run environment override only when it comes from allowed argument paths; with `--docker-image` rejected, no normal user path should add one.
- [ ] Update local preflight/graph/validate flows so they either call server preflight for environment resolution or print a clear message that server-owned environment resolution requires a running server.
- [ ] Update `fabro repo init` to write only `[run.environment] id = "local"` and no `[environments.local]` block.
- [ ] Update CLI tests for manifest args, repo init output, rejected `--docker-image`, and server-owned environment selection.

Run:

```bash
cargo nextest run -p fabro-cli
```

Expected: CLI tests pass and no generated workflow config contains `[environments.*]`.

## Task 6: Update Install, Docs, And Generated References

**Files:**
- Modify install persistence code in `lib/crates/fabro-cli/src/commands/install.rs` and server install handlers/tests.
- Modify docs: `docs/public/execution/environments.mdx`, `docs/public/execution/run-configuration.mdx`, `docs/public/reference/user-configuration.mdx`, `docs/public/administration/server-configuration.mdx`, `docs/public/administration/sandboxing.mdx`, `docs/public/integrations/daytona.mdx`, and examples that currently define `[environments.<id>]`.
- Modify generated settings reference if applicable.

- [ ] Update install flows to write server environment files instead of `[environments.default]` into `settings.toml`.
- [ ] Keep install-written `[run.environment] id = "default"` when a default run environment selection is still needed.
- [ ] Update tests that assert `settings.toml` contains `[environments.default]` to assert the sibling environment file exists and settings no longer contains `[environments]`.
- [ ] Rewrite public docs so environment definitions are server-owned TOML files and run configs only select ids.
- [ ] Add a compatibility note explaining that project/workflow `[environments]` definitions now fail and must be moved to the server.
- [ ] Keep `Settings > Environments` UI documentation out of this pass.

Run:

```bash
cargo nextest run -p fabro-server --test it -- api::install
cargo nextest run -p fabro-cli --test it
```

Expected: install tests pass and docs no longer present project/workflow environment definitions as valid.

## Task 7: Workspace Verification

**Files:**
- No new files unless test snapshots require reviewed updates.

- [ ] Run Rust formatting check:

```bash
cargo +nightly-2026-04-14 fmt --check --all
```

- [ ] Run clippy:

```bash
cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings
```

- [ ] Run workspace tests:

```bash
cargo nextest run --workspace
```

- [ ] Run TypeScript checks if API client changes affect the web package:

```bash
cd apps/fabro-web && bun run typecheck
cd apps/fabro-web && bun test
```

- [ ] Inspect generated files and snapshots before accepting any snapshot changes.

## Acceptance Criteria

- Server startup creates or loads `environments/default.toml`, `local.toml`, `docker.toml`, and `daytona.toml`.
- `GET /api/v1/environments` returns seeded environments with revisions.
- API-created environments persist as TOML files and survive server restart.
- Runs using `[run.environment] id = "cloud"` resolve from server files only.
- Project/workflow/user `[environments.<id>]` definitions no longer affect runs.
- Existing runs keep their dense environment snapshot after environment files change.
- `--environment` still works.
- `--preserve-sandbox` still works.
- `--docker-image` no longer works and produces the targeted replacement guidance.
- Web UI changes are not included.
