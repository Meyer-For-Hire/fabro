# Chisel Codebase Map

Cartography v1 · revision `2bcf94fed8a9b429f18d9196fa824711d6f4cb0a` · 2026-07-27T14:07:02Z
Assigned 2256 files · excluded 833 · unmapped 15 · instructions: AGENTS.md, CLAUDE.md, CONTRIBUTING.md

Fabro is a Cargo workspace whose CLI and HTTP server compose shared workflow, agent, model, sandbox, persistence, integration, and foundation crates. A Bun workspace contains the React web application, Astro marketing site, Remotion composition, and OpenAPI-derived TypeScript client tooling; the OpenAPI document is the shared HTTP contract. Public and internal documentation, protocol twins, fixture corpora, evaluation tooling, build/release/deployment automation, and repository-local agent workflows form separate support boundaries around the product runtime.

## Components

### `fabro-cli` — Fabro CLI Application

- **Purpose:** Provides the fabro command-line process, command dispatch, terminal presentation, server bootstrap, and hidden run-worker entry.
- **Paths:** `lib/apps/fabro-cli/**`
- **Entry points:** `lib/apps/fabro-cli/src/main.rs:main`, `lib/apps/fabro-cli/src/args.rs:Commands`
- **Owns:** CLI process and command lifecycle, output contracts, command context, local server discovery, and the run-worker subprocess entry
- **Depends on:** `fabro-acp`, `fabro-agent`, `fabro-api`, `fabro-auth`, `fabro-build-support`, `fabro-checkpoint`, `fabro-client`, `fabro-config`, `fabro-dump`, `fabro-environment`, `fabro-github`, `fabro-graphviz`, `fabro-hooks`, `fabro-http`, `fabro-install`, `fabro-interview`, `fabro-llm`, `fabro-manifest`, `fabro-mcp`, `fabro-mcp-server`, `fabro-model`, `fabro-oauth`, `fabro-proc`, `fabro-redact`, `fabro-sandbox`, `fabro-server`, `fabro-static`, `fabro-store`, `fabro-telemetry`, `fabro-template`, `fabro-tool`, `fabro-types`, `fabro-util`, `fabro-validate`, `fabro-vault`, `fabro-workflow`, `workflow-test-corpus`
- **Evidence:** lib/apps/fabro-cli/Cargo.toml — declares the fabro binary and its direct workspace dependencies; lib/apps/fabro-cli/src/main.rs:main_inner — constructs shared command state and dispatches the complete command surface

### `fabro-mcp-server` — Fabro MCP Stdio Server

- **Purpose:** Exposes Fabro run operations as an MCP stdio tool service and generates supported MCP client configuration.
- **Paths:** `lib/apps/fabro-mcp-server/**`
- **Entry points:** `lib/apps/fabro-mcp-server/src/lib.rs:start`, `lib/apps/fabro-mcp-server/src/config.rs:init_agent`
- **Owns:** MCP stdio service lifecycle, tool router, lazy Fabro client backend, and MCP client configuration updates
- **Depends on:** `fabro-api`, `fabro-client`, `fabro-config`, `fabro-manifest`, `fabro-model`, `fabro-server`, `fabro-tool`, `fabro-types`, `fabro-util`
- **Evidence:** lib/apps/fabro-mcp-server/Cargo.toml — declares a distinct MCP server library package; lib/apps/fabro-mcp-server/src/server.rs:start — owns the rmcp stdio service lifecycle

### `fabro-server` — Fabro HTTP Server

- **Purpose:** Hosts Fabro's HTTP control plane and web surface while coordinating persisted run state, workers, schedulers, sessions, authentication, and integrations.
- **Paths:** `lib/apps/fabro-server/**`
- **Entry points:** `lib/apps/fabro-server/src/serve.rs:serve_command`, `lib/apps/fabro-server/src/server.rs:build_router`
- **Owns:** Server startup and shutdown, AppState, API and web routing, authentication, scheduling, worker control, and integration coordination
- **Depends on:** `fabro-agent`, `fabro-api`, `fabro-auth`, `fabro-automation`, `fabro-build-support`, `fabro-client`, `fabro-config`, `fabro-db`, `fabro-environment`, `fabro-github`, `fabro-graphviz`, `fabro-hooks`, `fabro-http`, `fabro-http-api-contract`, `fabro-install`, `fabro-interview`, `fabro-llm`, `fabro-manifest`, `fabro-mcp-store`, `fabro-model`, `fabro-proc`, `fabro-redact`, `fabro-sandbox`, `fabro-slack`, `fabro-spa`, `fabro-static`, `fabro-store`, `fabro-tool`, `fabro-types`, `fabro-util`, `fabro-validate`, `fabro-variable`, `fabro-vault`, `fabro-workflow`
- **Evidence:** lib/apps/fabro-server/Cargo.toml — declares the HTTP server package and its application dependencies; lib/apps/fabro-server/src/server.rs:AppState — centralizes the service's stores, runtimes, schedulers, credentials, integrations, and shutdown state

### `fabro-spa` — Embedded SPA Assets

- **Purpose:** Provides compile-time embedded production SPA lookup, bytes, and content hashes to the Rust server.
- **Paths:** `lib/apps/fabro-spa/Cargo.toml`, `lib/apps/fabro-spa/src/**`
- **Entry points:** `lib/apps/fabro-spa/src/lib.rs:get`, `lib/apps/fabro-spa/src/lib.rs:AssetBytes`
- **Owns:** Compile-time SPA embedding, asset lookup, byte and hash metadata, and source-map exclusion
- **Evidence:** lib/apps/fabro-spa/Cargo.toml — declares a distinct embedded-assets package; lib/apps/fabro-spa/src/lib.rs:EmbeddedAssets — defines compile-time asset embedding and lookup; lib/apps/fabro-server/src/static_files.rs — consumes the embedded asset interface

### `fabro-acp` — Agent Client Protocol Runtime

- **Purpose:** Launches and controls Agent Client Protocol processes through Fabro sandboxes and translates their sessions into run results.
- **Paths:** `lib/components/fabro-acp/**`
- **Entry points:** `lib/components/fabro-acp/src/command.rs:AcpProcessSpec`, `lib/components/fabro-acp/src/session.rs:run_acp_turn`
- **Owns:** ACP process specifications, transport and session lifetime, live steering, cancellation, and exit translation
- **Depends on:** `fabro-sandbox`, `fabro-types`, `fabro-util`
- **Evidence:** lib/components/fabro-acp/Cargo.toml — declares the ACP backend and optional sandbox runtime edge; lib/components/fabro-acp/tests/session.rs — exercises the ACP session boundary

### `fabro-agent` — Coding Agent Runtime

- **Purpose:** Runs programmable coding-agent sessions with model profiles, context management, native and MCP tools, permissions, and subagents.
- **Paths:** `lib/components/fabro-agent/**`
- **Entry points:** `lib/components/fabro-agent/src/session.rs:Session`, `lib/components/fabro-agent/src/tool_registry.rs:ToolRegistry`
- **Owns:** Agent session history, prompts and profiles, tool execution, context compaction, permissions, questions, todos, and subagents
- **Depends on:** `fabro-auth`, `fabro-config`, `fabro-http`, `fabro-llm`, `fabro-mcp`, `fabro-model`, `fabro-sandbox`, `fabro-static`, `fabro-template`, `fabro-types`, `fabro-util`, `fabro-vault`
- **Evidence:** lib/components/fabro-agent/Cargo.toml — describes a programmable agentic loop and its runtime dependencies; lib/components/fabro-agent/src/lib.rs — exposes the session, profile, tool, permission, history, and subagent facade

### `fabro-automation` — Automation Definitions and Storage

- **Purpose:** Validates, versions, imports, and durably stores scheduled, API-triggered, and manual automation definitions.
- **Paths:** `lib/components/fabro-automation/**`
- **Entry points:** `lib/components/fabro-automation/src/store.rs:AutomationStore`, `lib/components/fabro-automation/src/migrations.rs:import_legacy_directory_once`
- **Owns:** Automation identifiers, targets, triggers, revisions, SQLite records, and legacy import
- **Depends on:** `fabro-db`
- **Evidence:** lib/components/fabro-automation/Cargo.toml — declares the automation domain and durable storage boundary; lib/components/fabro-automation/migrations/2026071101_file_definitions_to_sqlite.rs — evolves the owned persistence format

