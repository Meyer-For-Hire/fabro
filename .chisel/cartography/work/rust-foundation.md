# Rust foundation cartography scout

Assessed revision: `2bcf94fed8a9b429f18d9196fa824711d6f4cb0a` (`2bcf94fed`)

Scope: all 365 tracked files under `lib/foundation/**`. Root and consumer manifests, the OpenAPI specification, and public consumer entry points were consulted only as boundary evidence and are not part of this scope's coverage counts.

Applicable instructions read: `AGENTS.md` and `CONTRIBUTING.md` (`CLAUDE.md` is a symlink to `AGENTS.md`).

## Boundary approach

- Most foundation crates are proposed as components in their own right because their manifests, crate-root facades, public state or lifecycle, focused tests, and reverse dependency edges describe a distinct responsibility.
- `build-support` and `fabro-dev` are grouped as `fabro-build-tooling`: the two-file build-support crate would otherwise be too narrow for a stable assessment, and both crates serve repository build/development lifecycle rather than product runtime.
- `fabro-macros` and `fabro-options-metadata` are grouped as `fabro-macros-metadata`: the proc-macro crate cannot expose runtime metadata itself, and the `OptionsMetadata` derive and runtime visitor model form one compiler/runtime contract. The proc-macro crate's `Combine` and `e2e_test` entry points remain part of that compiler-support component.
- The small `fabro-http`, `fabro-proc`, and `fabro-static` crates remain separate. Each is a dependency hub with a distinct public policy boundary (HTTP construction/proxy policy, OS process primitives, and shared string registries respectively), so grouping them would mix independent reasons to change.
- `fabro-types` and `fabro-util` remain crate-level components. Their crate-root facades and cross-module use are the stable public boundaries available at this revision; a finer file-family split would not have an independent manifest or facade and would create overlapping conceptual ownership.
- Production and normal compile-time internal dependencies are listed below. Dev-only edges to `fabro-test` are omitted except for the test-support component itself.

## Proposed components

### `fabro-build-tooling` — Fabro build and developer tooling

- **File count:** 23
- **Purpose:** Runs repository development, build, documentation, SPA, container, benchmark, and release automation and supplies compile-time Git metadata to product build scripts.
- **Globs:** `lib/foundation/build-support/**`, `lib/foundation/fabro-dev/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-dev/src/main.rs:main`, `lib/foundation/fabro-dev/src/lib.rs:run`, `lib/foundation/build-support/git_metadata.rs:collect_from`, `lib/foundation/build-support/git_metadata.rs:cargo_profile`
- **Owns:** developer CLI command dispatch; subprocess plans for build/docs/SPA/Docker/release/test benchmarking; reference generation checks; compile-time Git SHA, rerun paths, and Cargo profile discovery.
- **Depends on candidates:** `fabro-config`, `fabro-macros-metadata`, `fabro-util`
- **Evidence:**
  - `lib/foundation/fabro-dev/Cargo.toml` — declares an internal `fabro-dev` binary/library and integration-test target behind the `dev` feature.
  - `lib/foundation/fabro-dev/src/lib.rs:Command` — dispatches the build, Docker, docs, release, SPA, and benchmark command families.
  - `lib/foundation/fabro-dev/src/commands/mod.rs:PlannedCommand` — centralizes the subprocess lifecycle shared by those commands.
  - `lib/foundation/fabro-dev/tests/it/main.rs` — provides the integration-test composition root for the developer CLI.
  - `lib/foundation/build-support/Cargo.toml` and `lib/foundation/build-support/git_metadata.rs:BuildGitMetadata` — define a build-script-only support crate whose public result is embedded Git/build metadata; `lib/apps/fabro-cli/Cargo.toml` and `lib/apps/fabro-server/Cargo.toml` consume it as a build dependency.

### `fabro-api` — Generated API contract and Rust client

- **File count:** 60
- **Purpose:** Generates the low-level Rust HTTP client and API type surface from the OpenAPI contract while reusing canonical Fabro domain types and verifying wire/type parity.
- **Globs:** `lib/foundation/fabro-api/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-api/build.rs:main`, `lib/foundation/fabro-api/src/lib.rs:ApiClient`, `lib/foundation/fabro-api/src/lib.rs:types`
- **Owns:** OpenAPI-to-Progenitor compatibility transformations; generated-client configuration; canonical type replacement map; low-level generated client facade; API/domain type identity and JSON round-trip tests.
- **Depends on candidates:** `fabro-config`, `fabro-model`, `fabro-types`
- **External dependency edges:** API types are also replaced with types from the `fabro-automation` and `fabro-environment` components.
- **Evidence:**
  - `lib/foundation/fabro-api/Cargo.toml` — describes generated Rust types and HTTP client and declares `build.rs` generation dependencies.
  - `lib/foundation/fabro-api/build.rs:main` — reads `docs/public/api-reference/fabro-api.yaml`, patches the generator view, registers canonical type replacements, and writes `OUT_DIR/codegen.rs`.
  - `lib/foundation/fabro-api/src/lib.rs:generated` — includes the generated file behind a private module and exposes `ApiClient` plus a type facade.
  - `lib/foundation/fabro-api/tests/run_event_round_trip.rs:run_event_reuses_canonical_type` and the other `tests/*_round_trip.rs` files — verify type identity and OpenAPI JSON shape across the exported contract.
  - `docs/public/api-reference/fabro-api.yaml` — repository instructions identify this out-of-scope file as the HTTP contract source of truth.

