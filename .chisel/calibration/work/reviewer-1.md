# Calibration review — reviewer 1

Revision reviewed: `6bb6b5efcc0e36b52e3c097f532d9f2c00914c6c`

Scope: `fabro-workflow`, `fabro-http`, `fabro-web-app`, and `repository-ci` as routed by `.chisel/cartography/codebase-map.md`. I excluded `apps/fabro-web/app/components/playground/**` from `fabro-web-app`, and limited `repository-ci` to `.github/workflows/rust.yml`, `.github/workflows/typescript.yml`, and `.github/zizmor.yml`. The routed paths have no changes between the map revision and the reviewed revision.

## Provisional ratings

| Component | Ownership boundaries | Simplicity | Domain model | Duplication of knowledge |
| --- | --- | --- | --- | --- |
| `fabro-workflow` | **2 — High** | **2 — High** | **2 — High** | **2 — High** |
| `fabro-http` | **4 — High** | **4 — High** | **4 — High** | **3 — High** |
| `fabro-web-app` | **4 — High** | **2 — High** | **2 — High** | **2 — High** |
| `repository-ci` | **4 — High** | **3 — High** | **2 — High** | **2 — High** |

## `fabro-workflow`

### Ownership boundaries — 2, High confidence

The component has a clear top-level phase boundary: `pipeline/mod.rs` orders parse, transform, validate, initialize, execute, finalize, and pull-request processing; `pipeline/types.rs` gives those phases distinct result types. `pipeline/execute.rs:execute`, `graph.rs:WorkflowGraph`, and `node_handler.rs:WorkflowNodeHandler` also make the boundary with the generic `fabro-core` executor explicit. `lifecycle/mod.rs:WorkflowLifecycle` composes named lifecycle owners instead of placing every callback in the executor.

The pressure appears in terminal-run ownership. The normal path is owned by `pipeline/finalize.rs:finalize` and `pipeline/finalize.rs:build_terminal_event`, while engine/bootstrap failures are handled by `operations/start.rs:emit_workflow_run_failed`, `operations/start.rs:persist_terminal_engine_failure`, and the completion/drop guards in `operations/start.rs`. Retry and archive operations also synthesize terminal events in `operations/retry.rs` and `operations/archive.rs`. These paths are understandable individually, but terminal state, persistence, and event emission do not have one stable lifecycle home.

A representative routine change is adding terminal metadata that must be present for every failed or concluded run. It would require checking or changing `pipeline/finalize.rs:build_terminal_event`, `pipeline/finalize.rs:finalize`, `operations/start.rs:emit_workflow_run_failed`, `operations/start.rs:persist_terminal_engine_failure`, the start-operation guards, and the corresponding terminal paths in `operations/retry.rs` and `operations/archive.rs`.

Strongest counterevidence: the main successful-run path is explicit and strongly partitioned, and `WorkflowLifecycle` plus `RunServices` give many responsibilities named owners.

Why adjacent scores do not fit: 3 understates the issue because terminal completion is a central lifecycle concern, not an edge-only exception; an ordinary terminal-contract change must inspect several authorities. 1 does not fit because the normal path and the exceptional paths are still traceable and deliberately named.

### Simplicity — 2, High confidence

The top-level flow is readable, but routine run startup crosses a large amount of central wiring. `operations/start.rs:start` enters `execute_persisted_run`, constructs `RunSession`, and then `RunSession::run` coordinates logging, SHA listeners, initialization, cleanup/drain guards, execution, finalization, and pull-request handling. `pipeline/types.rs:InitOptions` carries a large set of run inputs, and `operations/start.rs:RunSession::run` assembles them before handing control to `pipeline/initialize.rs`. The resulting services are then repartitioned through `services.rs:RunServices`, `services.rs:EngineServices`, and `pipeline/execute.rs:execute`.

A representative routine change is adding a run-scoped service needed by node handlers. It would pass through `operations/start.rs:StartServices` or `RunSession`, `pipeline/types.rs:InitOptions`, `pipeline/initialize.rs:initialize`, `pipeline/types.rs:Initialized`, `services.rs:RunServices`, `services.rs:EngineServices`, and the destructuring/building in `pipeline/execute.rs:execute`.

