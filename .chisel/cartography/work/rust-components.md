# Cartography scout report: Rust components

Assessed revision: `2bcf94fed8a9b429f18d9196fa824711d6f4cb0a` (`2bcf94fed`)

Scope: tracked files under `lib/components/**`, with workspace manifests and public consumers consulted only as boundary evidence.

## Boundary synthesis

The scope contains 23 non-published, shared in-repository Rust library crates. The primary proposal keeps one component per crate: every crate has its own manifest and crate root, exposes a distinct public vocabulary or execution facade, and owns a separate domain state, external protocol, or runtime lifecycle. This also keeps the regular Cargo dependency edges directional and makes every glob non-overlapping.

The four SQLite-backed resource crates (`fabro-automation`, `fabro-environment`, `fabro-mcp-store`, and `fabro-variable`) use a similar storage pattern, but their identifiers, validation, import formats, tables, and public consumers differ; they are therefore proposed as separate components. The two-file crates (`fabro-dump`, `fabro-install`, and `fabro-manifest`) are also kept separate because each contains a substantial public operation and has a distinct dependency/consumer boundary rather than being a collection of incidental helpers.

Checked-in snapshots, prompt templates, grammars, migrations, and test fixture keys are assigned to the component whose behavior they exercise. No tracked file in this scope has evidence of being vendored or build output, and no checked-in generated source is excluded.

## Proposed components

### `fabro-acp` — Agent Client Protocol runtime

- Purpose: Launch and control Agent Client Protocol processes through Fabro sandboxes and translate their sessions into Fabro run results.
- Globs: `lib/components/fabro-acp/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-acp/src/lib.rs`, `lib/components/fabro-acp/src/command.rs:AcpProcessSpec`, `lib/components/fabro-acp/src/session.rs:run_acp_turn`
- Owns: ACP process specifications; ACP transport/session lifetime; live steering and cancellation handles; ACP process exit/error translation.
- Depends on candidates: `fabro-sandbox`
- Evidence:
  - `lib/components/fabro-acp/Cargo.toml` — declares an ACP backend crate with a default `runtime` feature and an optional runtime dependency on `fabro-sandbox`.
  - `lib/components/fabro-acp/src/lib.rs` — exposes the process specification and runtime session/control API while keeping transport internal.
  - `lib/components/fabro-acp/tests/session.rs` — exercises the session boundary as an integration test.
- Scoped tracked files: 8

### `fabro-agent` — Coding agent runtime

- Purpose: Run programmable coding-agent sessions, including model profiles, context management, native tools, permissions, MCP tools, and subagents.
- Globs: `lib/components/fabro-agent/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-agent/src/lib.rs`, `lib/components/fabro-agent/src/session.rs:Session`, `lib/components/fabro-agent/src/tool_registry.rs:ToolRegistry`, `lib/components/fabro-agent/src/cli.rs:run_with_args`
- Owns: agent session state and history; agent/model profiles and prompt templates; tool registry and execution lifecycle; context compaction; todo/question/subagent runtimes; agent-emitted events.
- Depends on candidates: `fabro-llm`, `fabro-mcp`, `fabro-sandbox`
- Evidence:
  - `lib/components/fabro-agent/Cargo.toml` — describes a programmable agentic loop and declares direct dependencies on the LLM, MCP, and sandbox crates.
  - `lib/components/fabro-agent/src/lib.rs` — presents one crate-level facade spanning sessions, profiles, tools, permissions, history, and subagent supervision.
  - `lib/components/fabro-agent/tests/it/main.rs` — anchors the crate's integration-test suite; profile prompt snapshots and `.j2` templates are behavioral assets of the same runtime.
- Scoped tracked files: 66

### `fabro-automation` — Automation definitions and storage