### `fabro-auth` — Provider credential resolution

- **File count:** 16
- **Purpose:** Resolves provider credentials and interpolated headers from environment or vault sources, refreshes OAuth credentials, and drives interactive authentication strategies.
- **Globs:** `lib/foundation/fabro-auth/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-auth/src/resolve.rs:CredentialResolver`, `lib/foundation/fabro-auth/src/credential_source.rs:CredentialSource`, `lib/foundation/fabro-auth/src/strategy.rs:AuthStrategy`, `lib/foundation/fabro-auth/src/sql_vault_source.rs:SqlVaultCredentialSource`
- **Owns:** provider credential-source precedence; API authorization/header material; configured-provider discovery; OAuth refresh and vault write-back; API-key and Codex-device login strategy state.
- **Depends on candidates:** `fabro-http`, `fabro-model`, `fabro-oauth`, `fabro-redact`, `fabro-static`, `fabro-types`, `fabro-vault`
- **Evidence:**
  - `lib/foundation/fabro-auth/Cargo.toml` — describes typed provider credential storage/resolution and declares the model, OAuth, redaction, vault, HTTP, and type dependencies.
  - `lib/foundation/fabro-auth/src/lib.rs` — exposes sources, resolver, strategies, refresh, and vault adapters as the crate facade.
  - `lib/foundation/fabro-auth/src/resolve.rs:CredentialResolver::resolve` — composes catalog policy, vault/environment lookup, header interpolation, and OAuth refresh into the provider-facing credential.
  - `lib/foundation/fabro-auth/src/credential_source.rs:CredentialSource` — provides the source abstraction used by environment, in-memory vault, and SQLite-backed vault implementations.
  - `lib/foundation/fabro-auth/src/sql_vault_source.rs:SqlVaultCredentialSource::persist_oauth_refreshes` — owns revision-aware persistence of refreshed OAuth state.

### `fabro-client` — High-level Fabro service client

- **File count:** 9
- **Purpose:** Provides the high-level authenticated Fabro service client over HTTP or Unix sockets, including endpoint operations, SSE streams, token refresh, target normalization, and local CLI auth storage.
- **Globs:** `lib/foundation/fabro-client/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-client/src/client.rs:Client::builder`, `lib/foundation/fabro-client/src/client.rs:ClientBuilder::connect`, `lib/foundation/fabro-client/src/target.rs:ServerTarget`, `lib/foundation/fabro-client/src/auth_store.rs:AuthStore`, `lib/foundation/fabro-client/src/client.rs:RunEventStream`
- **Owns:** connected client transport state; API operation wrappers and error classification; OAuth refresh coordination; HTTP/Unix target canonicalization; SSE buffering; per-server CLI authentication file and locking lifecycle.
- **Depends on candidates:** `fabro-api`, `fabro-http`, `fabro-model`, `fabro-static`, `fabro-types`, `fabro-util`
- **Evidence:**
  - `lib/foundation/fabro-client/Cargo.toml` — distinguishes the typed high-level client from the generated `fabro-api` dependency.
  - `lib/foundation/fabro-client/src/client.rs:ClientState` and `Client` — own the generated client, raw HTTP client, bearer token, base URL, refresh lock, and optional transport reconnection.
  - `lib/foundation/fabro-client/src/target.rs:ServerTarget::build_public_http_client` — defines the HTTP-versus-Unix-socket transport boundary.
  - `lib/foundation/fabro-client/src/auth_store.rs:AuthStore` — owns the locked local authentication file lifecycle.
  - `lib/foundation/fabro-client/src/lib.rs` — exposes the client, streams, credential, error, session, store, and target facade consumed by CLI/server/tool applications.

### `fabro-config` — Layered configuration and runtime paths