### `fabro-checkpoint` — Git Checkpoint Storage

- **Purpose:** Stores workflow checkpoints and metadata in Git commits and dedicated metadata branches.
- **Paths:** `lib/components/fabro-checkpoint/**`
- **Entry points:** `lib/components/fabro-checkpoint/src/branch.rs:BranchStore`, `lib/components/fabro-checkpoint/src/git.rs:Store`
- **Owns:** Checkpoint commits, Git trees, metadata branches, authorship, trailers, and checkpoint errors
- **Depends on:** `fabro-config`, `fabro-store`, `fabro-types`
- **Evidence:** lib/components/fabro-checkpoint/Cargo.toml — identifies Git-backed workflow checkpoint storage; lib/components/fabro-checkpoint/src/lib.rs — exposes the branch, Git, author, trailer, and error surface

### `fabro-dump` — Run Dump Materialization

- **Purpose:** Materializes stored run projections, events, checkpoints, artifacts, and blobs into a portable directory tree.
- **Paths:** `lib/components/fabro-dump/**`
- **Entry points:** `lib/components/fabro-dump/src/lib.rs:RunDump`, `lib/components/fabro-dump/src/lib.rs:RunDump::write_to_dir`
- **Owns:** Dump layout, stage ranking, blob hydration, serialization, and directory writing
- **Depends on:** `fabro-store`, `fabro-types`
- **Evidence:** lib/components/fabro-dump/Cargo.toml — gives the operation a distinct crate and storage dependency; lib/components/fabro-dump/src/lib.rs:RunDump — contains the public dump-building lifecycle

### `fabro-environment` — Environment Definitions and Storage

- **Purpose:** Validates, seeds, versions, imports, and durably stores server-owned execution environment definitions.
- **Paths:** `lib/components/fabro-environment/**`
- **Entry points:** `lib/components/fabro-environment/src/store.rs:EnvironmentStore`, `lib/components/fabro-environment/src/store.rs:seed_default_environment`
- **Owns:** Environment identifiers, revisions, drafts, SQLite records, built-in seeding, and legacy import
- **Depends on:** `fabro-config`, `fabro-db`, `fabro-types`
- **Evidence:** lib/components/fabro-environment/Cargo.toml — declares a server-owned environment domain and store; lib/components/fabro-environment/tests/store.rs — exercises the independent persistence boundary

### `fabro-github` — GitHub Authentication and API

- **Purpose:** Resolves GitHub credentials and performs authenticated App, repository, branch, and pull-request operations.
- **Paths:** `lib/components/fabro-github/**`
- **Entry points:** `lib/components/fabro-github/src/lib.rs:GitHubCredentials`, `lib/components/fabro-github/src/lib.rs:create_pull_request`
- **Owns:** GitHub credentials and token minting, API translation, repository URL handling, and pull-request lifecycle calls
- **Depends on:** `fabro-http`, `fabro-redact`, `fabro-static`, `fabro-types`
- **Evidence:** lib/components/fabro-github/Cargo.toml — describes the GitHub App authentication and API adapter; lib/components/fabro-github/src/lib.rs:GitHubContext — defines the credential context and testable HTTP boundary

### `fabro-graphviz` — Workflow Graph Language

- **Purpose:** Parses Graphviz DOT into Fabro's typed graph model and handles conditions, stylesheets, fidelity, and graph rendering.
- **Paths:** `lib/components/fabro-graphviz/**`
- **Entry points:** `lib/components/fabro-graphviz/src/parser/mod.rs:parse`, `lib/components/fabro-graphviz/src/render.rs:render_dot`
- **Owns:** DOT lexer, parser, semantic conversion, graph errors, condition and stylesheet syntax, and rendering normalization
- **Depends on:** `fabro-types`, `workflow-test-corpus`
- **Evidence:** lib/components/fabro-graphviz/Cargo.toml — names the crate as the DOT parser and graph data model; lib/components/fabro-graphviz/src/parser/mod.rs:parse — is the source-to-typed-graph entry point

### `fabro-hooks` — Workflow Lifecycle Hooks

- **Purpose:** Configures and executes user-defined workflow hooks and bridges tool hooks into the agent runtime.
- **Paths:** `lib/components/fabro-hooks/**`
- **Entry points:** `lib/components/fabro-hooks/src/runner.rs:HookRunner`, `lib/components/fabro-hooks/src/bridge.rs:WorkflowToolHookCallback`
- **Owns:** Hook definitions and selection, execution context, result merging, command and HTTP dispatch, and agent bridging
- **Depends on:** `fabro-agent`, `fabro-auth`, `fabro-http`, `fabro-llm`, `fabro-model`, `fabro-redact`, `fabro-types`, `fabro-util`
- **Evidence:** lib/components/fabro-hooks/Cargo.toml — identifies the workflow hook boundary and runtime dependencies; lib/components/fabro-hooks/tests/host_command_hooks.rs — tests host hooks through the public lifecycle

### `fabro-install` — Installation Persistence

- **Purpose:** Prepares, persists, and rolls back shared CLI/server installation settings, credentials, development tokens, and default environments.
- **Paths:** `lib/components/fabro-install/**`
- **Entry points:** `lib/components/fabro-install/src/lib.rs:InstallPersistencePlan`, `lib/components/fabro-install/src/lib.rs:persist_install_outputs_direct`
- **Owns:** Install persistence plans, settings and environment mutations, vault writes, development tokens, and rollback
- **Depends on:** `fabro-config`, `fabro-db`, `fabro-environment`, `fabro-static`, `fabro-types`, `fabro-util`, `fabro-vault`
- **Evidence:** lib/components/fabro-install/Cargo.toml — declares shared install primitives for CLI and server; lib/components/fabro-install/src/lib.rs:InstallPersistencePlan — groups the files, tokens, and vault state committed by one install

### `fabro-interview` — Human Interaction Runtime

- **Purpose:** Represents workflow questions and answers and provides console, callback, queue, control, recording, replay, and automatic interviewer implementations.
- **Paths:** `lib/components/fabro-interview/**`
- **Entry points:** `lib/components/fabro-interview/src/lib.rs:Interviewer`, `lib/components/fabro-interview/src/control.rs:ControlInterviewer`
- **Owns:** Question and answer protocol, interviewer request lifetime, timeout behavior, delivery, recording, and replay
- **Depends on:** `fabro-types`, `fabro-util`
- **Evidence:** lib/components/fabro-interview/Cargo.toml — defines interviewer traits and implementations as one crate; lib/components/fabro-interview/src/lib.rs:Interviewer — is the shared asynchronous human-interaction interface

### `fabro-llm` — Unified LLM Client

- **Purpose:** Provides a provider-neutral generation API with routing, middleware, retries, token and cost accounting, provider adapters, and wire codecs.
- **Paths:** `lib/components/fabro-llm/**`
- **Entry points:** `lib/components/fabro-llm/src/client.rs:Client`, `lib/components/fabro-llm/src/provider.rs:ProviderAdapter`
- **Owns:** Normalized generation types, adapter registry, provider authentication and transport, codecs, retries, middleware, and accounting
- **Depends on:** `fabro-auth`, `fabro-http`, `fabro-model`, `fabro-redact`, `fabro-static`, `fabro-types`, `fabro-util`
- **Evidence:** lib/components/fabro-llm/Cargo.toml — declares the unified multi-provider client; lib/components/fabro-llm/tests/it/wire/mod.rs — verifies provider codecs against one normalized boundary

### `fabro-manifest` — Run Manifest Construction

- **Purpose:** Resolves workflow and configuration inputs, collects static dependencies, and constructs self-contained run manifests with Git provenance.
- **Paths:** `lib/components/fabro-manifest/**`
- **Entry points:** `lib/components/fabro-manifest/src/lib.rs:build_run_manifest`, `lib/components/fabro-manifest/src/lib.rs:ManifestBuildInput`
- **Owns:** Manifest input and output, configuration resolution, workflow dependency collection, Git context, and pre-run push preparation
- **Depends on:** `fabro-api`, `fabro-config`, `fabro-github`, `fabro-graphviz`, `fabro-template`, `fabro-types`, `fabro-workflow`
- **Evidence:** lib/components/fabro-manifest/Cargo.toml — declares manifest construction and its graph, Git, and workflow dependencies; lib/components/fabro-manifest/src/lib.rs:build_run_manifest — is the shared assembly operation used by CLI, server, and MCP server