- Purpose: Validate, version, import, and durably store scheduled, API-triggered, and manual Fabro automation definitions.
- Globs: `lib/components/fabro-automation/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-automation/src/lib.rs`, `lib/components/fabro-automation/src/store.rs:AutomationStore`, `lib/components/fabro-automation/src/migrations.rs:import_legacy_directory_once`
- Owns: automation IDs and revisions; automation targets and triggers; canonical revision calculation; automation SQLite records; legacy file-definition import.
- Depends on candidates: `[]`
- Evidence:
  - `lib/components/fabro-automation/Cargo.toml` — declares “Automation domain and durable storage for Fabro” and uses the shared database foundation.
  - `lib/components/fabro-automation/src/lib.rs` — re-exports the automation domain, validation errors, revisions, store, and one-time importer as one API.
  - `lib/components/fabro-automation/tests/store.rs` and `lib/components/fabro-automation/migrations/2026071101_file_definitions_to_sqlite.rs` — cover and evolve the owned automation persistence format.
- Scoped tracked files: 9

### `fabro-checkpoint` — Git checkpoint storage

- Purpose: Store workflow checkpoints and metadata in Git commits and dedicated metadata branches.
- Globs: `lib/components/fabro-checkpoint/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-checkpoint/src/lib.rs`, `lib/components/fabro-checkpoint/src/branch.rs:BranchStore`, `lib/components/fabro-checkpoint/src/git.rs:Store`
- Owns: Git tree entries and checkpoint commits; metadata-branch naming and access; checkpoint commit authorship and trailers; checkpoint-specific error types.
- Depends on candidates: `fabro-store`
- Evidence:
  - `lib/components/fabro-checkpoint/Cargo.toml` — identifies Git-backed workflow checkpoint storage and directly depends on `fabro-store`.
  - `lib/components/fabro-checkpoint/src/lib.rs` — exposes branch, Git, author, trailer, and checkpoint error modules behind one crate facade.
  - `lib/components/fabro-checkpoint/src/branch.rs:BranchStore` and `lib/components/fabro-checkpoint/src/git.rs:Store` — provide the two persistence entry points over the same Git repository state.
- Scoped tracked files: 7

### `fabro-dump` — Run dump materialization

- Purpose: Materialize a stored run projection, event history, checkpoints, artifacts, and referenced blobs into a portable directory tree.
- Globs: `lib/components/fabro-dump/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-dump/src/lib.rs:RunDump`, `lib/components/fabro-dump/src/lib.rs:RunDump::from_store_state_and_events`, `lib/components/fabro-dump/src/lib.rs:RunDump::write_to_dir`
- Owns: dump entry layout and filenames; stage ranking within dumps; blob hydration; dump serialization and directory writing.
- Depends on candidates: `fabro-store`
- Evidence:
  - `lib/components/fabro-dump/Cargo.toml` — gives the crate a direct dependency on `fabro-store`, which supplies projections and event envelopes.
  - `lib/components/fabro-dump/src/lib.rs:RunDump` — contains the public dump-building and writing lifecycle, with inline tests for its output contract.
  - Workspace consumers `fabro-cli` and `fabro-workflow` both depend directly on `fabro-dump`, rather than accessing its behavior through `fabro-store`.
- Scoped tracked files: 2

### `fabro-environment` — Environment definitions and storage

- Purpose: Validate, seed, version, import, and durably store server-owned execution environment definitions.
- Globs: `lib/components/fabro-environment/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-environment/src/lib.rs`, `lib/components/fabro-environment/src/store.rs:EnvironmentStore`, `lib/components/fabro-environment/src/store.rs:seed_default_environment`
- Owns: environment IDs and revisions; environment drafts and canonical revisions; environment SQLite records; built-in environment seeding; legacy directory import.
- Depends on candidates: `[]`
- Evidence:
  - `lib/components/fabro-environment/Cargo.toml` — declares the server-owned environment domain and durable storage.
  - `lib/components/fabro-environment/src/lib.rs` — exports a specific environment domain/store API, including seeding and import operations.
  - `lib/components/fabro-environment/tests/store.rs` — exercises the environment persistence boundary independently of the other resource stores.