- **File count:** 52
- **Purpose:** Parses, combines, migrates, validates, and resolves Fabro configuration layers into runtime settings and canonical storage/runtime paths.
- **Globs:** `lib/foundation/fabro-config/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-config/src/builders.rs:ServerSettingsBuilder`, `lib/foundation/fabro-config/src/builders.rs:RunSettingsBuilder`, `lib/foundation/fabro-config/src/builders.rs:load_server_runtime_settings`, `lib/foundation/fabro-config/src/lib.rs:load_config_file`, `lib/foundation/fabro-config/src/resolve/mod.rs`
- **Owns:** source-layer structs and merge semantics; built-in defaults; settings parsing/validation/resolution; configuration compatibility migrations; home, storage, runtime-directory, and run-scratch path conventions; daemon/envfile/log-filter configuration helpers.
- **Depends on candidates:** `fabro-macros-metadata`, `fabro-model`, `fabro-proc`, `fabro-static`, `fabro-types`, `fabro-util`
- **Evidence:**
  - `lib/foundation/fabro-config/Cargo.toml` — declares the centralized configuration crate and its optional `clap` integration.
  - `lib/foundation/fabro-config/src/lib.rs` — exposes layer types, builders, resolvers, parsing, storage, and runtime path facade.
  - `lib/foundation/fabro-config/src/builders.rs` — composes defaults and source layers into dense user, server, run, workflow, and model-catalog settings.
  - `lib/foundation/fabro-config/src/layers/combine.rs` and `lib/foundation/fabro-config/src/layers/*.rs` — define the layer merge contract and source-specific shapes.
  - `lib/foundation/fabro-config/src/migrations.rs` plus `lib/foundation/fabro-config/migrations/*.rs` — register and implement the settings-file migration lifecycle.
  - `lib/foundation/fabro-config/src/tests/*.rs` — exercise resolution independently for root, CLI, project, run, server, and workflow sources.

### `fabro-core` — Generic graph execution kernel

- **File count:** 13
- **Purpose:** Executes generic directed workflow graphs with handler, retry, lifecycle, cancellation, checkpoint, visit-limit, and stall-monitoring contracts.
- **Globs:** `lib/foundation/fabro-core/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-core/src/executor.rs:ExecutorBuilder`, `lib/foundation/fabro-core/src/executor.rs:Executor::run`, `lib/foundation/fabro-core/src/graph.rs:Graph`, `lib/foundation/fabro-core/src/handler.rs:NodeHandler`, `lib/foundation/fabro-core/src/lifecycle.rs:RunLifecycle`
- **Owns:** in-memory execution state; node/edge traversal loop; handler and lifecycle extension contracts; retry/visit/cancellation decisions; stall-watchdog task lifecycle.
- **Depends on candidates:** `fabro-types`, `fabro-util`
- **Evidence:**
  - `lib/foundation/fabro-core/Cargo.toml` — identifies the crate as the generic workflow execution engine without depending on the higher-level workflow component.
  - `lib/foundation/fabro-core/src/graph.rs` — defines generic graph, node, and edge contracts.
  - `lib/foundation/fabro-core/src/executor.rs:Executor::run` — owns the traversal and execution lifecycle.
  - `lib/foundation/fabro-core/src/state.rs:ExecutionState` — owns current node, outcomes, retries, visits, completed nodes, and context.
  - `lib/foundation/fabro-core/src/lifecycle.rs:RunLifecycle` and `lib/foundation/fabro-core/src/stall.rs:StallWatchdog` — expose the lifecycle hooks and owned background timeout task.
  - `lib/components/fabro-workflow/Cargo.toml` — out-of-scope consumer evidence that the product workflow component adapts this lower-level kernel.

### `fabro-db` — Shared SQLite database foundation

- **File count:** 9
- **Purpose:** Opens and migrates the shared SQLite database, manages migration rollback snapshots and private file permissions, and defines the bundled schema migration set.
- **Globs:** `lib/foundation/fabro-db/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-db/src/lib.rs:Database::connect`, `lib/foundation/fabro-db/src/lib.rs:Database::migrate`, `lib/foundation/fabro-db/src/lib.rs:Database::health_check`, `lib/foundation/fabro-db/src/lib.rs:DbPool`
- **Owns:** SQLite pool setup; WAL/synchronous/busy-timeout policy; schema migration registry; pre-migration snapshot and legacy-backup paths; database file permissions; shared tables and indexes declared in `migrations/*.sql`.
- **Depends on candidates:** `[]`
- **Evidence:**
  - `lib/foundation/fabro-db/Cargo.toml` — declares a SQLite storage foundation with SQLx migration support.
  - `lib/foundation/fabro-db/src/lib.rs:Database` — owns database connection, migration, snapshot, health-check, and pool access lifecycle.
  - `lib/foundation/fabro-db/migrations/*.sql` — define the variables, environments, secrets, MCP servers, automations, and run-projection schema compiled into this crate's migrator.
  - `lib/foundation/fabro-db/tests/sqlite.rs` — exercises migration, snapshot, permissions, and database behavior at the crate boundary.
  - `lib/components/fabro-variable/Cargo.toml`, `lib/components/fabro-environment/Cargo.toml`, `lib/components/fabro-mcp-store/Cargo.toml`, `lib/components/fabro-automation/Cargo.toml`, and `lib/components/fabro-store/Cargo.toml` — out-of-scope manifests show multiple persistence components sharing this foundation.

