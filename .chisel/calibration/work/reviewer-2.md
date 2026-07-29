# Calibration Sample Review — Reviewer 2

Revision: `6bb6b5efcc0e36b52e3c097f532d9f2c00914c6c`

This review uses the component boundaries in `.chisel/cartography/codebase-map.md`. In particular, `fabro-web-app` excludes `apps/fabro-web/app/components/playground/**`, and `repository-ci` contains only `.github/workflows/rust.yml`, `.github/workflows/typescript.yml`, and `.github/zizmor.yml`.

## Score summary

| Component | Ownership and boundaries | Simplicity | Domain model | Duplication of knowledge |
|---|---:|---:|---:|---:|
| `fabro-workflow` | 3 (Medium) | 2 (High) | 3 (Medium) | 2 (High) |
| `fabro-http` | 4 (High) | 4 (High) | 3 (High) | 4 (High) |
| `fabro-web-app` | 4 (Medium) | 2 (Medium) | 2 (Medium) | 2 (Medium) |
| `repository-ci` | 4 (High) | 3 (High) | 3 (High) | 2 (High) |

## `fabro-workflow`

### `ownership-boundaries` — 3, Medium confidence

The component has a recognizable high-level owner and intended dependency direction. `lib/components/fabro-workflow/src/operations/mod.rs` owns run-level operations, while `lib/components/fabro-workflow/src/pipeline/mod.rs` owns the ordered phase API. `lib/components/fabro-workflow/src/pipeline/types.rs:Parsed`, `Transformed`, `Validated`, `Persisted`, `Initialized`, `Executed`, `Concluded`, and `Finalized` make phase ownership explicit. `lib/components/fabro-workflow/src/services.rs:RunServices` and `EngineServices` distinguish run-lifetime services from node-execution services, and `lib/components/fabro-workflow/src/node_handler.rs:WorkflowNodeHandler` is a visible adapter to `fabro-core`.

The friction is at the public edge: `lib/components/fabro-workflow/src/lib.rs` exposes operations, pipeline phases, handlers, records, services, runtime storage, and several `#[doc(hidden)]` modules. Callers can therefore enter below the complete lifecycle as well as through `lib/components/fabro-workflow/src/operations/start.rs:start`. This weakens containment, but it does not create a competing production owner.

**Strongest counterevidence:** The typed phase outputs and the `RunServices`/`EngineServices` split strongly reinforce one workflow lifecycle.

**Why adjacent scores do not fit:** A 4 does not fit because the broad facade exposes enough lifecycle internals to make the boundary porous. A 2 does not fit because the normal `start` path and each phase owner remain identifiable and dependencies are delegated to dedicated crates.

### `simplicity` — 2, High confidence

The stable common path is traceable, but routine work crosses substantial central machinery: `lib/components/fabro-workflow/src/operations/start.rs:start` → `execute_persisted_run` → `RunSession::new` → `RunSession::run` → `pipeline::initialize` → `pipeline::execute` → `pipeline::finalize` → `pipeline::pull_request`. Along that path, `StartServices`, `RunSession`, and `lib/components/fabro-workflow/src/pipeline/types.rs:InitOptions` each carry many run concerns, while bootstrap, completion, cleanup, steering-drain, sandbox, and event-flush guards add multiple exit paths. `lib/components/fabro-workflow/src/pipeline/initialize.rs:initialize` also coordinates sandbox creation/reconnection, hooks, credentials, Git setup, handler construction, and resume state.

**Representative routine change:** Adding one run-scoped execution service would normally thread through `operations/start.rs:StartServices`, `RunSession`, and `RunSession::new`; `pipeline/types.rs:InitOptions`; `pipeline/initialize.rs:initialize`; and `services.rs:RunServices` or `EngineServices`.

**Strongest counterevidence:** `operations/start.rs:RunSession::run` presents the main phases in a linear order, and the phase-specific types preserve that order despite the setup machinery.

**Why adjacent scores do not fit:** A 3 does not fit because the pressure is on the main run path rather than at an edge. A 1 does not fit because there is a stable phase sequence and named service bundles to follow.

### `domain-model` — 3, Medium confidence

The strongest mechanism is the phase-state model in `lib/components/fabro-workflow/src/pipeline/types.rs`; private fields on `Validated` and `Persisted` and opaque `ResumeState` prevent several invalid transitions. `lib/components/fabro-workflow/src/pipeline/finalize.rs:classify_engine_result` is also a clear authority for translating an engine result into `StageOutcome`, failure detail, and `RunStatus`.

The main friction is the extensible, string-valued handler vocabulary on the common graph path. `lib/components/fabro-workflow/src/handler/mod.rs:HandlerRegistry::resolve` works with type strings and falls back to the default handler, while `default_registry` registers the built-in strings. Validation in `fabro-validate` protects normal runs, but execution itself does not carry a closed built-in handler type.

