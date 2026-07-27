# Rust Applications Cartography Scout

Assessed revision: `2bcf94fed8a9b429f18d9196fa824711d6f4cb0a` (`2bcf94fed`)

Scope: tracked files under `lib/apps/**`. Root `Cargo.toml`, `.gitignore`,
`AGENTS.md`, and `CONTRIBUTING.md` were read only as workspace, exclusion, and
repository-instruction evidence; they are not included in the scope counts.
`CLAUDE.md` resolves to the same repository guidance as `AGENTS.md`.

The primary proposal is one component per Cargo application package. These
boundaries are established by independent package manifests, binary or library
entry points, public interfaces, package-owned lifecycle/state, package test
suites, and explicit Cargo dependency edges. The CLI and server have broad
module trees, but their entry points and tests converge on one executable or
one shared server state/router respectively.

## Inventory and coverage

The inventory was computed with:

```text
git ls-tree -r --name-only 2bcf94fed8a9b429f18d9196fa824711d6f4cb0a -- lib/apps
```

| Scope | Tracked | Assigned | Excluded | Unmapped |
| --- | ---: | ---: | ---: | ---: |
| `lib/apps/fabro-cli/**` | 241 | 241 | 0 | 0 |
| `lib/apps/fabro-mcp-server/**` | 5 | 5 | 0 | 0 |
| `lib/apps/fabro-server/**` | 112 | 112 | 0 | 0 |
| `lib/apps/fabro-spa/**` | 3 | 2 | 1 | 0 |
| **Total** | **361** | **360** | **1** | **0** |

The one excluded tracked file is
`lib/apps/fabro-spa/assets/.gitkeep`. `AGENTS.md` states that embedded SPA
assets are refreshed build output and are gitignored except for `.gitkeep`;
`.gitignore` corroborates this with `lib/apps/fabro-spa/assets/*` and the
explicit `.gitkeep` exception. The placeholder is therefore excluded as
evidence of a generated build-output directory. No generated code, vendored
code, dependency trees, or other build output is tracked elsewhere in this
scope.

## Proposed components

### `fabro-cli` — Fabro CLI Application

- **Purpose:** Provides the `fabro` command-line application, including command parsing and dispatch, terminal presentation, server/client bootstrap, and the hidden local run-worker process entry.
- **Assigned file count:** 241
- **Globs:**
  - `lib/apps/fabro-cli/Cargo.toml`
  - `lib/apps/fabro-cli/build.rs`
  - `lib/apps/fabro-cli/src/**`
  - `lib/apps/fabro-cli/tests/**`
- **Exclude globs:** none
- **Entry points:**
  - `lib/apps/fabro-cli/src/main.rs:main`
  - `lib/apps/fabro-cli/src/main.rs:main_inner`
  - `lib/apps/fabro-cli/src/args.rs:Cli`
  - `lib/apps/fabro-cli/src/args.rs:Commands`
  - `lib/apps/fabro-cli/src/commands/run/mod.rs:dispatch`
- **Owns:**
  - The `fabro` process lifecycle, exit classification, telemetry bootstrap, and logging bootstrap.
  - CLI argument and subcommand contracts plus human-readable and JSON output behavior.
  - Per-command resolved settings, lazy API client/credential/catalog state in `CommandContext`.
  - Local server discovery/startup and authenticated server connections.
  - The hidden `__run-worker` subprocess entry and its terminal run-progress presentation.