### `fabro-mcp` — MCP Client Runtime

- **Purpose:** Connects to configured Model Context Protocol servers, manages connections, discovers tools, and dispatches qualified calls.
- **Paths:** `lib/components/fabro-mcp/**`
- **Entry points:** `lib/components/fabro-mcp/src/client.rs:McpClient`, `lib/components/fabro-mcp/src/connection_manager.rs:McpConnectionManager`
- **Owns:** MCP client connections, stdio and HTTP transports, connection-manager state, tool discovery, and result conversion
- **Depends on:** `fabro-config`, `fabro-http`, `fabro-types`
- **Evidence:** lib/components/fabro-mcp/Cargo.toml — declares the MCP client and transport features; lib/components/fabro-mcp/tests/stdio_integration.rs — verifies the external process boundary over stdio

### `fabro-mcp-store` — MCP Server Catalog Storage

- **Purpose:** Durably stores, revisions, caches, and imports server-managed MCP server definitions.
- **Paths:** `lib/components/fabro-mcp-store/**`
- **Entry points:** `lib/components/fabro-mcp-store/src/store.rs:McpServerStore`, `lib/components/fabro-mcp-store/src/store.rs:import_legacy_directory_once`
- **Owns:** MCP definition records, optimistic revisions, catalog cache, and legacy directory import
- **Depends on:** `fabro-db`, `fabro-types`
- **Evidence:** lib/components/fabro-mcp-store/Cargo.toml — declares durable MCP catalog storage; lib/components/fabro-mcp-store/src/lib.rs — explicitly assigns persistence ownership to this crate

### `fabro-sandbox` — Execution Sandbox Abstraction

- **Purpose:** Defines sandbox and provider contracts and implements local, Docker, and Daytona execution lifecycles.
- **Paths:** `lib/components/fabro-sandbox/**`
- **Entry points:** `lib/components/fabro-sandbox/src/sandbox.rs:Sandbox`, `lib/components/fabro-sandbox/src/provider.rs:SandboxProviderRegistry`
- **Owns:** Sandbox filesystem, process, and terminal interface; provider lifecycle; clone setup; reconnect behavior; and provider implementations
- **Depends on:** `fabro-config`, `fabro-github`, `fabro-http`, `fabro-proc`, `fabro-redact`, `fabro-static`, `fabro-types`, `fabro-util`
- **Evidence:** lib/components/fabro-sandbox/Cargo.toml — defines provider features around a common sandbox crate; lib/components/fabro-sandbox/src/provider.rs:SandboxProvider — separates provider lifecycle from per-sandbox operations

### `fabro-slack` — Slack Interaction Integration

- **Purpose:** Connects to Slack Socket Mode and translates questions, answers, run events, and threads between Slack and Fabro.
- **Paths:** `lib/components/fabro-slack/**`
- **Entry points:** `lib/components/fabro-slack/src/connection.rs:run`, `lib/components/fabro-slack/src/client.rs:SlackClient`
- **Owns:** Slack credentials, Socket Mode lifecycle, API client, block rendering, payload parsing, thread registry, and dispatch
- **Depends on:** `fabro-http`, `fabro-interview`, `fabro-static`, `fabro-types`, `fabro-workflow`
- **Evidence:** lib/components/fabro-slack/Cargo.toml — declares the Slack interviewer integration; lib/components/fabro-slack/src/connection.rs:run — owns the Socket Mode event loop

### `fabro-store` — Run and Authentication Persistence

- **Purpose:** Persists run events, projections, blobs, artifacts, summaries, catalog indexes, and authentication grants over SlateDB, object storage, and SQLite.
- **Paths:** `lib/components/fabro-store/**`
- **Entry points:** `lib/components/fabro-store/src/slate/mod.rs:Database`, `lib/components/fabro-store/src/run_state.rs:RunProjectionReducer`
- **Owns:** Run event and projection lifecycle, blob and artifact layout, summary indexes, auth records, locking, and storage errors
- **Depends on:** `fabro-types`, `fabro-util`
- **Evidence:** lib/components/fabro-store/src/lib.rs — presents one persistence facade for events, projections, artifacts, summaries, blobs, and auth; lib/components/fabro-store/src/slate/mod.rs:Database — is the shared storage root for the owned stores

### `fabro-tool` — Run-Control Tools

- **Purpose:** Defines and executes shared run create, search, get, event, gather, interaction, and pairing tools over an abstract Fabro backend.
- **Paths:** `lib/components/fabro-tool/**`
- **Entry points:** `lib/components/fabro-tool/src/common.rs:FabroToolBackend`, `lib/components/fabro-tool/src/common.rs:tool_definitions`
- **Owns:** Tool names and schemas, parameter validation, backend-neutral operations, result records, and text rendering
- **Depends on:** `fabro-api`, `fabro-client`, `fabro-types`, `fabro-util`
- **Evidence:** lib/components/fabro-tool/Cargo.toml — identifies shared run-control tool behavior over API/client contracts; lib/components/fabro-tool/src/common.rs:FabroToolBackend — is the abstraction shared by CLI, server, workflow, and MCP server

### `fabro-tracker` — Issue Tracker Adapters

- **Purpose:** Provides a common issue-tracker interface with GitHub Projects and Linear implementations.
- **Paths:** `lib/components/fabro-tracker/**`
- **Entry points:** `lib/components/fabro-tracker/src/lib.rs:Tracker`, `lib/components/fabro-tracker/src/github.rs:GitHubTracker`
- **Owns:** Normalized issues and blockers, candidate selection and transitions, and GitHub Projects and Linear GraphQL adapters
- **Depends on:** `fabro-github`, `fabro-http`
- **Evidence:** lib/components/fabro-tracker/Cargo.toml — declares the tracker trait and provider adapters; lib/components/fabro-tracker/src/lib.rs:Tracker — defines the provider-neutral issue workflow

### `fabro-validate` — Workflow Graph Validation

- **Purpose:** Runs built-in and catalog-aware lint rules over typed workflow graphs and returns structured diagnostics.
- **Paths:** `lib/components/fabro-validate/**`
- **Entry points:** `lib/components/fabro-validate/src/lib.rs:validate`, `lib/components/fabro-validate/src/lib.rs:LintRule`
- **Owns:** Validation diagnostics, rule interface and registry, graph and catalog traversal, and error escalation
- **Depends on:** `fabro-acp`, `fabro-graphviz`, `fabro-model`, `fabro-types`, `workflow-test-corpus`
- **Evidence:** lib/components/fabro-validate/Cargo.toml — declares graph validation and its graph/catalog dependencies; lib/components/fabro-validate/src/rules/mod.rs:built_in_rules — forms the explicit built-in rule registry

### `fabro-variable` — Workflow Variable Storage

- **Purpose:** Validates, durably stores, snapshots, and imports workflow-visible non-sensitive variables.
- **Paths:** `lib/components/fabro-variable/**`
- **Entry points:** `lib/components/fabro-variable/src/lib.rs:VariableStore`, `lib/components/fabro-variable/src/lib.rs:import_legacy_json_once`
- **Owns:** Variable validation, SQLite records, render-context snapshots, and legacy JSON import
- **Depends on:** `fabro-db`, `fabro-types`
- **Evidence:** lib/components/fabro-variable/Cargo.toml — defines workflow-visible variables as a storage concern; lib/components/fabro-variable/tests/store.rs — verifies its independent persistence and import contract

### `fabro-workflow` — Workflow Orchestration Engine

- **Purpose:** Transforms, validates, initializes, executes, persists, resumes, and finalizes graph-defined Fabro runs.
- **Paths:** `lib/components/fabro-workflow/**`
- **Entry points:** `lib/components/fabro-workflow/src/operations/start.rs:start`, `lib/components/fabro-workflow/src/pipeline/execute.rs:execute`
- **Owns:** Run operations, workflow phases, node handlers, run services, events, checkpoints, Git, artifacts, hooks, status, steering, and cancellation
- **Depends on:** `fabro-acp`, `fabro-agent`, `fabro-auth`, `fabro-checkpoint`, `fabro-config`, `fabro-core`, `fabro-dump`, `fabro-github`, `fabro-graphviz`, `fabro-hooks`, `fabro-http`, `fabro-interview`, `fabro-llm`, `fabro-mcp`, `fabro-model`, `fabro-redact`, `fabro-sandbox`, `fabro-static`, `fabro-store`, `fabro-template`, `fabro-tool`, `fabro-types`, `fabro-util`, `fabro-validate`, `fabro-vault`, `workflow-test-corpus`
- **Evidence:** lib/components/fabro-workflow/Cargo.toml — declares the DOT-based runner and component dependencies; lib/components/fabro-workflow/src/pipeline/mod.rs — exposes the ordered transform, validate, initialize, execute, and finalize phases