### `fabro-http` — Shared HTTP transport construction

- **File count:** 2
- **Purpose:** Centralizes reqwest type exposure and synchronous/asynchronous HTTP client construction with Fabro's proxy and test no-proxy policy.
- **Globs:** `lib/foundation/fabro-http/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-http/src/lib.rs:HttpClientBuilder`, `lib/foundation/fabro-http/src/lib.rs:http_client`, `lib/foundation/fabro-http/src/lib.rs:test_http_client`, `lib/foundation/fabro-http/src/lib.rs:BlockingHttpClientBuilder`
- **Owns:** approved reqwest facade; proxy-policy resolution from `FABRO_HTTP_PROXY_POLICY`; async/blocking client builders; deterministic no-proxy test clients.
- **Depends on candidates:** `fabro-static`
- **Evidence:**
  - `lib/foundation/fabro-http/Cargo.toml` — declares a shared reqwest-wrapper crate.
  - `lib/foundation/fabro-http/src/lib.rs:ProxyPolicy` and `HttpClientBuilder` — implement the shared transport-construction policy rather than domain HTTP behavior.
  - The root `Cargo.toml` exposes `fabro-http` as a workspace dependency, and app/component manifests consume it directly, establishing it as a cross-cutting transport boundary.

### `fabro-macros-metadata` — Compile-time macros and option metadata

- **File count:** 6
- **Purpose:** Supplies Fabro's derive/attribute macros and the runtime option-metadata model used by generated configuration and documentation tooling.
- **Globs:** `lib/foundation/fabro-macros/**`, `lib/foundation/fabro-options-metadata/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-macros/src/lib.rs:e2e_test`, `lib/foundation/fabro-macros/src/lib.rs:derive_combine`, `lib/foundation/fabro-macros/src/lib.rs:derive_options_metadata`, `lib/foundation/fabro-options-metadata/src/lib.rs:OptionsMetadata`, `lib/foundation/fabro-options-metadata/src/lib.rs:OptionSet`
- **Owns:** macro input parsing and expansion for E2E mode gates, configuration-layer combination, and option metadata; option visitor/tree representation; flattened lookup/display/serialization of option metadata.
- **Depends on candidates:** `[]`
- **Evidence:**
  - `lib/foundation/fabro-macros/Cargo.toml` — declares the proc-macro crate and a dev dependency on the runtime metadata crate.
  - `lib/foundation/fabro-macros/src/options_metadata.rs:derive_impl` — generates implementations against `fabro_options_metadata::OptionsMetadata`.
  - `lib/foundation/fabro-options-metadata/src/lib.rs:OptionsMetadata` and `OptionSet` — provide the runtime half of that generated contract.
  - `lib/foundation/fabro-macros/tests/options_metadata.rs` — tests the proc-macro/runtime pair together.
  - `lib/foundation/fabro-config/Cargo.toml` and `lib/foundation/fabro-dev/Cargo.toml` — out-of-scope consumer evidence for configuration derives and generated option documentation.

### `fabro-model` — LLM model and provider catalog

- **File count:** 28
- **Purpose:** Defines provider/model identity, capabilities, billing metadata, embedded catalog data, override merging, and model selection.
- **Globs:** `lib/foundation/fabro-model/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-model/src/catalog.rs:Catalog::builtin`, `lib/foundation/fabro-model/src/catalog.rs:Catalog::from_builtin_with_overrides`, `lib/foundation/fabro-model/src/catalog.rs:Catalog::select`, `lib/foundation/fabro-model/src/bootstrap_catalog.rs:catalog`, `lib/foundation/fabro-model/src/lib.rs`
- **Owns:** canonical provider/model IDs; embedded provider TOML catalog; catalog indexes and selection state; provider auth declarations; model capabilities, controls, codecs/adapters, reasoning levels, pricing, and billing calculations.
- **Depends on candidates:** `fabro-static`
- **Evidence:**
  - `lib/foundation/fabro-model/Cargo.toml` — names provider identity, model metadata, and resolution as the crate responsibility and embeds catalog resources.
  - `lib/foundation/fabro-model/src/catalog.rs:BuiltinCatalogToml` and `Catalog` — load embedded provider files into indexed selection state.
  - `lib/foundation/fabro-model/src/catalog/providers/*.toml` — are the tracked built-in provider/model catalog sources.
  - `lib/foundation/fabro-model/src/ids.rs` — defines open-ended provider and model identity shared by auth, config, API, and LLM consumers.
  - `lib/foundation/fabro-model/src/billing.rs` and `src/types.rs` — define the catalog's billing and public model metadata surfaces.

### `fabro-oauth` — OAuth PKCE and loopback callback flow