- Scoped tracked files: 7

### `fabro-github` — GitHub authentication and API

- Purpose: Resolve GitHub credentials and perform authenticated GitHub App, repository, branch, and pull-request API operations.
- Globs: `lib/components/fabro-github/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-github/src/lib.rs:GitHubCredentials`, `lib/components/fabro-github/src/lib.rs:GitHubContext`, `lib/components/fabro-github/src/lib.rs:create_pull_request`, `lib/components/fabro-github/src/lib.rs:resolve_authenticated_url`
- Owns: GitHub credential forms and token minting; GitHub API request/response translation; repository URL normalization and authenticated clone URLs; pull-request lifecycle calls.
- Depends on candidates: `[]`
- Evidence:
  - `lib/components/fabro-github/Cargo.toml` — describes GitHub App authentication and API helpers and declares the JWT/HTTP dependencies used at this boundary.
  - `lib/components/fabro-github/src/lib.rs` — defines the credential context, testable HTTP abstraction, App token flow, and repository/PR operations in one public surface.
  - `lib/components/fabro-github/tests/integration.rs` and `lib/components/fabro-github/src/testdata/rsa_private.pem` — exercise the external authentication/API boundary using a dedicated test key fixture.
- Scoped tracked files: 4

### `fabro-graphviz` — Workflow graph language

- Purpose: Parse Graphviz DOT into Fabro's typed graph model and parse conditions/stylesheets or render graphs for presentation.
- Globs: `lib/components/fabro-graphviz/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-graphviz/src/lib.rs`, `lib/components/fabro-graphviz/src/parser/mod.rs:parse`, `lib/components/fabro-graphviz/src/condition.rs:parse_condition_expr`, `lib/components/fabro-graphviz/src/render.rs:render_dot`
- Owns: DOT lexer/parser/semantic conversion; graph parsing errors; condition and stylesheet syntax; Graphviz rendering normalization.
- Depends on candidates: `[]`
- Evidence:
  - `lib/components/fabro-graphviz/Cargo.toml` — names the crate as the DOT parser and typed graph data model.
  - `lib/components/fabro-graphviz/src/parser/mod.rs:parse` — is the source-to-typed-graph entry point backed by separate lexer, grammar, AST, and semantic modules.
  - `lib/components/fabro-graphviz/src/lib.rs` — exposes parsing-adjacent condition, fidelity, rendering, and stylesheet interfaces as the graph-language boundary.
- Scoped tracked files: 14

### `fabro-hooks` — Workflow lifecycle hooks

- Purpose: Configure and execute user-defined workflow lifecycle hooks and bridge tool hooks into the agent runtime.
- Globs: `lib/components/fabro-hooks/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-hooks/src/lib.rs`, `lib/components/fabro-hooks/src/runner.rs:HookRunner`, `lib/components/fabro-hooks/src/executor.rs:HookExecutor`, `lib/components/fabro-hooks/src/bridge.rs:WorkflowToolHookCallback`
- Owns: hook configuration and event selection; hook execution context; hook result/decision merging; HTTP/command hook dispatch; agent tool-hook bridging.
- Depends on candidates: `fabro-agent`, `fabro-llm`
- Evidence:
  - `lib/components/fabro-hooks/Cargo.toml` — identifies workflow lifecycle hooks and directly depends on the agent and LLM components used by hook execution.
  - `lib/components/fabro-hooks/src/lib.rs` — exposes hook definitions, decisions, runner, execution context, and the agent bridge.
  - `lib/components/fabro-hooks/tests/host_command_hooks.rs` — tests host-command hooks through the public lifecycle boundary.
- Scoped tracked files: 8

### `fabro-install` — Installation persistence