Strongest counterevidence: the phase result types in `pipeline/types.rs` and the extracted executor/lifecycle adapters make the long path navigable; the complexity is structured rather than accidental.

Why adjacent scores do not fit: 3 does not fit because the pressure is on the common startup and execution path, and a small run-scoped dependency change propagates through several central handoff types. 1 does not fit because the ordered pipeline and named handoffs still provide a stable path through the component.

### Domain model — 2, High confidence

The strongest positive mechanism is the phase model in `pipeline/types.rs`: `Parsed`, `Transformed`, `Validated`, `Persisted`, `Initialized`, `Executed`, `Concluded`, and `Finalized` constrain which data exists at each stage. Canonical run records are reused from `fabro-types`, and `services.rs:RunServices` documents cancellation ownership.

However, the core event path weakens those guarantees. `event/events.rs:Event::StageCompleted` carries `status: String`; lifecycle code such as `lifecycle/event.rs` converts `StageOutcome` to a string, and `event/convert.rs:stage_status_from_string` parses it back when creating the durable event. An unknown value is not rejected: it is warned about and converted to `StageOutcome::Failed`. The durable model in `fabro-types` is typed, but the internal central event model permits invalid status values and gives them a lossy fallback meaning. `WorkflowRunCompleted` similarly carries a string status internally.

Strongest counterevidence: the durable event body and most run/pipeline records use named enums and phase-specific types, so this is not a component with generally unmodeled state.

Why adjacent scores do not fit: 3 does not fit because stage and run outcomes are central workflow vocabulary used on every execution, and the internal-to-durable boundary permits and silently reinterprets invalid values. 1 does not fit because canonical typed outcomes exist and dominate downstream storage; the break is concentrated at the internal event boundary.

### Duplication of knowledge — 2, High confidence

Adding an event requires coordinated knowledge in several central authorities. The internal variant lives in `event/events.rs:Event`; its wire name is separately selected by `event/names.rs:event_name`; durable fields are declared in `fabro-types::EventBody`; conversion is implemented in `event/convert.rs:event_body_from_event`; stored-field behavior is selected in `event/stored_fields.rs:stored_event_fields_for_variant`; and tracing behavior is implemented on `Event`. `docs/internal/events-strategy.md` documents this multi-site procedure, confirming that this is the expected recurring event-evolution path rather than a one-off remnant.

A representative routine change is adding a persisted workflow event. It touches `event/events.rs:Event`, `event/names.rs:event_name`, the `Event` tracing method, `fabro_types::EventBody`, `event/convert.rs:event_body_from_event`, `event/stored_fields.rs:stored_event_fields_for_variant`, emitters, and any event consumers.

Strongest counterevidence: `event/emitter.rs:Emitter::emit_with_scope` constructs the canonical run event once before dispatch, exhaustive matches make omissions visible to the compiler, and the strategy document gives maintainers one checklist.

Why adjacent scores do not fit: 3 does not fit because event evolution is frequent, central workflow work and requires synchronized changes across representations and crates. 1 does not fit because each representation has a stated role and there is a single canonicalization point before dispatch.

Lens-boundary note: the internal `Event`/durable `EventBody` split could be described as a domain-model issue or duplication. I treated the repeated declarations and conversion sites as duplication of knowledge; the separate `String`-to-`StageOutcome` loss of meaning is the domain-model issue. Likewise, repeated terminal constructors are secondary duplication, but I classified the primary problem as ownership because the key question is which operation owns terminal lifecycle completion.

## `fabro-http`

### Ownership boundaries — 4, High confidence

`lib/foundation/fabro-http/src/lib.rs` is a small, focused owner for HTTP client construction and proxy policy. Callers get approved async or blocking builders and convenience clients from this crate. Repository lint policy in `clippy.toml` disallows direct `reqwest` constructors and points callers to `fabro-http`, so the boundary is reinforced rather than merely conventional. `ProxyPolicy::resolve` also owns the environment-variable authority through `fabro_static::EnvVars::FABRO_HTTP_PROXY_POLICY`.

Strongest counterevidence: the crate deliberately re-exports several `reqwest` types and carries lint exceptions for those facade exports, so callers are not isolated from every transport detail.