**Strongest counterevidence:** `pipeline/types.rs:ResumeState::from_projection`, the phase output types, and `pipeline/finalize.rs:classify_engine_result` give important workflow concepts one enforced shape.

**Why adjacent scores do not fit:** A 4 does not fit because handler identity remains string-valued and default-resolved through a central execution boundary. A 2 does not fit because validation and typed phase states canonicalize the normal run before execution.

### `duplication-knowledge` — 2, High confidence

Event knowledge is repeated across central authorities. `lib/components/fabro-workflow/src/event/events.rs:Event` defines the emitter-facing shape, `lib/components/fabro-workflow/src/event/convert.rs:event_body_from_event` translates it to the stored `fabro_types::EventBody`, `lib/components/fabro-workflow/src/event/names.rs:event_name` separately assigns wire names, and `lib/components/fabro-workflow/src/event/stored_fields.rs:stored_event_fields_for_variant` separately assigns envelope metadata. These exhaustive matches help detect omissions, but every ordinary event extension still requires synchronized semantic decisions.

**Representative routine change:** Adding a stored workflow event can touch `event/events.rs:Event`, `event/convert.rs:event_body_from_event`, `event/names.rs:event_name`, `event/stored_fields.rs:stored_event_fields_for_variant`, and the canonical `lib/foundation/fabro-types/src/run_event/mod.rs:EventBody` authority.

**Strongest counterevidence:** `event/convert.rs:to_run_event_at` is the single assembly point, and Rust's exhaustive matches turn many missed updates into compile failures.

**Why adjacent scores do not fit:** A 3 does not fit because event emission and persistence are central, recurring behavior. A 1 does not fit because the authorities are explicit and compiler-checked rather than unidentifiable.

## `fabro-http`

### `ownership-boundaries` — 4, High confidence

`lib/foundation/fabro-http/src/lib.rs` has one focused transport-construction boundary. `HttpClientBuilder`, `BlockingHttpClientBuilder`, `ProxyPolicy`, the client aliases, and the production/test constructors all live there; the crate depends only on `fabro-static`, `reqwest`, and `thiserror`. Repository policy reinforces the boundary through `clippy.toml:disallowed-methods`, which directs raw reqwest construction to this facade.

**Strongest counterevidence:** The public reqwest aliases and re-exports make the abstraction intentionally permeable, so it does not own higher-level request behavior.

**Why the adjacent score does not fit:** A 3 does not fit because exposing reqwest types is part of the mapped purpose, while construction policy and proxy resolution still have one clear owner.

### `simplicity` — 4, High confidence

`lib/foundation/fabro-http/src/lib.rs:define_builder` expresses shared async/blocking forwarding once. Both builders end at the same short `ProxyPolicy::resolve` and `build` path, and `http_client`, `test_http_client`, `blocking_http_client`, and `blocking_test_http_client` are thin named entry points. A shared reqwest builder option is normally added once to the macro.

**Strongest counterevidence:** The macro hides generated methods, and async-only `HttpClientBuilder::read_timeout` must sit outside it.

**Why the adjacent score does not fit:** A 3 does not fit because this indirection directly removes twin implementations and leaves callers with a single conventional builder path.

### `domain-model` — 3, High confidence

`lib/foundation/fabro-http/src/lib.rs:ProxyPolicy` gives the repository policy two named states, `ProxyPolicy::resolve_with_env_value` defines explicit-over-environment precedence, and `HttpClientBuildError::InvalidProxyPolicy` rejects unknown values. The tests cover default, environment, invalid, and explicit-override cases.

The isolated ambiguity is that `HttpClientBuilder::no_proxy` and `HttpClientBuilder::proxy_policy(ProxyPolicy::Disabled)` both publicly express disabled proxy behavior, but `no_proxy` mutates the inner builder without updating the policy field. Their relationship is not represented or documented in the type.

**Strongest counterevidence:** The closed enum, typed error, and resolver tests make the environment-facing policy meaning unusually explicit.

**Why adjacent scores do not fit:** A 4 does not fit because two public controls overlap without an encoded relationship. A 2 does not fit because the overlap is local and every normal constructor still passes through one two-state resolver.

### `duplication-knowledge` — 4, High confidence

The builder macro is the authority for behavior shared by synchronous and asynchronous clients, and every constructor delegates to those builders. The production/test and async/blocking helper names repeat syntax, not policy: test behavior is expressed once as `ProxyPolicy::Disabled`.

**Strongest counterevidence:** Four constructor helpers and the separate async-only impl are superficially repetitive.