- Purpose: Prepare, persist, and roll back shared CLI/server installation settings, credentials, development tokens, and default environments.
- Globs: `lib/components/fabro-install/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-install/src/lib.rs:InstallPersistencePlan`, `lib/components/fabro-install/src/lib.rs:persist_install_outputs_direct`, `lib/components/fabro-install/src/lib.rs:merge_server_settings`
- Owns: install persistence plans; settings and server-env mutations; vault writes/removals; development-token creation and rollback; default environment seeding during install.
- Depends on candidates: `fabro-environment`
- Evidence:
  - `lib/components/fabro-install/Cargo.toml` — describes shared install primitives for CLI and server flows and directly depends on the environment store.
  - `lib/components/fabro-install/src/lib.rs:InstallPersistencePlan` — groups the files, env entries, token, and vault state committed by one install operation.
  - Workspace consumers `fabro-cli` and `fabro-server` depend directly on this crate, making it a shared install boundary rather than CLI-local code.
- Scoped tracked files: 2

### `fabro-interview` — Human interaction runtime

- Purpose: Represent workflow questions and answers and provide console, callback, queue, control, recording, replay, and automatic interviewer implementations.
- Globs: `lib/components/fabro-interview/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-interview/src/lib.rs:Interviewer`, `lib/components/fabro-interview/src/lib.rs:ask_with_timeout`, `lib/components/fabro-interview/src/control.rs:ControlInterviewer`
- Owns: question/answer protocol; interviewer request lifetime and timeout behavior; queued and controlled answer delivery; interview recording and replay.
- Depends on candidates: `[]`
- Evidence:
  - `lib/components/fabro-interview/Cargo.toml` — defines human-in-the-loop interviewer traits and implementations as the crate purpose.
  - `lib/components/fabro-interview/src/lib.rs:Interviewer` — is the shared async interaction interface and re-exports all implementation strategies.
  - `lib/components/fabro-interview/src/control_protocol.rs` and `lib/components/fabro-interview/src/control.rs` — own the worker-control delivery protocol and pending interaction state.
- Scoped tracked files: 10

### `fabro-llm` — Unified LLM client

- Purpose: Provide a provider-neutral generation API with model routing, middleware, retries, token/cost accounting, provider adapters, and wire codecs.
- Globs: `lib/components/fabro-llm/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-llm/src/lib.rs`, `lib/components/fabro-llm/src/client.rs:Client`, `lib/components/fabro-llm/src/provider.rs:ProviderAdapter`, `lib/components/fabro-llm/src/generate.rs:generate`, `lib/components/fabro-llm/src/generate.rs:stream`
- Owns: normalized LLM request/response/stream types; provider adapter registry; provider-specific authentication and transport; request/response/stream wire translation; retry/middleware/generation orchestration; token and cost calculations.
- Depends on candidates: `[]`
- Evidence:
  - `lib/components/fabro-llm/Cargo.toml` — describes a unified multi-provider client and does not depend on another component crate.
  - `lib/components/fabro-llm/src/provider.rs:ProviderAdapter` and `lib/components/fabro-llm/src/client.rs:Client` — define the adapter contract and client registry through which the provider modules are consumed.
  - `lib/components/fabro-llm/tests/it/wire/mod.rs` and its provider-specific snapshot trees — verify that the codecs and adapters implement the same normalized client boundary.
- Scoped tracked files: 188

### `fabro-manifest` — Run manifest construction

- Purpose: Resolve workflow/configuration inputs, collect static dependencies, and construct a self-contained run manifest with Git provenance.
- Globs: `lib/components/fabro-manifest/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-manifest/src/lib.rs:build_run_manifest`, `lib/components/fabro-manifest/src/lib.rs:build_run_overrides`, `lib/components/fabro-manifest/src/lib.rs:ManifestBuildInput`
- Owns: manifest build input/output; configuration-layer resolution for manifest creation; workflow/file dependency collection; Git context and pre-run push preparation.
- Depends on candidates: `fabro-github`, `fabro-graphviz`, `fabro-workflow`
- Evidence:
  - `lib/components/fabro-manifest/Cargo.toml` — declares run manifest construction and direct dependencies on graph parsing, GitHub support, and selected workflow utilities.
  - `lib/components/fabro-manifest/src/lib.rs:build_run_manifest` — is a single public assembly operation that produces the API `RunManifest`.
  - Workspace consumers `fabro-cli`, `fabro-server`, and `fabro-mcp-server` depend directly on the crate to share identical manifest construction.