Why adjacent scores do not fit: 3 does not fit because construction policy, environment precedence, test defaults, and transport facade all have one enforced home with no observed competing builder authority.

### Simplicity — 4, High confidence

The common path is short: choose `HttpClientBuilder` or `BlockingHttpClientBuilder`, optionally configure it, resolve `ProxyPolicy`, and build the underlying client. `define_builder!` generates the shared async/blocking surface once, while the async-only `read_timeout` extension remains plainly visible next to the macro invocation. Convenience functions such as `http_client`, `blocking_http_client`, `test_http_client`, and `blocking_test_http_client` expose the common cases directly.

Strongest counterevidence: macro generation means the two concrete builder implementations are not visible as ordinary source, and async-only options must be added outside the shared definition.

Why adjacent scores do not fit: 3 does not fit because the macro removes rather than creates routine common-option work: a shared builder option is added in one readable location, while the generated types remain thin wrappers.

### Domain model — 4, High confidence

`ProxyPolicy` names the only supported policies, `ProxyPolicy::parse` rejects unknown values, and `ProxyPolicy::resolve_with_env_value` makes precedence explicit: a caller override wins, then the environment value, then the system default. Test helpers force `Disabled`, making local test semantics deliberate. `HttpClientBuildError` distinguishes policy configuration failure from transport construction failure.

Strongest counterevidence: callers can express no-proxy behavior through both `proxy_policy(ProxyPolicy::Disabled)` and the lower-level `no_proxy()` builder method, and the facade re-exports lower-level proxy types.

Why adjacent scores do not fit: 3 does not fit because the overlapping entry points do not introduce an ambiguous stored state or silent fallback: the policy values and their precedence are explicit, and invalid environment vocabulary fails closed.

### Duplication of knowledge — 3, High confidence

The builder macro is a strong anti-duplication mechanism for async and blocking clients. The remaining policy vocabulary is manually repeated: `ProxyPolicy` variants, `ProxyPolicy::parse`, the expected-value text in `HttpClientBuildError::InvalidProxyPolicy`, and the policy match in the generated `build` method must agree.

A representative routine change is adding another supported proxy policy. It would touch `ProxyPolicy`, `ProxyPolicy::parse`, the expected-value message on `HttpClientBuildError::InvalidProxyPolicy`, the `define_builder!` build-time match, and policy tests in the same source file.

Strongest counterevidence: every repeated policy decision is co-located in one small file, and the exhaustive build match makes a missing behavioral branch a compile error.

Why adjacent scores do not fit: 4 does not fit because the accepted vocabulary and error vocabulary are independently maintained strings. 2 does not fit because the synchronization is confined to one authority and does not force routine callers or neighboring components to change.

Lens-boundary note: macro use could be counted as simplicity indirection, but its primary effect here is eliminating async/blocking duplication. The generated control flow is small enough that I did not lower simplicity for it.

## `fabro-web-app`

### Ownership boundaries — 4, High confidence

The app has explicit composition points. `app/entry.tsx` selects normal or install mode and installs shared providers; `app/router.tsx` and `app/install-router.tsx` own the two route trees. `app/lib/api-client.ts` owns generated-client construction and uniform API errors, `app/lib/query-keys.ts` owns cache keys, and `app/lib/queries.ts` owns shared reads. The React effects policy is embodied by approved wrappers in `app/hooks/effects.ts`; direct effect usage is concentrated in hooks and live-event libraries rather than route/component bodies. `scripts/build.ts` separately owns deterministic asset building and atomic publication.

Strongest counterevidence: some cache mutation and API-write coordination remains in route handlers, particularly in the large run and installation screens, so not every server interaction passes through a single application-service layer.

Why adjacent scores do not fit: 3 does not fit because routing, reads, client configuration, effects, and build publication each have a visible and consistently used owner; route-local writes are appropriate UI orchestration rather than a competing global authority.

### Simplicity — 2, High confidence