**Why the adjacent score does not fit:** A 3 does not fit because changing proxy precedence or disabled behavior has one authority; the remaining repetition does not require synchronized policy decisions.

## `fabro-web-app`

### `ownership-boundaries` — 4, Medium confidence

The main browser lifecycle has clear homes. `apps/fabro-web/app/entry.tsx` selects install or normal routing and owns root providers; `app/router.tsx:routes` owns the product route graph; `app/install-router.tsx:installRoutes` owns first-run routing; `app/lib/api-client.ts` owns HTTP normalization; `app/lib/queries.ts` and `app/lib/mutations.ts` own shared server access; and `app/hooks/effects.ts` contains reusable browser-effect lifecycles. Route modules own page-specific composition. The separately mapped playground enters through `app/router.tsx` without its excluded implementation being absorbed into this assessment.

**Strongest counterevidence:** `app/routes/run-stages.tsx` and `app/install-app.tsx` each combine page state, domain projection, and rendering in one route-owned file.

**Why the adjacent score does not fit:** A 3 does not fit because those combinations create local complexity, but no competing owner or reversed dependency was identified; shared cross-route responsibilities still have clear modules.

### `simplicity` — 2, Medium confidence

Two common product paths carry central transformation machinery. `apps/fabro-web/app/routes/run-stages.tsx` turns event envelopes into `TurnType` values in `buildStageActivity`, then separately groups, filters, timelines, labels, summarizes, and renders them through `buildChatItems`, `groupConsecutiveTools`, `filterDisplayItems`, `buildThreadDnaItems`, and the route's view components. `apps/fabro-web/app/install-app.tsx` similarly contains the install reducer, session hydration, controller, step forms, review, finishing, payload construction, and supporting controls in one flow.

**Representative routine change:** Changing how a tool event appears on the stage page requires tracing `run-stages.tsx:buildStageActivity`, `buildChatItems`/`groupConsecutiveTools`, `buildThreadDnaItems`, `turnLabel`, `turnSummary`, `EventDetails`, and `StageChatView`.

**Strongest counterevidence:** The stage path uses discriminated unions and mostly pure exported transformations with focused tests, so each individual step can be reasoned about.

**Why adjacent scores do not fit:** A 3 does not fit because the long transformation chains are central to major routes. A 1 does not fit because the named pure functions provide a stable trace through both flows.

### `domain-model` — 2, Medium confidence

Generated API types provide a useful boundary, but the central event path accepts several simultaneous shapes. `apps/fabro-web/app/lib/run-events.ts:RunEventPayload` makes event identity and metadata optional and `stageIdFromPayload` falls back from `stage_id` to `node_id` to `properties.node_id`. `app/routes/run-stages.tsx:activityEventStageId` repeats that shape tolerance for stored `EventEnvelope`s, while `buildStageActivity` reads tool, text, argument, and output values from both `properties` and legacy top-level fields via `app/lib/unknown.ts`.

**Representative routine change:** Moving one stage-event field to its canonical envelope location can require coordinated interpretation changes in `lib/run-events.ts:RunEventPayload` and `stageIdFromPayload`, plus `routes/run-stages.tsx:activityEventStageId` and `buildStageActivity`.

**Strongest counterevidence:** Once parsed, `run-stages.tsx:TurnType`, `StageRenderer`, and generated `StageHandler`/`StageState` types give the UI clear closed shapes.

**Why adjacent scores do not fit:** A 3 does not fit because the multi-shape event interpretation is on live invalidation and the main stage view, not an edge. A 1 does not fit because generated types and discriminated UI projections establish a stable canonical shape after parsing.

### `duplication-knowledge` — 2, Medium confidence

Stage-state presentation policy is authoritative in several common views. `apps/fabro-web/app/lib/stage-sidebar.ts:ACTIVE_STAGE_STATES`, `IN_FLIGHT_STAGE_STATES`, `SUCCEEDED_STAGE_STATES`, `STAGE_STATUS_TONE`, and `STAGE_STATUS_LABEL` define classifications and visuals, while `app/components/stage-sidebar.tsx:statusConfig`, `app/components/run-waterfall.tsx:stageBarClass` and `isStageInFlight`, and `app/components/stage-popover.tsx:StatusPill` make parallel state decisions.

**Representative routine change:** Adding a generated `StageState` requires reviewing or changing all of those authorities so the sidebar, waterfall, and popover agree on activity, success, label, and tone.

**Strongest counterevidence:** Generated `StageState` plus exhaustive `Record<StageState, ...>` mappings catch many omissions, and `lib/stage-sidebar.ts` already centralizes several shared classifications.

**Why adjacent scores do not fit:** A 3 does not fit because stage status is central to multiple routine run views and synchronization is recurring. A 1 does not fit because the generated enum is a clear semantic authority and TypeScript catches many missing cases.