- **Candidate `depends_on` IDs within this scout:** `fabro-mcp-server`, `fabro-server`.
- **Manifest-backed cross-scope dependency candidates:** `fabro-agent`, `fabro-api`, `fabro-auth`, `fabro-checkpoint`, `fabro-client`, `fabro-config`, `fabro-dump`, `fabro-environment`, `fabro-github`, `fabro-graphviz`, `fabro-hooks`, `fabro-http`, `fabro-install`, `fabro-interview`, `fabro-llm`, `fabro-manifest`, `fabro-mcp`, `fabro-model`, `fabro-oauth`, `fabro-proc`, `fabro-redact`, `fabro-sandbox`, `fabro-static`, `fabro-store`, `fabro-telemetry`, `fabro-template`, `fabro-tool`, `fabro-types`, `fabro-util`, `fabro-validate`, `fabro-vault`, `fabro-workflow`. `fabro-build-support` is also a build-time edge.
- **Evidence:**
  - `lib/apps/fabro-cli/Cargo.toml:[[bin]]` — declares package `fabro-cli` as the `fabro` binary with `src/main.rs` as its entry point and lists direct workspace dependencies, including `fabro-mcp-server` and `fabro-server`.
  - `Cargo.toml:[workspace]` — includes `lib/apps/*` as members and selects `lib/apps/fabro-cli` as the default workspace member.
  - `lib/apps/fabro-cli/src/main.rs:main_inner` — creates the shared command context and dispatches every `Commands` variant, including the server and run-worker paths.
  - `lib/apps/fabro-cli/src/args.rs:Commands` — defines the complete top-level CLI command surface; `RunCommands` includes the hidden `__run-worker` entry.
  - `lib/apps/fabro-cli/src/command_context.rs:CommandContext` — owns the per-invocation settings, output mode, storage path, lazy server client, credential source, and model catalog shared by commands.
  - `lib/apps/fabro-cli/src/server_client.rs:connect_server_with_settings` — resolves local or remote targets and constructs the authenticated control-plane client used by command implementations.
  - `lib/apps/fabro-cli/tests/it/main.rs` — assembles command, scenario, support, and end-to-end workflow tests around the same binary application boundary.

### `fabro-mcp-server` — Fabro MCP Stdio Server

- **Purpose:** Exposes Fabro run operations as an MCP stdio tool server and supplies MCP-client configuration generation and installation helpers used by the CLI.
- **Assigned file count:** 5
- **Globs:**
  - `lib/apps/fabro-mcp-server/Cargo.toml`
  - `lib/apps/fabro-mcp-server/src/**`
- **Exclude globs:** none
- **Entry points:**
  - `lib/apps/fabro-mcp-server/src/lib.rs:start`
  - `lib/apps/fabro-mcp-server/src/server.rs:start`
  - `lib/apps/fabro-mcp-server/src/lib.rs:FabroMcpServerSettings`
  - `lib/apps/fabro-mcp-server/src/config.rs:config_json`
  - `lib/apps/fabro-mcp-server/src/config.rs:init_agent`
- **Owns:**
  - The MCP stdio service lifecycle and registered Fabro tool router.
  - Lazy construction of the Fabro client-backed tool backend.
  - Translation from MCP run-create inputs to Fabro API run manifests.
  - MCP client configuration rendering and updates to supported agent config files.
- **Candidate `depends_on` IDs within this scout:** `fabro-server`.
- **Manifest-backed cross-scope dependency candidates:** `fabro-api`, `fabro-client`, `fabro-config`, `fabro-manifest`, `fabro-model`, `fabro-tool`, `fabro-types`, `fabro-util`.
- **Evidence:**
  - `lib/apps/fabro-mcp-server/Cargo.toml:[package]` — declares a distinct library package described as the Fabro MCP stdio server and lists a direct `fabro-server` dependency.
  - `lib/apps/fabro-mcp-server/src/lib.rs:FabroMcpServerSettings` — defines the public construction boundary, client factory, config path, and working directory used to start the service.
  - `lib/apps/fabro-mcp-server/src/server.rs:start` — owns the `rmcp` stdio service lifecycle; `FabroMcpServer` owns the tool router and lazy backend.
  - `lib/apps/fabro-mcp-server/src/manifest_builder.rs:McpRunManifestBuilder` — adapts MCP tool creation requests through `fabro_server::run_tool_manifest`.
  - `lib/apps/fabro-cli/src/commands/mcp/mod.rs:dispatch` — the separate CLI package consumes this library solely through its public start/config/init interfaces.