The normal routing shell is simple, but two central screens concentrate substantial policy and presentation. `app/routes/run-stages.tsx` combines event-to-turn reduction, event filtering, grouping, stage/activity interpretation, row and panel rendering, stage renderer selection, and the route page. `app/install-app.tsx` similarly combines installation state transitions, controller behavior, forms, and view composition. Cross-tab stream coordination in `app/lib/cross-tab-sse.ts` is another large central mechanism.

A representative routine change is showing a new kind of stage activity in the run timeline. It requires following `app/lib/run-events.ts:STAGE_ACTIVITY_EVENT_TYPES`, `app/routes/run-stages.tsx:STAGE_ACTIVITY_EVENT_SET`, `app/routes/run-stages.tsx:buildStageActivity`, the route's turn/activity types, and the corresponding render helpers in the same large route module.

Strongest counterevidence: shared event lists, query keys, generated API types, and route helpers provide landmarks, and the activity reducer is deterministic rather than dispersed among many components.

Why adjacent scores do not fit: 3 does not fit because run-stage interpretation is a common product path and small presentation changes require navigating large modules that mix reduction and rendering concerns. 1 does not fit because the route and install flows remain typed, testable, and traceable from explicit entry points.

### Domain model — 2, High confidence

Generated API types provide a strong canonical model for ordinary request/response queries, and several local models use discriminated unions. The live-event boundary is weaker. `app/lib/sse.ts:EventPayload` permits an optional event name plus arbitrary fields. `app/lib/run-events.ts:RunEventPayload` and `app/lib/live-events.ts:LiveEventPayload` repeat mostly optional envelope fields with `properties: unknown`. `app/lib/sse.ts:subscribeToSharedEventSource` parses JSON and casts it to the requested payload type without runtime validation. Common live UI behavior therefore accepts payloads that lack the fields implied by their event names.

There is additional vocabulary translation in `app/data/runs.ts:RunStatus`, which locally reproduces API run-state kinds and adds presentation state, and compatibility shape probing in `app/lib/run-sandbox-lifecycle.ts:sandboxLifecycleKind` and `sandboxInstance`.

Strongest counterevidence: generated types remain the authority for normal API calls, `session-stream.ts` and query paths use generated event-envelope types where possible, and the local run status adds a genuine presentation concept rather than merely renaming every API state.

Why adjacent scores do not fit: 3 does not fit because SSE drives common live run behavior and its central payload model makes invalid event/field combinations representable and unchecked. 1 does not fit because static generated models are sound and the weak representation is concentrated at live and compatibility boundaries.

### Duplication of knowledge — 2, High confidence

Live refresh policy is repeated in separate manually curated authorities. `app/lib/run-events.ts:RUN_SUMMARY_EVENTS` lists events that invalidate run summaries, while `app/lib/board-events.ts:BOARD_STATUS_EVENTS` independently lists many of the same run, interview, and pull-request lifecycle events for board refresh. The duplicated payload interfaces in `run-events.ts` and `live-events.ts` add another synchronization surface.

A representative routine change is adding a lifecycle event that changes both a run summary and its board status. It requires updating `app/lib/run-events.ts:RUN_SUMMARY_EVENTS` and `app/lib/board-events.ts:BOARD_STATUS_EVENTS`, then checking phase derivation in `app/lib/run-phases.ts:deriveRunPhases` and live consumers if the event also changes the visible run phase.

Strongest counterevidence: stage activity vocabulary is centralized in `app/lib/run-events.ts:STAGE_ACTIVITY_EVENT_TYPES` and imported by the run-stages route; query keys and server contract types are also centralized or generated.

Why adjacent scores do not fit: 3 does not fit because the repeated invalidation lists govern common live behavior, and a missing update produces stale UI rather than a compile-time failure. 1 does not fit because each list has a clear local purpose and several other high-change vocabularies already have a single authority.

Lens-boundary note: the repeated loose live-event interfaces are both duplicate declarations and a weak model. I treated representable invalid payloads and unchecked casts as the domain-model finding; I used independently maintained event-invalidation sets as the primary duplication finding. The size of `run-stages.tsx` is primarily simplicity pressure, not evidence that its route ownership is unclear.

## `repository-ci`

### Ownership boundaries — 4, High confidence