### `fabro-build-support` — Rust Build-Script Support

- **Purpose:** Supplies shared compile-time Git and Cargo profile metadata to Fabro application build scripts.
- **Paths:** `lib/foundation/build-support/**`
- **Entry points:** `lib/foundation/build-support/git_metadata.rs:collect_from`, `lib/foundation/build-support/git_metadata.rs:cargo_profile`
- **Owns:** Compile-time Git SHA discovery, Cargo rerun paths, and profile discovery
- **Evidence:** lib/foundation/build-support/Cargo.toml — declares the shared build-support package; lib/foundation/build-support/git_metadata.rs:BuildGitMetadata — defines build-script Git and profile metadata; lib/apps/fabro-cli/build.rs — consumes the shared metadata collector; lib/apps/fabro-server/build.rs — consumes the shared metadata collector

### `fabro-build-tooling` — Fabro Build and Developer Tooling

- **Purpose:** Runs repository build, documentation, SPA, container, benchmark, release, and test-support automation.
- **Paths:** `lib/foundation/fabro-dev/**`, `test/bin/release_test.sh`, `test/analysis/bench-tests-diff.sql`
- **Entry points:** `lib/foundation/fabro-dev/src/main.rs:main`
- **Owns:** Developer CLI dispatch, subprocess plans, generated-reference checks, build and release workflows, and benchmark analysis
- **Depends on:** `container-packaging-and-deployment`, `fabro-cli`, `fabro-config`, `fabro-macros-metadata`, `fabro-spa`, `fabro-util`, `fabro-web-app`, `public-documentation`, `repository-development-policy`
- **Evidence:** lib/foundation/fabro-dev/src/lib.rs:Command — dispatches build, Docker, docs, release, SPA, and benchmark commands

### `fabro-api` — Generated Rust API Client

- **Purpose:** Generates the low-level Rust HTTP client and API type facade from OpenAPI while reusing canonical product types and verifying wire parity.
- **Paths:** `lib/foundation/fabro-api/**`
- **Entry points:** `lib/foundation/fabro-api/build.rs:main`, `lib/foundation/fabro-api/src/lib.rs:ApiClient`
- **Owns:** OpenAPI compatibility transformations, generation settings, type replacement map, generated-client facade, and wire/type parity tests
- **Depends on:** `fabro-automation`, `fabro-config`, `fabro-environment`, `fabro-http-api-contract`, `fabro-model`, `fabro-types`
- **Evidence:** lib/foundation/fabro-api/build.rs:main — reads the OpenAPI contract and writes generated Rust code to OUT_DIR; lib/foundation/fabro-api/tests/run_event_round_trip.rs — verifies identity and JSON parity for canonical reused types

### `fabro-auth` — Provider Credential Resolution

- **Purpose:** Resolves provider credentials and headers from environment or vault sources, refreshes OAuth credentials, and drives authentication strategies.
- **Paths:** `lib/foundation/fabro-auth/**`
- **Entry points:** `lib/foundation/fabro-auth/src/resolve.rs:CredentialResolver`, `lib/foundation/fabro-auth/src/strategy.rs:AuthStrategy`
- **Owns:** Credential-source precedence, provider discovery, OAuth refresh and write-back, header interpolation, and interactive auth state
- **Depends on:** `fabro-http`, `fabro-model`, `fabro-oauth`, `fabro-redact`, `fabro-static`, `fabro-types`, `fabro-vault`
- **Evidence:** lib/foundation/fabro-auth/Cargo.toml — declares typed provider credential resolution; lib/foundation/fabro-auth/src/resolve.rs:CredentialResolver::resolve — composes catalog policy, source lookup, headers, and refresh

### `fabro-client` — High-Level Fabro Service Client

- **Purpose:** Provides an authenticated Fabro service client over HTTP or Unix sockets with endpoint wrappers, SSE streams, refresh, and local auth storage.
- **Paths:** `lib/foundation/fabro-client/**`
- **Entry points:** `lib/foundation/fabro-client/src/client.rs:ClientBuilder::connect`, `lib/foundation/fabro-client/src/target.rs:ServerTarget`
- **Owns:** Connected transport state, operation wrappers, SSE buffering, token refresh, target normalization, and per-server CLI auth files
- **Depends on:** `fabro-api`, `fabro-http`, `fabro-model`, `fabro-static`, `fabro-types`, `fabro-util`
- **Evidence:** lib/foundation/fabro-client/Cargo.toml — distinguishes the high-level client from the generated API client; lib/foundation/fabro-client/src/client.rs:ClientState — owns transport, generated client, token, URL, and refresh coordination

### `fabro-config` — Layered Configuration and Runtime Paths

- **Purpose:** Parses, combines, migrates, validates, and resolves Fabro configuration layers into runtime settings and canonical paths.
- **Paths:** `lib/foundation/fabro-config/**`
- **Entry points:** `lib/foundation/fabro-config/src/builders.rs:ServerSettingsBuilder`, `lib/foundation/fabro-config/src/resolve/mod.rs`
- **Owns:** Source layers and merge semantics, defaults, parsing and validation, migrations, home/storage/runtime paths, daemon, envfile, and logging configuration
- **Depends on:** `fabro-macros-metadata`, `fabro-model`, `fabro-proc`, `fabro-static`, `fabro-types`, `fabro-util`
- **Evidence:** lib/foundation/fabro-config/Cargo.toml — declares the centralized configuration crate; lib/foundation/fabro-config/src/builders.rs — composes defaults and layers into dense runtime settings

### `fabro-core` — Generic Graph Execution Kernel

- **Purpose:** Executes generic directed graphs with handler, retry, lifecycle, cancellation, checkpoint, visit-limit, and stall-monitoring contracts.
- **Paths:** `lib/foundation/fabro-core/**`
- **Entry points:** `lib/foundation/fabro-core/src/executor.rs:Executor::run`, `lib/foundation/fabro-core/src/handler.rs:NodeHandler`
- **Owns:** Execution state, graph traversal, handler and lifecycle contracts, retry and visit decisions, cancellation, and stall watchdog
- **Depends on:** `fabro-types`, `fabro-util`
- **Evidence:** lib/foundation/fabro-core/Cargo.toml — identifies a generic kernel without higher-level workflow dependencies; lib/foundation/fabro-core/src/executor.rs:Executor::run — owns the traversal and execution lifecycle

### `fabro-db` — Shared SQLite Database Foundation

- **Purpose:** Opens and migrates the shared SQLite database, manages rollback snapshots and permissions, and defines the bundled schema.
- **Paths:** `lib/foundation/fabro-db/**`
- **Entry points:** `lib/foundation/fabro-db/src/lib.rs:Database::connect`, `lib/foundation/fabro-db/src/lib.rs:Database::migrate`
- **Owns:** SQLite pool policy, migration registry, snapshots, backup paths, permissions, tables, and indexes
- **Evidence:** lib/foundation/fabro-db/Cargo.toml — declares the shared SQLite foundation; lib/foundation/fabro-db/migrations/2026071101_secrets.sql — is one migration in the compiled shared schema

### `fabro-http` — Shared HTTP Transport Construction

- **Purpose:** Centralizes reqwest type exposure and synchronous and asynchronous HTTP client construction with Fabro proxy policy.
- **Paths:** `lib/foundation/fabro-http/**`
- **Entry points:** `lib/foundation/fabro-http/src/lib.rs:HttpClientBuilder`, `lib/foundation/fabro-http/src/lib.rs:test_http_client`
- **Owns:** Approved reqwest facade, proxy-policy resolution, client builders, and deterministic no-proxy test clients
- **Depends on:** `fabro-static`
- **Evidence:** lib/foundation/fabro-http/Cargo.toml — declares the shared reqwest wrapper; lib/foundation/fabro-http/src/lib.rs:ProxyPolicy — defines the common transport-construction policy