- **File count:** 3
- **Purpose:** Implements generic OAuth 2.0 PKCE authorization, loopback callback serving, browser launch, code exchange, and token refresh.
- **Globs:** `lib/foundation/fabro-oauth/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-oauth/src/lib.rs:run_browser_flow`, `lib/foundation/fabro-oauth/src/lib.rs:start_callback_server_with_errors`, `lib/foundation/fabro-oauth/src/lib.rs:exchange_code`, `lib/foundation/fabro-oauth/src/lib.rs:refresh_token`, `lib/foundation/fabro-oauth/examples/login.rs:main`
- **Owns:** PKCE verifier/challenge and state generation; authorization URL encoding; ephemeral callback listener/task and shutdown handle; callback validation/result delivery; token response decoding and refresh requests.
- **Depends on candidates:** `fabro-http`, `fabro-redact`, `fabro-static`, `fabro-util`
- **Evidence:**
  - `lib/foundation/fabro-oauth/Cargo.toml` — declares a generic OAuth 2.0 PKCE token-acquisition crate.
  - `lib/foundation/fabro-oauth/src/lib.rs:CallbackHandle` — owns the ephemeral callback server port and shutdown channel.
  - `lib/foundation/fabro-oauth/src/lib.rs:run_browser_flow` — composes PKCE, callback server, browser, and token exchange into the top-level flow.
  - `lib/foundation/fabro-oauth/examples/login.rs` — demonstrates the crate as a standalone protocol flow.
  - `lib/foundation/fabro-auth/Cargo.toml` and `lib/apps/fabro-cli/Cargo.toml` — out-of-scope manifests establish both auth-library and direct CLI consumers.

### `fabro-proc` — OS process primitives

- **File count:** 8
- **Purpose:** Wraps platform process primitives for signals, process groups, advisory file locking, pre-exec hooks, process liveness, and process-title rewriting.
- **Globs:** `lib/foundation/fabro-proc/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-proc/src/lib.rs`, `lib/foundation/fabro-proc/src/signal.rs:process_running`, `lib/foundation/fabro-proc/src/signal.rs:sigterm_process_group`, `lib/foundation/fabro-proc/src/pre_exec.rs:pre_exec_setsid`, `lib/foundation/fabro-proc/src/title.rs:init`
- **Owns:** Unix signal/process-group calls; cross-platform liveness semantics; advisory locks; child pre-exec configuration; captured argv memory and process title state.
- **Depends on candidates:** `[]`
- **Evidence:**
  - `lib/foundation/fabro-proc/Cargo.toml` — describes safe wrappers for process-management primitives and compiles the C argv capture helper.
  - `lib/foundation/fabro-proc/src/lib.rs` — is a platform-gated facade over flock, pre-exec, signal, and title modules.
  - `lib/foundation/fabro-proc/c/capture_argv.c` and `lib/foundation/fabro-proc/build.rs` — establish the FFI/build boundary for title rewriting.
  - `lib/apps/fabro-server/Cargo.toml`, `lib/apps/fabro-cli/Cargo.toml`, and `lib/components/fabro-sandbox/Cargo.toml` — out-of-scope manifests show independent process-lifecycle consumers.

### `fabro-redact` — Secret and credential redaction

- **File count:** 8
- **Purpose:** Detects and redacts credential-like content in strings, URLs, JSON, and JSONL using embedded Gitleaks rules and entropy scanning.
- **Globs:** `lib/foundation/fabro-redact/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-redact/src/lib.rs:redact_string`, `lib/foundation/fabro-redact/src/lib.rs:redacted_url_for_log`, `lib/foundation/fabro-redact/src/jsonl.rs:redact_jsonl_line`, `lib/foundation/fabro-redact/src/safe_url.rs:DisplaySafeUrl`
- **Owns:** Gitleaks rule source and generated rule table; lazy rule engine; entropy thresholds; overlap merging and redaction marker; JSON field/object skip policy; safe URL display semantics.
- **Depends on candidates:** `[]`
- **Evidence:**
  - `lib/foundation/fabro-redact/Cargo.toml` — declares the secret/credential redaction boundary.
  - `lib/foundation/fabro-redact/build.rs:main` and `lib/foundation/fabro-redact/data/gitleaks.toml` — compile the tracked rule source into an untracked `OUT_DIR` table.
  - `lib/foundation/fabro-redact/src/lib.rs:redact_string` — composes entropy and Gitleaks detection into one public redaction surface.
  - `lib/foundation/fabro-redact/src/safe_url.rs:DisplaySafeUrl` — owns the raw-versus-display URL credential boundary.
  - `lib/foundation/fabro-redact/src/jsonl.rs` — applies the scanner to structured event/log content.

### `fabro-static` — Shared static conventions