### `fabro-server` — Fabro HTTP Server

- **Purpose:** Hosts Fabro's HTTP control plane and web surface while coordinating persisted run state, schedulers, worker processes, sessions, authentication, integrations, and startup/shutdown.
- **Assigned file count:** 112
- **Globs:**
  - `lib/apps/fabro-server/Cargo.toml`
  - `lib/apps/fabro-server/build.rs`
  - `lib/apps/fabro-server/migrations/**`
  - `lib/apps/fabro-server/src/**`
  - `lib/apps/fabro-server/tests/**`
- **Exclude globs:** none
- **Entry points:**
  - `lib/apps/fabro-server/src/serve.rs:serve_command`
  - `lib/apps/fabro-server/src/server.rs:AppState`
  - `lib/apps/fabro-server/src/server.rs:build_router`
  - `lib/apps/fabro-server/src/server.rs:build_router_with_options`
  - `lib/apps/fabro-server/src/server.rs:spawn_scheduler`
  - `lib/apps/fabro-server/src/lib.rs`
- **Owns:**
  - Listener binding, resolved startup configuration, migrations, web enablement, and graceful shutdown.
  - Shared `AppState`: managed runs, persistent stores, session runtimes, artifact storage, resource sampling, settings/catalog state, and integration services.
  - API and web routing, authentication/principal middleware, static-file delivery, security headers, and OpenAPI conformance at the router boundary.
  - Run and automation scheduling, worker launch/control/token state, cancellation escalation, and global event broadcast.
  - Server-side install, diagnostics, GitHub webhook, Slack, environment, secret, variable, MCP-server, and sandbox coordination exposed through HTTP handlers.
- **Candidate `depends_on` IDs within this scout:** `fabro-spa`.
- **Manifest-backed cross-scope dependency candidates:** `fabro-agent`, `fabro-api`, `fabro-auth`, `fabro-automation`, `fabro-client`, `fabro-config`, `fabro-db`, `fabro-environment`, `fabro-github`, `fabro-graphviz`, `fabro-hooks`, `fabro-http`, `fabro-install`, `fabro-interview`, `fabro-llm`, `fabro-manifest`, `fabro-mcp-store`, `fabro-model`, `fabro-proc`, `fabro-redact`, `fabro-sandbox`, `fabro-slack`, `fabro-static`, `fabro-store`, `fabro-tool`, `fabro-types`, `fabro-util`, `fabro-validate`, `fabro-variable`, `fabro-vault`, `fabro-workflow`. `fabro-build-support` is also a build-time edge.
- **Evidence:**
  - `lib/apps/fabro-server/Cargo.toml:[package]` — declares a distinct HTTP-server library package, an integration-test target gated by `test-support`, and a direct `fabro-spa` dependency.
  - `lib/apps/fabro-server/src/lib.rs` — exposes the server's supported module/API surface and gates `test_support` behind tests or the explicit feature.
  - `lib/apps/fabro-server/src/serve.rs:serve_command` — resolves settings and secrets, runs database and compatibility migrations, builds stores/state/router, binds listeners, starts background services, and coordinates shutdown.
  - `lib/apps/fabro-server/src/server.rs:AppState` — centralizes the service's run registry, stores, session and worker runtime state, schedulers, event channel, settings, credentials, integrations, and shutdown token.
  - `lib/apps/fabro-server/src/server.rs:build_router_with_options` — composes real/demo APIs, auth/web routes, middleware, static assets, and the health surface around the shared state.
  - `lib/apps/fabro-server/src/server/handler/mod.rs:real_routes` — registers the HTTP resource handlers that consume `AppState`.
  - `lib/apps/fabro-server/tests/it/main.rs` — assembles API, conformance, pagination, and lifecycle scenario tests around the same library/router boundary.

### `fabro-spa` — Embedded SPA Assets