## `repository-ci`

### `ownership-boundaries` — 4, High confidence

The two workflows divide validation by ecosystem: `.github/workflows/rust.yml:jobs` owns Rust format, lint, generated-doc, workspace test, twin-mode ignored tests, and manual macOS validation; `.github/workflows/typescript.yml:jobs` owns web/client typecheck, web tests, and the embedded-SPA production build. Both use top-level empty permissions and job-local read permission. The cross-language Cargo build in the TypeScript build job validates the mapped embedded-SPA integration rather than creating a second build owner.

**Strongest counterevidence:** The Rust clippy job contains a repository-wide legacy-auth guard that also scans TypeScript and API paths.

**Why the adjacent score does not fit:** A 3 does not fit because that cross-language invariant remains an explicitly named CI check, while job and workflow lifecycle ownership stays clear.

### `simplicity` — 3, High confidence

The main flow is explicit: named jobs perform checkout, tool setup, and one or two direct repository commands. The isolated friction is `.github/workflows/rust.yml:jobs.clippy.steps.Verify legacy auth identity removal`, where a long regular expression and shell exit-status protocol are embedded in a lint job. The twin-mode test semantics also need a substantial comment and package expression in `jobs.test`.

**Strongest counterevidence:** Separate jobs, direct commands, pinned tools, and no reusable-workflow indirection make routine CI behavior easy to locate.

**Why adjacent scores do not fit:** A 4 does not fit because the legacy guard and twin-mode selection require non-obvious local interpretation. A 2 does not fit because that machinery is isolated and ordinary check changes still follow a direct job structure.

### `domain-model` — 3, High confidence

Job names, triggers, permissions, platforms, and commands have consistent meanings in the GitHub Actions structure. Exact action SHAs and named modes such as `--profile ci` reduce ambiguity. The main gap is that `.github/workflows/rust.yml:jobs.test` relies on the external default meaning of `FABRO_TEST_MODE` for its twin run rather than setting the mode in the workflow; the comment is the only local declaration of that state.

**Strongest counterevidence:** The command, package selector, and explanation tightly describe the intended twin-only behavior, and every job has an explicit runner and permission set.

**Why adjacent scores do not fit:** A 4 does not fit because a central test mode is implicit in an external default. A 2 does not fit because the rest of the workflow vocabulary is coherent and the implicit state is limited to one documented test step.

### `duplication-knowledge` — 2, High confidence

Trigger policy is repeated verbatim between `on.push.paths` and `on.pull_request.paths` in both workflow files. Action versions and bootstrap steps are also copied across every job. `.github/zizmor.yml:rules.stale-action-refs.ignore` adds line-number references to `rust.yml`, creating another manually synchronized representation; at this revision its listed lines 37, 49, and 62 are respectively a blank line, the `fmt` job key, and a Cargo command rather than action references.

**Representative routine change:** Adding a new Rust-owned source area requires matching edits to `.github/workflows/rust.yml:on.push.paths` and `on.pull_request.paths`; upgrading checkout requires synchronized edits in `jobs.fmt`, `clippy`, `generated-docs`, `test`, and `test-macos`, followed by review of `.github/zizmor.yml:rules.stale-action-refs.ignore`.

**Strongest counterevidence:** The duplication is explicit and small enough to inspect, and each actual validation command appears once in its intended job.

**Why adjacent scores do not fit:** A 3 does not fit because triggers and action versions are central, recurring maintenance knowledge and the stale line selectors demonstrate drift. A 1 does not fit because the canonical workflows and intended checks remain identifiable.

## Lens-boundary confusion

- The `fabro-workflow` `Event`/`EventBody` split could be described as two domain shapes. I assigned its score effect to `duplication-knowledge` because the discriminating problem is the synchronized event name, conversion, and envelope-field decisions, not an inability to identify either type's meaning.
- The size and mixed contents of `fabro-web-app` route files could look like misplaced responsibility. I assigned the main effect to `simplicity` because the route remains the clear owner; the problem is tracing the amount of local machinery.
- Repeated `StageState` maps could be treated as domain drift. I assigned them to `duplication-knowledge` because the generated enum preserves meaning and the observed burden is repeating presentation/classification policy across views.
- The `.github/zizmor.yml` line selectors could be treated as invalid configuration meaning. I assigned their main effect to `duplication-knowledge` because the failure mechanism is manual synchronization with line positions; `repository-ci` domain scoring instead uses the implicit twin-mode default.
- `fabro-http`'s macro could be treated as simplicity indirection, while its two proxy-disable controls could be treated as duplicate policy. I treated the macro as a positive simplicity/duplication mechanism and the overlapping controls as `domain-model` friction because the unresolved question is what each public control means.