### `fabro-macros-metadata` — Compile-Time Macros and Option Metadata

- **Purpose:** Supplies Fabro derive and attribute macros plus the runtime option-metadata model used by configuration and documentation tooling.
- **Paths:** `lib/foundation/fabro-macros/**`, `lib/foundation/fabro-options-metadata/**`
- **Entry points:** `lib/foundation/fabro-macros/src/lib.rs:derive_options_metadata`, `lib/foundation/fabro-options-metadata/src/lib.rs:OptionsMetadata`
- **Owns:** Macro expansion for E2E gates, layer combination, and option metadata plus the runtime visitor and option-tree representation
- **Evidence:** lib/foundation/fabro-macros/src/options_metadata.rs:derive_impl — generates implementations against the runtime metadata crate; lib/foundation/fabro-macros/tests/options_metadata.rs — tests the compiler/runtime pair together

### `fabro-model` — LLM Model and Provider Catalog

- **Purpose:** Defines provider and model identity, capabilities, billing metadata, embedded catalog data, override merging, and selection.
- **Paths:** `lib/foundation/fabro-model/**`
- **Entry points:** `lib/foundation/fabro-model/src/catalog.rs:Catalog::builtin`, `lib/foundation/fabro-model/src/catalog.rs:Catalog::select`
- **Owns:** Provider and model IDs, catalog sources and indexes, auth declarations, capabilities, controls, codecs, reasoning, pricing, and billing
- **Depends on:** `fabro-static`
- **Evidence:** lib/foundation/fabro-model/Cargo.toml — names model metadata and resolution as the crate responsibility; lib/foundation/fabro-model/src/catalog/providers/openai.toml — is one tracked built-in provider catalog source

### `fabro-oauth` — OAuth PKCE and Callback Flow

- **Purpose:** Implements generic OAuth PKCE authorization, loopback callback serving, browser launch, code exchange, and token refresh.
- **Paths:** `lib/foundation/fabro-oauth/**`
- **Entry points:** `lib/foundation/fabro-oauth/src/lib.rs:run_browser_flow`, `lib/foundation/fabro-oauth/src/lib.rs:refresh_token`
- **Owns:** PKCE and state, authorization URLs, callback listener and shutdown, callback validation, exchange, and refresh
- **Depends on:** `fabro-http`, `fabro-redact`, `fabro-static`, `fabro-util`
- **Evidence:** lib/foundation/fabro-oauth/Cargo.toml — declares a generic OAuth 2.0 PKCE flow; lib/foundation/fabro-oauth/src/lib.rs:CallbackHandle — owns the ephemeral callback server lifecycle

### `fabro-proc` — OS Process Primitives

- **Purpose:** Wraps platform process primitives for signals, groups, advisory locks, pre-exec hooks, liveness, and process-title rewriting.
- **Paths:** `lib/foundation/fabro-proc/**`
- **Entry points:** `lib/foundation/fabro-proc/src/signal.rs:process_running`, `lib/foundation/fabro-proc/src/pre_exec.rs:pre_exec_setsid`
- **Owns:** Unix signals and process groups, cross-platform liveness, locks, child pre-exec configuration, and argv/title state
- **Evidence:** lib/foundation/fabro-proc/Cargo.toml — describes safe process-management wrappers; lib/foundation/fabro-proc/c/capture_argv.c — establishes the FFI boundary for title rewriting

### `fabro-redact` — Secret and Credential Redaction

- **Purpose:** Detects and redacts credential-like content in strings, URLs, JSON, and JSONL using embedded rules and entropy scanning.
- **Paths:** `lib/foundation/fabro-redact/**`
- **Entry points:** `lib/foundation/fabro-redact/src/lib.rs:redact_string`, `lib/foundation/fabro-redact/src/safe_url.rs:DisplaySafeUrl`
- **Owns:** Rule source and engine, entropy thresholds, overlap merging, structured redaction policy, and safe URL display
- **Evidence:** lib/foundation/fabro-redact/build.rs:main — compiles the tracked Gitleaks rule source into OUT_DIR; lib/foundation/fabro-redact/src/lib.rs:redact_string — composes entropy and rule-based detection

### `fabro-static` — Shared Static Conventions

- **Purpose:** Defines dependency-light canonical environment-variable names and registries for bootstrap and optional vault secrets.
- **Paths:** `lib/foundation/fabro-static/**`
- **Entry points:** `lib/foundation/fabro-static/src/env_vars.rs:EnvVars`, `lib/foundation/fabro-static/src/secret_registry.rs:is_bootstrap_secret`
- **Owns:** Canonical environment names and bootstrap and optional secret classification
- **Evidence:** lib/foundation/fabro-static/Cargo.toml — declares a no-dependency static registry; lib/foundation/fabro-static/src/env_vars.rs:EnvVars — centralizes environment names used across the workspace

### `fabro-telemetry` — Analytics and Crash Telemetry

- **Purpose:** Initializes analytics and crash reporting, builds anonymous context, buffers events, and delivers them across CLI and server lifecycles.
- **Paths:** `lib/foundation/fabro-telemetry/**`
- **Entry points:** `lib/foundation/fabro-telemetry/src/lib.rs:init_cli`, `lib/foundation/fabro-telemetry/src/lib.rs:shutdown`
- **Owns:** Process-global telemetry state, identifiers, buffer thread, event context, command sanitization, Segment delivery, and panic capture
- **Depends on:** `fabro-http`, `fabro-static`, `fabro-util`
- **Evidence:** lib/foundation/fabro-telemetry/Cargo.toml — declares analytics and crash reporting; lib/foundation/fabro-telemetry/src/lib.rs:Global — owns sender, identity, context, level, and background thread

### `fabro-template` — Template Rendering and Dependency Discovery

- **Purpose:** Renders MiniJinja templates with source-aware diagnostics, rooted stores, wrappers, and static dependency discovery.
- **Paths:** `lib/foundation/fabro-template/**`
- **Entry points:** `lib/foundation/fabro-template/src/lib.rs:render_named`, `lib/foundation/fabro-template/src/store.rs:TemplateStore`
- **Owns:** Template context, render modes, diagnostics, include safety, stores, caching and recording, and dependency closure
- **Depends on:** `fabro-types`, `fabro-util`
- **Evidence:** lib/foundation/fabro-template/Cargo.toml — declares the shared rendering boundary; lib/foundation/fabro-template/src/dependency.rs — owns include and import extraction and closure discovery

### `fabro-test` — Shared Integration-Test Infrastructure

- **Purpose:** Provides isolated CLI/server test contexts, twin and live mode control, process harnessing, snapshot normalization, and HTTP assertions.
- **Paths:** `lib/foundation/fabro-test/**`
- **Entry points:** `lib/foundation/fabro-test/src/lib.rs:TestContext`, `lib/foundation/fabro-test/src/lib.rs:TestMode`
- **Owns:** Temporary test home and storage, managed processes, mode and secret gating, environment isolation, snapshot filters, twins, and HTTP diagnostics
- **Depends on:** `fabro-config`, `fabro-http`, `fabro-install`, `fabro-proc`, `fabro-static`, `fabro-types`, `fabro-util`, `twin-github`, `twin-openai`, `workflow-test-corpus`
- **Evidence:** lib/foundation/fabro-test/Cargo.toml — declares shared integration-test utilities and twin dependencies; lib/foundation/fabro-test/src/lib.rs:TestContext — owns isolated paths, subprocesses, filters, and managed server state

### `fabro-types` — Shared Product Contracts and State Records

- **Purpose:** Defines serializable identifiers, settings, run and session events, projections, and other product vocabulary exchanged across Fabro boundaries.
- **Paths:** `lib/foundation/fabro-types/**`
- **Entry points:** `lib/foundation/fabro-types/src/lib.rs`, `lib/foundation/fabro-types/src/run_event/mod.rs:RunEvent`
- **Owns:** Canonical serde shapes and IDs for runs, stages, sessions, events, settings, projections, sandboxes, integrations, billing, and repositories
- **Depends on:** `fabro-model`, `fabro-util`
- **Evidence:** lib/foundation/fabro-types/Cargo.toml — describes shared record structs and enums; lib/foundation/fabro-types/src/lib.rs — is the single facade for canonical product vocabulary

### `fabro-util` — Cross-Cutting Runtime and CLI Utilities