- **File count:** 4
- **Purpose:** Defines dependency-light canonical environment-variable names and the registry that classifies bootstrap and optional-vault secrets.
- **Globs:** `lib/foundation/fabro-static/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-static/src/env_vars.rs:EnvVars`, `lib/foundation/fabro-static/src/secret_registry.rs:is_bootstrap_secret`, `lib/foundation/fabro-static/src/secret_registry.rs:optional_vault_secrets`
- **Owns:** canonical process environment string constants; bootstrap-secret set; optional vault-secret set and classification.
- **Depends on candidates:** `[]`
- **Evidence:**
  - `lib/foundation/fabro-static/Cargo.toml` — declares a no-dependency static string registry.
  - `lib/foundation/fabro-static/src/env_vars.rs:EnvVars` — centralizes environment names consumed across applications, components, and foundation crates.
  - `lib/foundation/fabro-static/src/secret_registry.rs` — defines secret scope independently of vault/auth implementations.
  - The root `Cargo.toml` exposes the crate as a workspace dependency, and `fabro-http`, `fabro-model`, `fabro-util`, auth, telemetry, server, CLI, sandbox, Slack, and GitHub manifests consume it.

### `fabro-telemetry` — Analytics and crash telemetry

- **File count:** 11
- **Purpose:** Initializes analytics/crash reporting, builds anonymous telemetry context, buffers events, and hands delivery to blocking or detached senders across CLI and server lifecycles.
- **Globs:** `lib/foundation/fabro-telemetry/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-telemetry/src/lib.rs:init_cli`, `lib/foundation/fabro-telemetry/src/lib.rs:init_server`, `lib/foundation/fabro-telemetry/src/lib.rs:track`, `lib/foundation/fabro-telemetry/src/lib.rs:shutdown`, `lib/foundation/fabro-telemetry/src/panic.rs:install_panic_hook`
- **Owns:** process-global telemetry state; anonymous CLI/server identifiers; background buffer thread and shutdown join; analytics event shape/context; command sanitization; Segment delivery and detached subprocess handoff; Sentry panic capture.
- **Depends on candidates:** `fabro-http`, `fabro-static`, `fabro-util`
- **Evidence:**
  - `lib/foundation/fabro-telemetry/Cargo.toml` — declares analytics and crash reporting with HTTP, Sentry, Git, and process-spawn dependencies.
  - `lib/foundation/fabro-telemetry/src/lib.rs:Global` — owns the process-global sender, identity, context, level, and background thread.
  - `lib/foundation/fabro-telemetry/src/buffer.rs` and `src/sender.rs` — define buffered delivery and upload boundaries.
  - `lib/foundation/fabro-telemetry/src/spawn.rs` — owns the detached subprocess handoff used at process exit.
  - `lib/foundation/fabro-telemetry/src/panic.rs` — owns panic-hook event construction and capture.

### `fabro-template` — Template rendering and dependency discovery

- **File count:** 4
- **Purpose:** Renders MiniJinja templates with Fabro context, source-aware diagnostics, rooted include stores, caching/recording wrappers, and static dependency discovery.
- **Globs:** `lib/foundation/fabro-template/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-template/src/lib.rs:render_source`, `lib/foundation/fabro-template/src/lib.rs:render_named`, `lib/foundation/fabro-template/src/lib.rs:TemplateContext`, `lib/foundation/fabro-template/src/store.rs:TemplateStore`, `lib/foundation/fabro-template/src/dependency.rs:discover_static_dependency_closure`
- **Owns:** template context/value exposure; strict and lenient render modes; source-location error diagnostics; include/import path safety and rooted resolution; filesystem/bundle/cache/recording stores; static dependency closure.
- **Depends on candidates:** `fabro-types`, `fabro-util`
- **Evidence:**
  - `lib/foundation/fabro-template/Cargo.toml` — declares the shared MiniJinja rendering boundary.
  - `lib/foundation/fabro-template/src/lib.rs:TemplateContext` and `TemplateError` — define the public render input and source-aware failure surface.
  - `lib/foundation/fabro-template/src/store.rs:TemplateStore` and `TemplateIncludeResolver` — define source loading and root containment.
  - `lib/foundation/fabro-template/src/dependency.rs` — owns include/import extraction and dependency-closure discovery.
  - `lib/components/fabro-agent/Cargo.toml`, `lib/components/fabro-workflow/Cargo.toml`, and `lib/components/fabro-manifest/Cargo.toml` — out-of-scope manifests show agent, workflow, and manifest consumers.

### `fabro-test` — Shared integration-test infrastructure