`.github/workflows/rust.yml` and `.github/workflows/typescript.yml` have an explicit language split and named jobs for formatting, linting, generated documentation, tests, type checking, and builds. Each workflow sets narrow permissions, concurrency behavior is visible, and toolchain/action versions are pinned. The TypeScript build job's Rust build step has a clear purpose: verify the embedded production SPA through the repository's actual build command.

Strongest counterevidence: the Rust clippy job contains a repository-specific legacy-auth `git grep` policy check, rather than delegating that policy to a named script or dedicated job.

Why adjacent scores do not fit: 3 does not fit because the special check is still plainly owned by repository validation, while language-level checks, permissions, and production build validation have unambiguous homes and no competing workflow was observed.

### Simplicity — 3, High confidence

The workflows are short and linear, with direct commands corresponding to local development commands. Friction is isolated: setup steps are repeated across jobs, the clippy job embeds a multi-pattern shell assertion for legacy auth identity removal, and the ignored twin E2E selection is encoded directly in a long `nextest` expression. These cost attention but do not obscure the overall validation flow.

A representative routine change is adding a new TypeScript validation job. It would repeat the checkout, Bun setup, and dependency-install sequence already present in `.github/workflows/typescript.yml:jobs.typecheck`, `jobs.test`, and `jobs.build`, then add the new command.

Strongest counterevidence: each job can be understood independently, commands are explicit, and there is no multi-layer reusable-workflow indirection.

Why adjacent scores do not fit: 4 does not fit because repeated setup and inline special policies add avoidable local friction. 2 does not fit because ordinary check changes still have a direct path through one small workflow and do not cross a complex control structure.

### Domain model — 2, High confidence

Some configuration identifiers no longer denote repository reality. Both push and pull-request triggers in `.github/workflows/rust.yml` refer to `openapi/**`, but that path does not exist; the actual API contract is `docs/public/api-reference/fabro-api.yaml`, which the same workflow's legacy-auth check names directly. `.github/workflows/typescript.yml` also omits that contract path even though the TypeScript API client is generated from it. A contract-only change can therefore fall outside the configured validation vocabulary.

`.github/zizmor.yml:rules.stale-action-refs.ignore` identifies three exceptions by `rust.yml` source line. History shows those locations originally denoted Rust toolchain actions, while the current line numbers point elsewhere after workflow edits. The exception's identity is coupled to incidental layout rather than the action it is meant to describe.

Strongest counterevidence: jobs, test modes, toolchain versions, permissions, and build profiles are otherwise named explicitly and line up with repository commands.

Why adjacent scores do not fit: 3 does not fit because the stale/nonexistent identifiers affect whether central source-of-truth changes are validated and whether static-validation exceptions retain their intended meaning. 1 does not fit because most CI vocabulary remains stable and the affected values can be corrected from clear repository authorities.

### Duplication of knowledge — 2, High confidence

Trigger-path knowledge is repeated in every workflow and twice within each workflow: `.github/workflows/rust.yml:on.push.paths` duplicates `on.pull_request.paths`, and `.github/workflows/typescript.yml` does the same. Cross-language contract inputs then require synchronized edits in both files. The stale `openapi/**` entry and omission of `docs/public/api-reference/fabro-api.yaml` are direct evidence that this repeated knowledge has drifted.

A representative routine change is moving or adding a source-of-truth file that must trigger all relevant CI. It requires updating `rust.yml:on.push.paths`, `rust.yml:on.pull_request.paths`, `typescript.yml:on.push.paths`, and `typescript.yml:on.pull_request.paths`; there is no shared authority that makes one update cover the four consumers.

Strongest counterevidence: commands and action versions are local to their jobs, so much of the visible repetition is deliberate job isolation, and each language workflow is small.

Why adjacent scores do not fit: 3 does not fit because trigger selection is central to CI's purpose, the synchronization crosses both event sections and language workflows, and actual drift is present. 1 does not fit because the duplicated lists are easy to locate and most entries still agree.

Lens-boundary note: the stale OpenAPI trigger could be scored only as duplicate path knowledge. I used the repeated four-list maintenance burden for duplication, while treating the fact that `openapi/**` currently has no referent—and that line-based Zizmor identities no longer name the intended actions—as domain vocabulary drift.