- Scoped tracked files: 2

### `fabro-mcp` — MCP client runtime

- Purpose: Connect to configured Model Context Protocol servers, manage their connection lifetimes, discover tools, and dispatch qualified tool calls.
- Globs: `lib/components/fabro-mcp/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-mcp/src/lib.rs`, `lib/components/fabro-mcp/src/client.rs:McpClient`, `lib/components/fabro-mcp/src/connection_manager.rs:McpConnectionManager`
- Owns: MCP client connections; stdio and streaming HTTP transport selection; server connection manager state; tool discovery, qualified names, and call-result conversion.
- Depends on candidates: `[]`
- Evidence:
  - `lib/components/fabro-mcp/Cargo.toml` — describes the MCP client role and enables the rmcp client/transport features required by it.
  - `lib/components/fabro-mcp/src/lib.rs` — exposes client, config, connection manager, and HTTP transport modules while keeping protocol handlers internal.
  - `lib/components/fabro-mcp/tests/stdio_integration.rs` — verifies the external MCP process boundary over stdio.
- Scoped tracked files: 10

### `fabro-mcp-store` — MCP server catalog storage

- Purpose: Durably store, revision, cache, and import server-managed MCP server definitions.
- Globs: `lib/components/fabro-mcp-store/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-mcp-store/src/lib.rs`, `lib/components/fabro-mcp-store/src/store.rs:McpServerStore`, `lib/components/fabro-mcp-store/src/store.rs:import_legacy_directory_once`
- Owns: MCP server definition SQLite records; definition revisions and optimistic concurrency; synchronous catalog cache; legacy directory import.
- Depends on candidates: `[]`
- Evidence:
  - `lib/components/fabro-mcp-store/Cargo.toml` — declares server-managed MCP catalog durable storage.
  - `lib/components/fabro-mcp-store/src/lib.rs` — explicitly states that the domain model is shared but this crate owns persistence, and exports only the store/error/import API.
  - `lib/components/fabro-mcp-store/tests/store.rs` — exercises that persistence boundary independently from live MCP connections.
- Scoped tracked files: 6

### `fabro-sandbox` — Execution sandbox abstraction

- Purpose: Define the execution sandbox and provider contracts and implement local, Docker, and Daytona sandbox lifecycles.
- Globs: `lib/components/fabro-sandbox/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-sandbox/src/lib.rs`, `lib/components/fabro-sandbox/src/sandbox.rs:Sandbox`, `lib/components/fabro-sandbox/src/provider.rs:SandboxProvider`, `lib/components/fabro-sandbox/src/provider.rs:SandboxProviderRegistry`
- Owns: sandbox filesystem/process/terminal interface; provider creation, lookup, and removal lifecycle; local/Docker/Daytona implementations; clone-source setup and reconnect behavior; sandbox errors and redaction.
- Depends on candidates: `fabro-github`
- Evidence:
  - `lib/components/fabro-sandbox/Cargo.toml` — defines provider features (`local`, `docker`, `daytona`) around the common sandbox crate and makes GitHub support optional for clone-based providers.
  - `lib/components/fabro-sandbox/src/sandbox.rs:Sandbox` and `lib/components/fabro-sandbox/src/provider.rs:SandboxProvider` — separate per-sandbox operations from provider lifecycle management within one public boundary.
  - `lib/components/fabro-sandbox/tests/docker_streaming.rs` and `lib/components/fabro-sandbox/tests/daytona_streaming_live.rs` — exercise provider implementations against the shared contract.
- Scoped tracked files: 23

### `fabro-slack` — Slack interaction integration