- **Purpose:** Provides shared environment, filesystem, shell, terminal, logging, token, error, time, backoff, warning, and glob primitives.
- **Paths:** `lib/foundation/fabro-util/**`
- **Entry points:** `lib/foundation/fabro-util/src/lib.rs`, `lib/foundation/fabro-util/src/shell.rs:shell_quote`
- **Owns:** Low-level helper contracts plus warning, buffered log, environment, home, token, terminal, backoff, error, and glob state
- **Depends on:** `fabro-static`
- **Evidence:** lib/foundation/fabro-util/Cargo.toml — identifies shared runtime and terminal helpers; lib/foundation/fabro-util/src/run_log.rs — owns the buffered run-log guard lifecycle

### `fabro-vault` — Secret Vault and SQLite Store

- **Purpose:** Validates and stores workflow-visible secrets in file, memory, or SQLite stores with revision-aware updates and legacy import.
- **Paths:** `lib/foundation/fabro-vault/**`
- **Entry points:** `lib/foundation/fabro-vault/src/lib.rs:Vault::load`, `lib/foundation/fabro-vault/src/store.rs:SecretStore::open`
- **Owns:** Secret validation and redacted entries, atomic file persistence, SQL CRUD, revisions, snapshots, and legacy import
- **Depends on:** `fabro-db`, `fabro-static`, `fabro-types`
- **Evidence:** lib/foundation/fabro-vault/Cargo.toml — declares workflow-visible secret storage; lib/foundation/fabro-vault/src/store.rs:SecretStore::replace_if_revision — exposes concurrent refresh write-back semantics

### `fabro-web-app` — Fabro Browser Application

- **Purpose:** Builds and runs the React SPA for normal operations and first-run installation.
- **Paths:** `apps/fabro-web/**`
- **Excludes:** `apps/fabro-web/app/components/playground/**`
- **Entry points:** `apps/fabro-web/app/entry.tsx`, `apps/fabro-web/scripts/build.ts`
- **Owns:** Browser bundle and route graphs, install flow, shared browser runtime and UI, product operations UX, and public assets
- **Depends on:** `fabro-api-client-generation`, `fabro-http-api-contract`, `fabro-workflow-playground`
- **Evidence:** apps/fabro-web/package.json — declares the React application, custom build, tests, and API-client workspace edge; apps/fabro-web/app/entry.tsx — creates the browser root and selects normal or install routing

### `fabro-workflow-playground` — Browser Workflow Playground

- **Purpose:** Provides a self-contained workflow drafting, simulation, chat, visualization, file-generation, download, and run-launch surface.
- **Paths:** `apps/fabro-web/app/components/playground/**`
- **Entry points:** `apps/fabro-web/app/components/playground/playground.tsx:Playground`, `apps/fabro-web/app/components/playground/state/draft.ts:WorkflowDraft`
- **Owns:** Workflow draft schema and persistence, simulation, canvas, chat adaptation, generated project files, download, and launch controls
- **Depends on:** `fabro-http-api-contract`, `fabro-web-app`
- **Evidence:** apps/fabro-web/app/components/playground/playground.tsx:Playground — exposes a prop boundary framed for re-embedding; apps/fabro-web/app/components/playground/state/persist.ts:usePlaygroundDraft — owns versioned browser persistence

### `fabro-marketing-site` — Fabro Marketing Site

- **Purpose:** Builds and deploys the public Fabro site with landing content, blog, roadmap, showcase, install resources, and social assets.
- **Paths:** `apps/marketing/**`, `test/bin/install_test.sh`
- **Excludes:** `apps/marketing/.vercel/**`
- **Entry points:** `apps/marketing/src/pages/index.astro`, `apps/marketing/astro.config.mjs`, `apps/marketing/public/install.sh`
- **Owns:** Astro routes and layout, content collections, marketing presentation, workflow showcases, install resources, redirects, and deployment configuration
- **Evidence:** apps/marketing/package.json — declares an independent Astro application; apps/marketing/src/content.config.ts — defines typed roadmap, blog, and showcase collections; test/bin/install_test.sh — black-box tests the site's canonical install script

### `fabro-remotion-video` — Fabro Remotion Composition

- **Purpose:** Renders the branded FabroIntro motion-graphics video.
- **Paths:** `apps/remotion/**`
- **Entry points:** `apps/remotion/src/index.ts`, `apps/remotion/src/Root.tsx:RemotionRoot`
- **Owns:** Composition registration, frame timeline, image format, logo animation, brand assets, and rendered-video lifecycle
- **Evidence:** apps/remotion/package.json — declares an independent Remotion project and render target; apps/remotion/src/Root.tsx:RemotionRoot — declares composition identity, dimensions, frame rate, and duration

### `fabro-api-client-generation` — TypeScript API Client Generation

- **Purpose:** Configures, normalizes, and type-checks the generated TypeScript/Axios client for the Fabro HTTP contract.
- **Paths:** `lib/packages/fabro-api-client/package.json`, `lib/packages/fabro-api-client/openapitools.json`, `lib/packages/fabro-api-client/scripts/**`, `lib/packages/fabro-api-client/tests/**`, `lib/packages/fabro-api-client/tsconfig.json`
- **Entry points:** `lib/packages/fabro-api-client/package.json:scripts.generate`, `lib/packages/fabro-api-client/scripts/normalize-generated.ts`
- **Owns:** Generator versions and options, output location, normalization, strict compilation, and hand-written generated-shape invariants
- **Depends on:** `fabro-http-api-contract`
- **Evidence:** lib/packages/fabro-api-client/package.json — invokes pinned OpenAPI Generator against the shared YAML and writes src; lib/packages/fabro-api-client/tests/principal-exhaustive.ts — asserts a generated union contract at compile time

### `public-documentation` — Public Documentation

- **Purpose:** Owns authored Fabro user documentation, Mintlify presentation, the repository landing page, and published web-screenshot maintenance.
- **Paths:** `README.md`, `docs/public/**`, `docs/internal/updating-web-screenshots.md`
- **Excludes:** `docs/public/api-reference/fabro-api.yaml`, `docs/public/changelog/**`, `docs/public/images/*-workflow.svg`, `docs/public/images/tutorial-*.svg`, `docs/public/images/brave-search-research.svg`, `docs/public/images/how-fabro-works.svg`, `docs/public/images/nlspec-conformance.svg`, `docs/public/images/plan-implement-readme.svg`
- **Entry points:** `README.md`, `docs/public/docs.json`, `docs/public/getting-started/introduction.mdx`
- **Owns:** Mintlify navigation and presentation, public guides and reference prose, curated images and screenshots, syntax definitions, and repository overview
- **Depends on:** `documentation-demo-workflows`, `fabro-cli`, `fabro-http-api-contract`, `public-release-history`
- **Evidence:** docs/public/docs.json — declares the Mintlify theme, navigation, OpenAPI, and changelog surfaces; README.md — links to the published docs and embeds their canonical assets; docs/internal/updating-web-screenshots.md — defines the screenshot capture and verification workflow

### `public-release-history` — Published Changelog

- **Purpose:** Preserves and publishes dated user-facing release and change records independently of current reference documentation.
- **Paths:** `docs/public/changelog/**`
- **Entry points:** `docs/public/changelog/2026-07-25.mdx`
- **Owns:** Dated titles, migration warnings, feature summaries, and historical behavior notes
- **Depends on:** `public-documentation`
- **Evidence:** docs/public/docs.json — gives the changelog its own top-level tab and enumerates every page; docs/public/changelog/2026-07-25.mdx — is the newest dated release entry at the assessed revision

### `fabro-http-api-contract` — Fabro HTTP API Contract

- **Purpose:** Defines the OpenAPI-first wire contract used by the server, generated clients, conformance tests, and published API reference.
- **Paths:** `docs/public/api-reference/fabro-api.yaml`
- **Entry points:** `docs/public/api-reference/fabro-api.yaml`
- **Owns:** HTTP routes, request and response schemas, authentication declarations, and API-facing wire documentation
- **Evidence:** AGENTS.md — identifies the OpenAPI file as the HTTP interface source of truth; lib/foundation/fabro-api/build.rs:main — consumes the contract for Rust generation; lib/apps/fabro-server/tests/it/openapi_conformance.rs — reads it for router conformance

### `documentation-demo-workflows` — Executable Documentation Demos