- **File count:** 3
- **Purpose:** Provides isolated Fabro CLI/server integration-test contexts, twin/live mode control, process and environment harnessing, snapshot normalization, and HTTP assertion helpers.
- **Globs:** `lib/foundation/fabro-test/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-test/src/lib.rs:TestContext`, `lib/foundation/fabro-test/src/lib.rs:TestMode`, `lib/foundation/fabro-test/src/lib.rs:apply_test_isolation`, `lib/foundation/fabro-test/src/lib.rs:test_http_client`, `lib/foundation/fabro-test/src/http_assert.rs:expect_reqwest_status`
- **Owns:** per-test temporary home/storage/session/server lifecycle; E2E mode and live-secret gating; subprocess environment isolation; test daemon coordination; snapshot filters; twin service setup; Axum/reqwest response assertion diagnostics.
- **Depends on candidates:** `fabro-config`, `fabro-http`, `fabro-proc`, `fabro-static`, `fabro-types`, `fabro-util`
- **External dependency edges:** depends on the `fabro-install`, `twin-openai`, and `twin-github` test components.
- **Evidence:**
  - `lib/foundation/fabro-test/Cargo.toml` — identifies the crate as integration-test utilities and declares test-only component/twin dependencies.
  - `lib/foundation/fabro-test/src/lib.rs:TestContext` — owns isolated test paths, session state, Fabro binary invocation, filters, and managed server/storage state.
  - `lib/foundation/fabro-test/src/lib.rs:TestMode` and `apply_test_isolation` — define the twin/live/strict and environment-isolation contracts used by the `e2e_test` macro.
  - `lib/foundation/fabro-test/src/http_assert.rs` — centralizes response consumption and diagnostic assertion behavior for both server and network tests.
  - Workspace app/component manifests list `fabro-test` only in dev-dependency/test contexts.

### `fabro-types` — Shared product contracts and state records

- **File count:** 78
- **Purpose:** Defines the serializable identifiers, settings records, run/session/event/state projections, and other shared product vocabulary exchanged across Fabro crates and API boundaries.
- **Globs:** `lib/foundation/fabro-types/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-types/src/lib.rs`, `lib/foundation/fabro-types/src/run_event/mod.rs:RunEvent`, `lib/foundation/fabro-types/src/run.rs:RunSpec`, `lib/foundation/fabro-types/src/settings/mod.rs`, `lib/foundation/fabro-types/src/outcome.rs:Outcome`, `lib/foundation/fabro-types/src/status.rs:RunStatus`
- **Owns:** canonical serde shapes and IDs for runs, stages, sessions, events, transcripts, outcomes, status, projections, sandboxes, MCP servers, variables, secrets, integrations, billing, repositories, pull requests, and dense/resolved settings; feature-gated shared test fixtures.
- **Depends on candidates:** `fabro-model`, `fabro-util`
- **Evidence:**
  - `lib/foundation/fabro-types/Cargo.toml` — describes shared record structs/enums and exposes only `clap` and `test-support` feature boundaries.
  - `lib/foundation/fabro-types/src/lib.rs` — is a single crate facade that re-exports the canonical shared product vocabulary across its module families.
  - `lib/foundation/fabro-types/src/run_event/mod.rs` and `src/run_event/*.rs` — define the event contract consumed by workflow, storage, server, client, and API code.
  - `lib/foundation/fabro-types/src/settings/mod.rs` and `src/settings/*.rs` — define the resolved settings contract consumed by `fabro-config` and runtime components.
  - `lib/foundation/fabro-types/tests/*.rs` — verify serde and method contracts for run specs, events, failures, sandbox models, inventory, and stage handlers.
  - `lib/foundation/fabro-api/build.rs` and its round-trip tests — boundary evidence that API generation intentionally reuses these types rather than generating parallel DTOs.

### `fabro-util` — Cross-cutting runtime and CLI utilities

- **File count:** 24
- **Purpose:** Provides shared environment, filesystem, shell, terminal, logging, token, error-rendering, time, backoff, warning, and workspace-glob primitives used across Fabro crates.
- **Globs:** `lib/foundation/fabro-util/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-util/src/lib.rs`, `lib/foundation/fabro-util/src/shell.rs:shell_quote`, `lib/foundation/fabro-util/src/printer.rs:Printer`, `lib/foundation/fabro-util/src/home.rs:Home`, `lib/foundation/fabro-util/src/run_log.rs:BufferedFileAppender`, `lib/foundation/fabro-util/src/workspace_glob.rs:WorkspaceGlobSet`
- **Owns:** low-level helper contracts and any helper-owned state, including the global warning set, buffered run-log guard, environment abstraction, home directory, dev/session token formats, terminal styles/printers, backoff policy, error-chain rendering, and workspace glob compilation.
- **Depends on candidates:** `fabro-static`
- **Evidence:**
  - `lib/foundation/fabro-util/Cargo.toml` — identifies shared terminal/path/environment/runtime helpers and has no product-component dependencies.
  - `lib/foundation/fabro-util/src/lib.rs` — exposes the helper modules directly as the public crate facade.
  - `lib/foundation/fabro-util/src/shell.rs` — owns shell quoting/joining used by workflow and developer tooling.
  - `lib/foundation/fabro-util/src/run_log.rs` and `src/warnings.rs` — contain the component's stateful log-guard and warning-registry lifecycles.
  - `lib/foundation/fabro-util/tests/dev_token.rs` and `tests/error_chain.rs` — test stable token-file and error-rendering contracts.