- Purpose: Connect to Slack Socket Mode and translate workflow questions, answers, run lifecycle events, and thread replies between Slack and Fabro.
- Globs: `lib/components/fabro-slack/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-slack/src/connection.rs:run`, `lib/components/fabro-slack/src/client.rs:SlackClient`, `lib/components/fabro-slack/src/blocks.rs:question_to_blocks`
- Owns: Slack credential resolution; Socket Mode connection/event loop; Slack API client; block rendering; interaction payload parsing; run-to-thread registry and dispatch.
- Depends on candidates: `fabro-interview`, `fabro-workflow`
- Evidence:
  - `lib/components/fabro-slack/Cargo.toml` — declares the Slack interviewer integration and directly depends on the interview and workflow components.
  - `lib/components/fabro-slack/src/connection.rs:run` — owns the Socket Mode connection lifetime and dispatch loop.
  - `lib/components/fabro-slack/src/interaction.rs` and `lib/components/fabro-slack/src/threads.rs` — translate external payloads into interview submissions and associate Slack threads with run state.
- Scoped tracked files: 11

### `fabro-store` — Run and authentication persistence

- Purpose: Persist run event streams, projections, blobs, artifacts, summaries, catalog indexes, and server authentication grants over SlateDB, object storage, and SQLite.
- Globs: `lib/components/fabro-store/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-store/src/lib.rs`, `lib/components/fabro-store/src/slate/mod.rs:Database`, `lib/components/fabro-store/src/slate/run_store.rs:RunDatabase`, `lib/components/fabro-store/src/run_state.rs:RunProjectionReducer`
- Owns: run event append/read lifecycle; run projection reduction and caching; run/blob/artifact key layout; run catalog and summary indexes; authorization-code and refresh-token records; storage-specific errors and locking.
- Depends on candidates: `[]`
- Evidence:
  - `lib/components/fabro-store/src/lib.rs` — presents one persistence facade for events, projections, artifacts, summaries, blobs, and auth records.
  - `lib/components/fabro-store/src/slate/mod.rs:Database` — is the shared storage root from which run, blob, catalog, auth-code, and refresh-token stores are obtained.
  - `lib/components/fabro-store/tests/serializable_projection.rs` — tests the durable projection representation at the crate boundary.
- Scoped tracked files: 25

### `fabro-tool` — Run-control tools

- Purpose: Define and execute the shared run create, search, get, event, gather, interaction, and pairing tools over an abstract Fabro backend.
- Globs: `lib/components/fabro-tool/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-tool/src/lib.rs`, `lib/components/fabro-tool/src/common.rs:FabroToolBackend`, `lib/components/fabro-tool/src/common.rs:tool_definitions`, `lib/components/fabro-tool/src/create.rs:create_runs`
- Owns: tool names, JSON schemas, and parameter validation; backend-neutral run-control operations; result DTOs and text rendering; API-client backend adapter.
- Depends on candidates: `[]`
- Evidence:
  - `lib/components/fabro-tool/Cargo.toml` — identifies shared run-control tool behavior and depends on foundation API/client contracts rather than the server or workflow implementation.
  - `lib/components/fabro-tool/src/common.rs:FabroToolBackend` — is the abstraction shared by CLI, server, workflow, and MCP-server consumers.
  - `lib/components/fabro-tool/src/lib.rs` — exports a matched set of validated operation/result/text interfaces for all supported tools.
- Scoped tracked files: 12

### `fabro-tracker` — Issue tracker adapters

- Purpose: Provide a common issue-tracker interface with GitHub Projects and Linear implementations.
- Globs: `lib/components/fabro-tracker/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-tracker/src/lib.rs:Tracker`, `lib/components/fabro-tracker/src/github.rs:GitHubTracker`, `lib/components/fabro-tracker/src/linear.rs:LinearTracker`
- Owns: normalized issue and blocker records; candidate-issue query and state-transition contract; GitHub Projects GraphQL adapter; Linear GraphQL adapter.
- Depends on candidates: `fabro-github`
- Evidence:
  - `lib/components/fabro-tracker/Cargo.toml` — declares the tracker trait/types boundary and directly depends on GitHub support for one adapter.
  - `lib/components/fabro-tracker/src/lib.rs:Tracker` — defines a provider-neutral async issue workflow implemented by both provider modules.
  - `lib/components/fabro-tracker/src/fixtures/github-app-test-key.pem` — is a test fixture owned by the GitHub tracker adapter, not a runtime credential or vendored file.