- **Purpose:** Provides runnable workflow definitions, configuration, and prompts used by public tutorials and demonstrations.
- **Paths:** `docs/internal/demo/*.fabro`, `docs/internal/demo/*.toml`, `docs/internal/demo/prompts/**`
- **Entry points:** `docs/internal/demo/01-hello.fabro`, `docs/internal/demo/14-search-imagegen.toml`
- **Owns:** Executable example graphs, the image-generation run configuration, and shared demo prompt text
- **Depends on:** `fabro-cli`, `fabro-sandbox`, `fabro-workflow`
- **Evidence:** docs/public/tutorials/hello-world.mdx — invokes a demo workflow path directly; docs/internal/demo/14-search-imagegen.toml — selects the demo graph, environment, and output assets

### `internal-engineering-guidance` — Internal Engineering Guidance

- **Purpose:** Records active repository-wide engineering policies and maintained architecture and runtime contracts.
- **Paths:** `docs/internal/*-strategy.md`, `docs/internal/*-policy.md`, `docs/internal/events.md`, `docs/internal/fabro-event-schema-v2-concrete-shape.md`, `docs/internal/llm-client-resolution.md`, `docs/internal/run-directory-keys.md`
- **Entry points:** `docs/internal/events-strategy.md`, `docs/internal/testing-strategy.md`, `docs/internal/error-handling-strategy.md`
- **Owns:** Logging, events, testing, migrations, secrets, error handling, React effects, panic, event catalog, LLM resolution, parallelism, and run-file guidance
- **Depends on:** `fabro-cli`, `fabro-config`, `fabro-server`, `fabro-types`, `fabro-web-app`, `fabro-workflow`
- **Evidence:** AGENTS.md — makes the strategy and policy documents mandatory before related changes; docs/internal/events.md — is the maintained serialized event catalog

### `product-context` — Internal Product Context

- **Purpose:** Maintains product intent, audience, current shape, success signals, and stable technical and product constraints.
- **Paths:** `docs/internal/product/**`
- **Entry points:** `docs/internal/product/product-description.md`, `docs/internal/product/current-state.md`
- **Owns:** Business problem, personas, product description, current state, success metrics, and product-level technical requirements
- **Evidence:** docs/internal/product/current-state.md — identifies itself as a concise current product snapshot; docs/internal/product/technical-requirements.md — records stable constraints for product changes

### `twin-openai` — OpenAI Protocol Twin

- **Purpose:** Provides a deterministic OpenAI-compatible HTTP service for black-box and protocol-contract tests.
- **Paths:** `test/twin/openai/**`
- **Entry points:** `test/twin/openai/src/main.rs:main`, `test/twin/openai/src/lib.rs:build_app`
- **Owns:** OpenAI-compatible routes, scenario queues, request logs, deterministic IDs, streaming and failure behavior, admin APIs, and debug UI
- **Depends on:** `fabro-http`, `fabro-static`
- **Evidence:** test/twin/openai/Cargo.toml — declares a fake OpenAI-compatible library and binary; test/twin/openai/src/state.rs:AppState — owns namespaced counters, scenario queues, and request logs

### `twin-github` — GitHub Protocol Twin

- **Purpose:** Provides an in-process fake GitHub service with seeded mutable state and temporary Git repositories.
- **Paths:** `test/twin/github/**`
- **Entry points:** `test/twin/github/src/server.rs:TestServer::start`, `test/twin/github/src/server.rs:build_router`
- **Owns:** Fake GitHub App, OAuth, REST, GraphQL, smart-HTTP, repositories, pull requests, releases, projects, tokens, and test keys
- **Depends on:** `fabro-http`
- **Evidence:** test/twin/github/Cargo.toml — declares an independent fake GitHub service; test/twin/github/src/state.rs:AppState — owns the seeded and mutable GitHub-domain state

### `workflow-test-corpus` — Shared Workflow Compatibility Fixtures

- **Purpose:** Supplies reusable workflow, compatibility, configuration, prompt, partial, and template inputs to cross-crate tests.
- **Paths:** `test/*.fabro`, `test/attractor/**`, `test/dot-compatibility/**`, `test/templated_inputs/**`, `test/templated_unbound_imported/**`, `test/templated_unbound_partial/**`, `test/templates/**`
- **Entry points:** `test/simple.fabro`, `test/attractor/simple_example.dot`, `test/templates/static_dependencies/workflow.fabro`
- **Owns:** Representative workflow syntax and behavior cases, Attractor compatibility graphs, DOT fixtures, and template dependency trees
- **Evidence:** lib/foundation/fabro-test/src/lib.rs:TestContext::install_fixture — resolves named inputs from the shared test directory; lib/components/fabro-workflow/tests/it/attractor_compat.rs — enumerates the Attractor corpus

### `documentation-workflow-tests` — Documentation Workflow Conformance

- **Purpose:** Extracts, curates, validates, preflights, and executes workflow examples and companion files derived from Fabro documentation.
- **Paths:** `test/docs/**`
- **Entry points:** `test/docs/run_tests.sh`, `test/docs/extract_dots.py:main`, `test/docs/CHECKLIST.md`
- **Owns:** Documentation example corpus, extraction and stub generation, validation and execution phases, parallel runner state, and checklist
- **Depends on:** `fabro-cli`, `fabro-workflow`, `public-documentation`
- **Evidence:** test/docs/run_tests.sh — discovers and runs every tracked documentation workflow; test/docs/extract_dots.py:main — extracts complete graphs and creates companion fixtures

### `swe-bench-evaluation` — SWE-Bench Evaluation Workflow

- **Purpose:** Generates Fabro patches for SWE-bench Lite, grades them, monitors runs, builds environments, and records normalized summaries.
- **Paths:** `evals/swe-bench/*.py`, `evals/swe-bench/*.fabro`, `evals/swe-bench/*.txt`, `evals/swe-bench/README.md`
- **Entry points:** `evals/swe-bench/run_eval.py:main`, `evals/swe-bench/evaluate_daytona.py:main`, `evals/swe-bench/record_results.py:main`
- **Owns:** Dataset selection, per-instance workflow generation, sandbox specs, subprocess orchestration, patch extraction, grading, monitoring, and scoreboard schema
- **Depends on:** `fabro-cli`, `fabro-sandbox`, `fabro-workflow`
- **Evidence:** evals/swe-bench/README.md — defines the generate, evaluate, and record lifecycle; evals/swe-bench/run_eval.py:run_instance — creates per-instance Fabro inputs and invokes the CLI

### `repository-development-policy` — Repository Development Policy

- **Purpose:** Defines workspace, dependency, formatting, lint, test, version-control, contributor, and coding-agent development contracts.
- **Paths:** `.cargo/**`, `.config/**`, `.gitattributes`, `.gitignore`, `AGENTS.md`, `CONTRIBUTING.md`, `Cargo.toml`, `package.json`, `bunfig.toml`, `clippy.toml`, `rustfmt.toml`
- **Entry points:** `Cargo.toml:[workspace]`, `package.json:workspaces`, `AGENTS.md`
- **Owns:** Workspace membership and policy, tool aliases, test profiles, lints and formatting, tracked path treatment, contributor workflow, and agent instructions
- **Depends on:** `fabro-build-tooling`
- **Evidence:** Cargo.toml — declares Rust workspace members, dependencies, lints, and profiles; .cargo/config.toml — exposes cargo dev and repository test policy; AGENTS.md — defines architectural and workflow instructions

### `repository-ci` — Pull-Request and Branch CI

- **Purpose:** Runs branch and pull-request validation for Rust and TypeScript and configures GitHub Actions static validation.
- **Paths:** `.github/workflows/rust.yml`, `.github/workflows/typescript.yml`, `.github/zizmor.yml`
- **Entry points:** `.github/workflows/rust.yml`, `.github/workflows/typescript.yml`
- **Owns:** Path triggers, formatting, linting, generated-doc checks, tests, E2E modes, TypeScript checks, builds, concurrency, and workflow-lint policy
- **Depends on:** `fabro-api-client-generation`, `fabro-build-tooling`, `fabro-web-app`, `public-documentation`, `repository-development-policy`, `twin-openai`
- **Evidence:** .github/workflows/rust.yml — runs Rust formatting, lint, generated-document, workspace test, and twin E2E jobs; .github/workflows/typescript.yml — checks and builds the Bun workspace and embedded SPA