- **Purpose:** Provides the compile-time embedded production SPA asset lookup API and precomputed content hashes consumed by the HTTP server.
- **Assigned file count:** 2
- **Globs:**
  - `lib/apps/fabro-spa/Cargo.toml`
  - `lib/apps/fabro-spa/src/**`
  - `lib/apps/fabro-spa/assets/**`
- **Exclude globs:**
  - `lib/apps/fabro-spa/assets/**`
- **Entry points:**
  - `lib/apps/fabro-spa/src/lib.rs:get`
  - `lib/apps/fabro-spa/src/lib.rs:AssetBytes`
- **Owns:**
  - Compile-time embedding of production SPA files from `assets/`.
  - Asset byte ownership and the SHA-256 metadata returned to server static-file handling.
  - The invariant that source maps are not embedded.
- **Candidate `depends_on` IDs within this scout:** none
- **Manifest-backed cross-scope dependency candidates:** none
- **Evidence:**
  - `lib/apps/fabro-spa/Cargo.toml:[package]` — declares a distinct library package for embedded production SPA assets and depends only on `rust-embed`.
  - `lib/apps/fabro-spa/src/lib.rs:EmbeddedAssets` — defines the compile-time asset folder and source-map exclusions.
  - `lib/apps/fabro-spa/src/lib.rs:get` — is the package's public asset lookup interface and returns bytes with their precomputed SHA-256 value.
  - `lib/apps/fabro-server/src/static_files.rs` — consumes `fabro_spa::get` and `fabro_spa::AssetBytes`, establishing the direction `fabro-server` → `fabro-spa`.
  - `AGENTS.md` and `.gitignore` — identify `assets/` contents as refreshed, ignored build output while preserving only `.gitkeep`.

## Dependency reconciliation notes

The in-scope application dependency edges are exact production Cargo edges:

```text
fabro-cli ───────────────→ fabro-server ───────────────→ fabro-spa
    └──→ fabro-mcp-server ───→ fabro-server
```

The cross-scope dependency labels above use Cargo package names as provisional
component IDs. If another scout groups multiple packages into one component,
the parent map should translate those package edges to the reconciled
component ID. Build-time and dev-only edges should be handled consistently
across the final map; the primary candidate lists above include production
and build-time edges but do not add dev-only test-support dependencies.

Dev-only workspace edges that may matter during reconciliation are:

- `fabro-cli` tests additionally use `fabro-acp`, `fabro-macros`,
  `fabro-server` with `test-support`, `fabro-types` with `test-support`, and
  `fabro-workflow` with `test-support`.
- `fabro-server` tests additionally use `fabro-macros`, `fabro-sandbox` with
  `test-support`, and `fabro-types` with `test-support`.

## Exclusions and unmapped files

- **Excluded:** `lib/apps/fabro-spa/assets/.gitkeep` — placeholder retained in
  an otherwise ignored generated-asset directory.
- **Unmapped:** none.

## Open boundary questions

1. Should `fabro-server` remain one service component, as proposed, or should
   the final repository map expose separate server transport/auth and
   run/worker-coordination components? `serve_command`, `AppState`, and the
   integration suite currently join those lifecycles, while the public auth
   modules, handler tree, and worker-control modules offer possible
   sub-boundaries.
2. Should the hidden `fabro __run-worker` path remain part of `fabro-cli`, as
   proposed, or be represented as a run-worker component? It has a distinct
   process lifecycle and is launched by `fabro-server`, but it shares the CLI
   binary, manifest, dispatch, command context, and integration-test suite.
3. Should MCP client configuration/init behavior and the MCP stdio tool
   service remain one `fabro-mcp-server` component, as proposed? They are
   separate public operations but share one five-file package and one CLI
   namespace.
4. Should `fabro-spa` remain a separate component, as proposed, or be folded
   into `fabro-server` because all generated payloads are excluded and the
   remaining package has two assigned files? Its separate Cargo package and
   public asset/hash interface establish a dependency boundary, while its only
   production consumer in this scope is the server.