### `fabro-vault` — Secret vault and SQLite secret store

- **File count:** 4
- **Purpose:** Validates and stores workflow-visible secrets in file/in-memory vaults or the shared SQLite database, including revision-aware updates and one-time legacy import.
- **Globs:** `lib/foundation/fabro-vault/**`
- **Exclude globs:** `[]`
- **Entry points:** `lib/foundation/fabro-vault/src/lib.rs:Vault::load`, `lib/foundation/fabro-vault/src/store.rs:SecretStore::open`, `lib/foundation/fabro-vault/src/store.rs:SecretStore::apply`, `lib/foundation/fabro-vault/src/store.rs:SecretStore::snapshot`, `lib/foundation/fabro-vault/src/store.rs:import_legacy_json_once`
- **Owns:** secret-name/type validation; redacted secret entry representation; atomic JSON vault persistence; SQL secret CRUD; secret revisions and compare-and-swap refresh updates; snapshots; legacy JSON import and backup lifecycle.
- **Depends on candidates:** `fabro-db`, `fabro-static`, `fabro-types`
- **Evidence:**
  - `lib/foundation/fabro-vault/Cargo.toml` — declares the workflow-visible secret vault and its database/type dependencies.
  - `lib/foundation/fabro-vault/src/lib.rs:Vault` — owns file-backed or detached in-memory entries and atomic write behavior.
  - `lib/foundation/fabro-vault/src/store.rs:SecretStore` — owns the SQLite-backed secret operations and snapshots.
  - `lib/foundation/fabro-vault/src/store.rs:SecretStore::replace_if_revision` — exposes the revision boundary used for concurrent OAuth refresh write-back.
  - `lib/foundation/fabro-vault/tests/store.rs` — exercises store CRUD, validation, snapshots, and legacy import at the public boundary.

## Coverage

| Proposed component | Tracked files |
| --- | ---: |
| `fabro-build-tooling` | 23 |
| `fabro-api` | 60 |
| `fabro-auth` | 16 |
| `fabro-client` | 9 |
| `fabro-config` | 52 |
| `fabro-core` | 13 |
| `fabro-db` | 9 |
| `fabro-http` | 2 |
| `fabro-macros-metadata` | 6 |
| `fabro-model` | 28 |
| `fabro-oauth` | 3 |
| `fabro-proc` | 8 |
| `fabro-redact` | 8 |
| `fabro-static` | 4 |
| `fabro-telemetry` | 11 |
| `fabro-template` | 4 |
| `fabro-test` | 3 |
| `fabro-types` | 78 |
| `fabro-util` | 24 |
| `fabro-vault` | 4 |
| **Total assigned** | **365** |

- **Relevant tracked files:** 365
- **Assigned:** 365
- **Excluded:** 0
- **Unmapped:** 0
- **Overlap:** 0; every proposed glob is a whole crate directory, and the two grouped components use disjoint crate directories.
- **Tracked exclusions:** none. Build outputs such as `OUT_DIR/codegen.rs` and `OUT_DIR/rules_generated.rs` are generated but are not tracked and therefore are not part of the 365-file inventory. No vendored or generated tracked source was found in scope.
- **Unmapped files:** `[]`

## External boundary evidence consulted

These files are outside the scoped inventory and are neither assigned nor counted as unmapped:

- `Cargo.toml` — workspace membership, workspace dependencies, and lint policy.
- `docs/public/api-reference/fabro-api.yaml` — source contract read by `fabro-api/build.rs`.
- `lib/apps/fabro-cli/Cargo.toml`, `lib/apps/fabro-server/Cargo.toml`, `lib/apps/fabro-mcp-server/Cargo.toml` — application-level reverse dependency evidence.
- Relevant `lib/components/*/Cargo.toml` manifests — reverse dependency evidence for execution, storage, schema, types, templates, auth, HTTP, process, test, and API foundations.

## Genuine boundary questions

1. Should `build-support` remain grouped with `fabro-dev` in the final map, or should its compile-time consumer boundary make it a separate two-file component despite the resulting assessment granularity?
2. Should `fabro-macros` and `fabro-options-metadata` remain one component? Their `OptionsMetadata` compiler/runtime contract supports grouping, while `Combine` and `e2e_test` also connect the proc-macro crate to configuration and test infrastructure.
3. Should the SQL migration files under `fabro-db/migrations/**` remain with the shared database foundation, or should reconciliation assign table-specific migrations to the variable, environment, MCP-store, automation, and run-store components that own the corresponding query behavior? The current proposal follows compile-time ownership by `fabro-db`.
4. Is `fabro-types` an acceptable single assessment component, or does the final map need stable subcomponents for settings, run/event/projection, and other contract families? This revision exposes one manifest and one broad crate facade, so this scout found no non-overlapping public boundary for such a split.