### `release-distribution-automation` — Release and Package Publication

- **Purpose:** Cuts nightly releases and publishes CLI archives, GitHub Releases, multi-architecture images, attestations, and Homebrew formulas.
- **Paths:** `.github/workflows/nightly.yml`, `.github/workflows/release.yml`, `installer/**`
- **Entry points:** `.github/workflows/nightly.yml`, `.github/workflows/release.yml`, `installer/fabro.rb.template`
- **Owns:** Nightly tag creation, release matrix, archives and checksums, attestations, GitHub Releases, image publication, and Homebrew channels
- **Depends on:** `container-packaging-and-deployment`, `fabro-build-tooling`, `fabro-cli`, `fabro-web-app`, `repository-development-policy`
- **Evidence:** .github/workflows/release.yml — packages target matrices and publishes releases, images, and formulas; installer/fabro.rb.template — defines platform archives, checksums, installation, and smoke tests

### `container-packaging-and-deployment` — Container Packaging and Deployment

- **Purpose:** Packages Fabro as a runtime container and defines local, production, Tailscale, and split-web Compose deployments.
- **Paths:** `.dockerignore`, `.env.example`, `Dockerfile`, `docker-compose*.yaml`, `docker/**`
- **Entry points:** `Dockerfile`, `docker/entrypoint.sh`, `docker-compose.yaml`
- **Owns:** Container image layout, runtime packages and user, storage and Docker socket handoff, preflight checks, proxy behavior, Compose topology, volumes, ports, and health checks
- **Depends on:** `fabro-build-tooling`, `fabro-cli`, `fabro-server`, `fabro-web-app`
- **Evidence:** Dockerfile — consumes the architecture-specific staged binary and installs the runtime entrypoint; docker-compose.yaml — defines the primary image, state, socket, port, and health-check contract

### `fabro-repository-automation` — Fabro-Native Repository Automation

- **Purpose:** Configures Fabro's development environment and named workflow graphs, prompts, permissions, and project defaults for repository work.
- **Paths:** `.fabro/Dockerfile`, `.fabro/project.toml`, `.fabro/workflows/**`
- **Excludes:** `.fabro/workflows/goal/workflow.svg`
- **Entry points:** `.fabro/project.toml`, `.fabro/workflows/implement-plan/workflow.fabro`, `.fabro/workflows/smoke/workflow.fabro`
- **Owns:** Repository pull-request defaults, Daytona development environment, named workflow catalog, local prompts, GitHub permissions, and maintenance commands
- **Depends on:** `fabro-build-tooling`, `fabro-cli`, `fabro-config`, `fabro-github`, `fabro-graphviz`, `fabro-sandbox`, `fabro-workflow`, `repository-development-policy`
- **Evidence:** .fabro/project.toml — selects the repository environment, resources, lifecycle, labels, and pull-request defaults; .fabro/workflows/implement-plan/workflow.fabro — invokes repository Cargo and Bun verification and build tooling

### `coding-agent-automation` — Repository Coding-Agent Automation

- **Purpose:** Supplies repository-local review prompts, documentation and changelog skills, edit hooks, and an image-generation helper to coding agents.
- **Paths:** `.ai/prompts/**`, `.claude/settings.json`, `.claude/skills/**`, `bin/agent/**`
- **Excludes:** `.claude/skills/*/watermark`
- **Entry points:** `.ai/prompts/code-review-fast.md`, `.claude/skills/changelog/SKILL.md`, `.claude/skills/docs/SKILL.md`, `bin/agent/imagegen`
- **Owns:** Code-review orchestration, changelog and documentation maintenance, post-edit formatting hook, and agent image-generation command
- **Depends on:** `public-documentation`, `public-release-history`
- **Evidence:** .ai/prompts/code-review-deep-1.md — begins the multi-stage review artifact pipeline; .claude/skills/docs/SKILL.md — defines the code-to-public-documentation update workflow; .claude/settings.json — registers the repository post-edit Rust formatting hook

## Exclusions and Unmapped Code

- `lib/packages/fabro-api-client/src/**` — Generated TypeScript/Axios output written by the package's pinned OpenAPI Generator command; generated headers and .openapi-generator metadata corroborate the output boundary.
- `apps/marketing/.vercel/**` — Vercel CLI link metadata whose own README identifies it as automatically created local project/team state.
- `lib/apps/fabro-spa/assets/**` — Placeholder for ignored embedded-SPA build output; repository instructions and .gitignore identify the directory as generated.
- `docs/brainstorms/**`, `docs/ideation/**`, `docs/plans/**`, `docs/superpowers/plans/**`, `docs/superpowers/specs/**`, `docs/internal/cargo-target-apfs-churn-plan.md`, `docs/internal/cli-workflow-coupling-audit.md`, `docs/internal/event-schema-competitive-analysis.md`, `docs/internal/fabro-event-schema-v2-proposal.md`, `docs/internal/mcp-server-qa-test-plan.md`, `docs/internal/plan-events-as-source-of-truth-follow-ups.md`, `docs/internal/plan-events-as-source-of-truth.md`, `docs/internal/slow-test-opportunities-2026-04-07.md` — Point-in-time brainstorms, implementation plans, audits, research, handoffs, and superseded proposals rather than maintained source contracts.
- `docs/internal/demo/*.svg`, `docs/internal/demo/*.png`, `docs/public/images/*-workflow.svg`, `docs/public/images/tutorial-*.svg`, `docs/public/images/brave-search-research.svg`, `docs/public/images/how-fabro-works.svg`, `docs/public/images/nlspec-conformance.svg`, `docs/public/images/plan-implement-readme.svg` — Graphviz-generated SVG and PNG renderings whose executable or documentation graph sources remain assigned.
- `docs/internal/licenses/**` — Vendored third-party Graphviz license text rather than Fabro source.
- `evals/swe-bench/scoreboard/**` — Committed evaluation records generated by record_results.py, not executable evaluation source.
- `.fabro/skills/rust-style-guide/**` — Vendored policy payload copied from the brynary/rust-style-guide repository at a recorded commit.
- `Cargo.lock`, `bun.lock` — Machine-maintained dependency resolution snapshots consumed in locked or frozen mode.
- `.claude/skills/*/watermark` — Generated progress-state commit SHAs overwritten by the owning skill workflows.
- `.fabro/project.toml.bak` — Stale backup of the canonical .fabro/project.toml configuration.
- `.fabro/workflows/goal/workflow.svg`, `.github/assets/**` — Non-runtime workflow illustration and unreferenced pull-request review screenshots.
- `CLAUDE.md`, `install.sh`, `install.md` — Tracked symlink aliases whose canonical targets are assigned elsewhere, avoiding duplicate assessment of identical content.
- `LICENSE.md` — Repository legal text rather than an implementation or documentation component.
- `docs/internal/assets/brand/github-header-v2-mesh.png` — unmapped
- `docs/internal/assets/brand/github-header-v2-mesh.svg` — unmapped
- `docs/internal/assets/brand/logo/logotype-black.svg` — unmapped
- `docs/internal/assets/brand/logo/logotype.svg` — unmapped
- `docs/internal/assets/brand/logo/symbol-black.svg` — unmapped
- `docs/internal/assets/brand/logo/symbol.svg` — unmapped
- `docs/internal/assets/brand/palette-lockups.svg` — unmapped
- `docs/internal/assets/brand/palette-mockup-icons.svg` — unmapped
- `docs/internal/assets/brand/palette-mockup.svg` — unmapped
- `docs/internal/assets/brand/palette.png` — unmapped
- `docs/internal/assets/brand/palette.svg` — unmapped
- `docs/internal/assets/brand/social-card.html` — unmapped
- `docs/internal/assets/brand/social-card.png` — unmapped
- `docs/internal/assets/brand/twitter-card-v0.176.1.html` — unmapped
- `docs/internal/assets/brand/twitter-card-v0.176.1.png` — unmapped

## Open Questions

- Should the currently unreferenced docs/internal/assets brand collateral be assigned to a maintained brand component, or remain explicitly unmapped until an ownership and update workflow is identified?
- Should the first-run browser installer become a separate component if its route and state lifecycle gains an independent entry point, rather than remaining inside fabro-web-app?
- Should fabro-workflow eventually split run-operation/materialization ownership from pipeline execution if those facades acquire independent state and public contracts?