- Scoped tracked files: 5

### `fabro-validate` — Workflow graph validation

- Purpose: Run built-in and catalog-aware lint rules over typed Fabro workflow graphs and return structured diagnostics.
- Globs: `lib/components/fabro-validate/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-validate/src/lib.rs:validate`, `lib/components/fabro-validate/src/lib.rs:validate_with_catalog`, `lib/components/fabro-validate/src/lib.rs:LintRule`, `lib/components/fabro-validate/src/rules/mod.rs:built_in_rules`
- Owns: validation severity and diagnostic structure; lint-rule interface and built-in rule registry; graph/catalog validation traversal; validation error escalation.
- Depends on candidates: `fabro-acp`, `fabro-graphviz`
- Evidence:
  - `lib/components/fabro-validate/Cargo.toml` — declares graph validation/linting and directly depends on graph parsing plus ACP backend validation.
  - `lib/components/fabro-validate/src/lib.rs:LintRule` — provides the extension interface and public diagnostic API.
  - `lib/components/fabro-validate/src/rules/mod.rs:built_in_rules` and the 31 rule source files — form an explicit registry of independently tested rules under one validation lifecycle.
- Scoped tracked files: 36

### `fabro-variable` — Workflow variable storage

- Purpose: Validate, durably store, snapshot, and import workflow-visible non-sensitive variables.
- Globs: `lib/components/fabro-variable/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-variable/src/lib.rs:VariableStore`, `lib/components/fabro-variable/src/lib.rs:VariableStore::value_map`, `lib/components/fabro-variable/src/lib.rs:import_legacy_json_once`
- Owns: variable name validation; variable SQLite records and timestamps; name-to-value snapshots for template contexts; legacy JSON import and backup.
- Depends on candidates: `[]`
- Evidence:
  - `lib/components/fabro-variable/Cargo.toml` — defines workflow-visible, non-sensitive variables as a separate storage concern.
  - `lib/components/fabro-variable/src/lib.rs:VariableStore` — exposes CRUD and render-context snapshot operations over that single domain.
  - `lib/components/fabro-variable/tests/store.rs` — verifies its persistence/import contract independently from environments, automations, and MCP definitions.
- Scoped tracked files: 3

### `fabro-workflow` — Workflow orchestration engine

- Purpose: Transform, validate, initialize, execute, persist, resume, and finalize graph-defined Fabro runs across handlers, lifecycle hooks, sandboxes, checkpoints, events, and human controls.
- Globs: `lib/components/fabro-workflow/**`
- Exclude globs: `[]`
- Entry points: `lib/components/fabro-workflow/src/operations/mod.rs`, `lib/components/fabro-workflow/src/operations/start.rs:start`, `lib/components/fabro-workflow/src/pipeline/mod.rs`, `lib/components/fabro-workflow/src/pipeline/execute.rs:execute`, `lib/components/fabro-workflow/src/handler/mod.rs:Handler`
- Owns: run operation lifecycle (create/start/resume/retry/rewind/fork/archive); workflow transform/validate/initialize/execute/finalize phases; node handler registry and built-in handlers; run-scoped services and cancellation; workflow event conversion/emission; checkpoint, Git, artifact, hook, and status lifecycles; steering and run control.
- Depends on candidates: `fabro-acp`, `fabro-agent`, `fabro-checkpoint`, `fabro-dump`, `fabro-github`, `fabro-graphviz`, `fabro-hooks`, `fabro-interview`, `fabro-llm`, `fabro-mcp`, `fabro-sandbox`, `fabro-store`, `fabro-tool`, `fabro-validate`
- Evidence:
  - `lib/components/fabro-workflow/Cargo.toml` — defines the DOT-based workflow runner and declares the component dependencies used to assemble the engine.
  - `lib/components/fabro-workflow/src/pipeline/mod.rs` — exposes the ordered parse/transform/validate/initialize/execute/finalize phase boundary and its typed phase states.
  - `lib/components/fabro-workflow/src/handler/mod.rs:Handler` and `lib/components/fabro-workflow/src/lifecycle/mod.rs:WorkflowLifecycle` — connect node execution to the run-scoped lifecycle under the same engine.
  - `lib/components/fabro-workflow/tests/it/main.rs` and `lib/components/fabro-workflow/tests/materialize_run.rs` — exercise end-to-end orchestration and run materialization.
- Scoped tracked files: 122

## Coverage

The fixed-revision inventory was computed with:

```text
git ls-tree -r --name-only 2bcf94fed8a9b429f18d9196fa824711d6f4cb0a -- lib/components
```

| Component glob | Assigned tracked files |
| --- | ---: |
| `lib/components/fabro-acp/**` | 8 |
| `lib/components/fabro-agent/**` | 66 |
| `lib/components/fabro-automation/**` | 9 |
| `lib/components/fabro-checkpoint/**` | 7 |
| `lib/components/fabro-dump/**` | 2 |
| `lib/components/fabro-environment/**` | 7 |
| `lib/components/fabro-github/**` | 4 |
| `lib/components/fabro-graphviz/**` | 14 |
| `lib/components/fabro-hooks/**` | 8 |
| `lib/components/fabro-install/**` | 2 |
| `lib/components/fabro-interview/**` | 10 |
| `lib/components/fabro-llm/**` | 188 |
| `lib/components/fabro-manifest/**` | 2 |
| `lib/components/fabro-mcp/**` | 10 |
| `lib/components/fabro-mcp-store/**` | 6 |
| `lib/components/fabro-sandbox/**` | 23 |
| `lib/components/fabro-slack/**` | 11 |
| `lib/components/fabro-store/**` | 25 |
| `lib/components/fabro-tool/**` | 12 |
| `lib/components/fabro-tracker/**` | 5 |
| `lib/components/fabro-validate/**` | 36 |
| `lib/components/fabro-variable/**` | 3 |
| `lib/components/fabro-workflow/**` | 122 |
| **Total** | **580** |

- Relevant tracked files: 580
- Assigned files: 580
- Excluded files: 0
- Unmapped files: 0
- Duplicate claims: 0 (the proposed crate-directory globs are disjoint)

## Exclusions and unmapped files

- Evidence-backed exclusions: none.
- Unmapped files: none.
- Checked-in `.snap`, `.j2`, `.lark`, migration, README, and test-key files remain assigned because they specify or exercise component behavior.

## Boundary questions for reconciliation

1. Should `fabro-workflow` remain one engine component, as proposed, or be split into a public run-operations/materialization component and an execution component? `src/operations/**` and `src/pipeline/**` expose recognizable facades, but `services.rs`, `event.rs`, `runtime_store.rs`, the root modules, and lifecycle/handler code tie both facades to the same run-scoped state and make a non-overlapping ownership split less clear.
2. Should `fabro-llm` remain one unified client component, as proposed, or should `src/providers/**`, `src/codec/**`, and `tests/it/wire/**` form a provider-protocol-adapters component? The adapter trait and wire-focused tests support that sub-boundary, while `adapter_registry.rs`, shared normalized types, transport helpers, and direct module references keep it inside one crate-level client lifecycle.
3. Should `fabro-store` remain one persistence component, as proposed, or should its authorization-code/refresh-token stores be separated from run/event/blob persistence? `slate::Database` exposes them from one storage root, but their record lifecycles are consumed by server authentication rather than workflow execution.
