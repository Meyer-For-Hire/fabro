# Calibration Sample Review — Reviewer 3

Revision: `6bb6b5efcc0e36b52e3c097f532d9f2c00914c6c`

Scope follows `.chisel/cartography/codebase-map.md`: `fabro-workflow`,
`fabro-http`, `fabro-web-app`, and `repository-ci`. The `fabro-web-app`
reading excludes `apps/fabro-web/app/components/playground/**`;
`repository-ci` includes only `.github/workflows/rust.yml`,
`.github/workflows/typescript.yml`, and `.github/zizmor.yml`.

## Provisional Matrix

| Component | Ownership and boundaries | Simplicity | Domain model | Duplication of knowledge |
|---|---:|---:|---:|---:|
| `fabro-workflow` | 4 / High | 2 / High | 2 / High | 2 / High |
| `fabro-http` | 4 / High | 4 / High | 4 / High | 4 / High |
| `fabro-web-app` | 3 / High | 2 / High | 2 / High | 2 / High |
| `repository-ci` | 3 / High | 3 / High | 2 / High | 2 / High |

## `fabro-workflow`

### `ownership-boundaries` — 4, High confidence

Evidence:

- `lib/components/fabro-workflow/src/pipeline/mod.rs` exposes an ordered phase
  facade, while `pipeline/types.rs:Parsed`, `Transformed`, `Validated`,
  `Persisted`, `Initialized`, `Executed`, `Concluded`, and `Finalized` give each
  phase an explicit handoff.
- `lib/components/fabro-workflow/src/handler/mod.rs:Handler` and
  `HandlerRegistry` own workflow-specific dispatch;
  `src/node_handler.rs:WorkflowNodeHandler` is the narrow adapter to
  `fabro_core::handler::NodeHandler`.
- `lib/components/fabro-workflow/src/lifecycle/mod.rs:WorkflowLifecycle` states
  that it owns callback ordering and delegates event, hook, fidelity,
  auto-status, circuit-breaker, Git, and artifact work to focused lifecycle
  objects.
- `lib/components/fabro-workflow/Cargo.toml:[dependencies]` points from the
  orchestrator to parsing, validation, sandbox, persistence, model, and generic
  execution crates; generic traversal remains in `fabro-core`.

Strongest counterevidence: startup state is carried through
`operations/start.rs:StartServices`, `RunSession`,
`pipeline/types.rs:InitOptions`, and `services.rs:RunServices` /
`EngineServices`, so the lifecycle boundary has substantial wiring.

Why adjacent scores do not fit: 3 would treat that wiring as unclear ownership,
but the common path consistently identifies phase, handler, lifecycle, and
generic-executor owners. The counterevidence is primarily machinery inside the
intended orchestration owner, not a competing dependency direction or lifecycle
home.

### `simplicity` — 2, High confidence

Evidence:

- The normal start path crosses
  `operations/start.rs:start` → `execute_persisted_run` →
  `RunSession::new` → `RunSession::run` →
  `pipeline::initialize` → `pipeline::execute` →
  `pipeline::finalize` → `pipeline::pull_request`.
- The same run-scoped collaborators are reshaped across
  `operations/start.rs:StartServices`, `RunSession`,
  `pipeline/types.rs:InitOptions`, `services.rs:RunServices`, and
  `EngineServices`.
- `lifecycle/mod.rs:WorkflowLifecycle::new` takes the full set of lifecycle
  collaborators and has an explicit `too_many_arguments` exception before
  constructing seven sub-lifecycles with shared coordination state.

Strongest counterevidence: the phase-state types in
`pipeline/types.rs` and the focused handler/lifecycle modules make this
machinery traceable; the common path is not hidden.

Why adjacent scores do not fit: 3 does not fit because every ordinary run
traverses the service reshaping and multi-stage cleanup/finalization path; this
is central rather than edge friction. 1 does not fit because the named phase
sequence and handoff types provide a stable path through the machinery.

Representative routine change: adding a run-scoped execution-audit sink for
handlers would require threading it through
`operations/start.rs:StartServices`, `RunSession`,
`RunSession::new`, `RunSession::run`,
`pipeline/types.rs:InitOptions`, `pipeline/initialize.rs:initialize`, and
`services.rs:RunServices` or `EngineServices`.

### `domain-model` — 2, High confidence

Evidence:

- Positive mechanisms are substantial:
  `pipeline/types.rs:Validated` hides its graph and exposes validation
  operations, `ResumeState::from_projection` creates opaque resume state, and
  `run_status.rs` plus `outcome.rs` reuse canonical types from `fabro-types` and
  `fabro-core`.
- A central exception remains:
  `event/events.rs:Event::StageCompleted` represents `status` as `String`, while
  execution uses typed `outcome.rs:StageOutcome`.
  `event/convert.rs:stage_status_from_string` reparses the string and maps every
  unknown value to a failed outcome.
- The common producer
  `lifecycle/event.rs:EventLifecycle::after_node` converts the typed outcome to
  a string before the canonical event conversion converts it back.

Strongest counterevidence: the pipeline phase types, `RunStatus`,
`StageOutcome`, `StageId`, and the durable `fabro_types::EventBody` otherwise
give the main workflow concepts canonical typed shapes.

Why adjacent scores do not fit: 3 does not fit because stage completion is on
the execution hot path and accepts states the canonical outcome enum rejects.
1 does not fit because the canonical types and phase states still give the
workflow a coherent vocabulary overall.

Representative routine change: adding or changing a stage outcome would touch
the canonical `lib/foundation/fabro-core/src/outcome.rs:StageOutcome`, string
construction in `lifecycle/event.rs:EventLifecycle::after_node`,
`event/events.rs:Event::StageCompleted`,
`event/convert.rs:stage_status_from_string`, and terminal interpretation in
`pipeline/finalize.rs:classify_engine_result`.

### `duplication-knowledge` — 2, High confidence

Evidence:

- `event/events.rs:Event` defines the internal event shape,
  `event/names.rs:event_name` independently maps every variant to its external
  name, `event/stored_fields.rs:stored_event_fields` independently selects
  envelope fields, and `event/convert.rs:event_body_from_event` constructs the
  canonical `fabro_types::EventBody`.
- `docs/internal/events-strategy.md:Adding A New Event` explicitly requires
  synchronized edits to the internal event, tracing, external name,
  `EventBody`, stored fields, conversion, and consumers.
- Exhaustive matches make omissions visible, but they do not make one of those
  mappings authoritative for the others.

Strongest counterevidence: `event/emitter.rs:Emitter` canonicalizes each emitted
event once, all listeners receive the same `RunEvent`, and exhaustive matching
plus conversion tests detect much of the synchronization drift.

Why adjacent scores do not fit: 3 does not fit because adding an event is a
routine extension to this component and centrally requires several independent
authorities. 1 does not fit because the events strategy clearly identifies all
authorities and the compiler/test suite gives a stable update path.

Representative routine change: adding `run.suspended` would touch
`event/events.rs:Event`, `events.rs:Event::trace`,
`event/names.rs:event_name`,
`lib/foundation/fabro-types/src/run_event/mod.rs:EventBody`,
`event/stored_fields.rs:stored_event_fields`,
`event/convert.rs:event_body_from_event`, and relevant store/UI consumers.

## `fabro-http`

### `ownership-boundaries` — 4, High confidence

Evidence:

- The component is one focused source module:
  `lib/foundation/fabro-http/src/lib.rs` owns the reqwest facade,
  `ProxyPolicy`, client builders, build errors, and deterministic test clients.
- `src/lib.rs:HttpClientBuilder::build` and
  `BlockingHttpClientBuilder::build` are the construction boundary where the
  process proxy policy is applied.
- `clippy.toml:disallowed-methods` denies direct reqwest client constructors and
  points callers to this component; `fabro_static::EnvVars` supplies the one
  environment-variable name without introducing higher-level configuration.

Strongest counterevidence: the facade deliberately re-exports many reqwest
types, and exceptional consumers still carry direct reqwest dependencies for
generated clients or incompatible dependency versions.

Why adjacent scores do not fit: 3 does not fit because the normal async,
blocking, production, and test construction paths all converge on the same
owned policy, with a repository lint reinforcing that boundary.

### `simplicity` — 4, High confidence

Evidence:

- `src/lib.rs:define_builder!` expresses the common async/blocking builder once;
  the four convenience constructors are thin calls to the same builders.
- The common flow is direct:
  `HttpClientBuilder::new` → optional reqwest options →
  `HttpClientBuilder::build` → `ProxyPolicy::resolve` → reqwest build.
- The only async-only option is visibly isolated in
  `HttpClientBuilder::read_timeout`.

Strongest counterevidence: the macro hides the two generated impls and every
new exposed reqwest option requires another forwarding method.

Why adjacent scores do not fit: 3 does not fit because the macro removes a real
parallel API synchronization burden while leaving the common client-building
path locally readable; its indirection is not encountered beyond this file.

### `domain-model` — 4, High confidence

Evidence:

- `src/lib.rs:ProxyPolicy` has exactly the two supported states,
  `ProxyPolicy::resolve_with_env_value` makes explicit configuration override
  environment fallback, and invalid/non-Unicode values become
  `HttpClientBuildError`.
- `src/lib.rs:HttpClientBuildError` distinguishes invalid policy from underlying
  reqwest construction failure.
- `test_http_client` and `blocking_test_http_client` select the typed
  `ProxyPolicy::Disabled` rather than relying on ambient test environment state.

Strongest counterevidence: the environment boundary is necessarily stringly,
and `ProxyPolicy::parse` accepts case variants before producing the enum.

Why adjacent scores do not fit: 3 does not fit because invalid strings are
rejected at the boundary, precedence is explicit, and all downstream paths use
the closed enum.

### `duplication-knowledge` — 4, High confidence

Evidence:

- `src/lib.rs:define_builder!` is the single authority for shared async and
  blocking options and policy application.
- `ProxyPolicy::resolve` is the single production authority for explicit/env/
  default precedence.
- `clippy.toml:disallowed-methods` prevents ordinary callers from silently
  recreating client-construction policy outside the component.

Strongest counterevidence: async and blocking convenience constructors remain
as four syntactically similar functions, and `read_timeout` cannot live in the
shared macro surface.

Why adjacent scores do not fit: 3 does not fit because the remaining repetition
does not duplicate a policy or require independent decisions; it exposes
parallel entry points backed by the same authority.

## `fabro-web-app`

### `ownership-boundaries` — 3, High confidence

Evidence:

- `apps/fabro-web/app/entry.tsx:AppRuntime` owns browser bootstrap and global
  runtime providers; `router.tsx:routes` and
  `install-router.tsx:installRoutes` own the two route graphs.
- `app/lib/queries.ts` and `app/lib/mutations.ts` own server reads and writes;
  `app/lib/api-client.ts` owns transport/error normalization.
- `app/hooks/effects.ts` and purpose-named hooks such as
  `useRunEvents` and `useInstallRestartHealthPolling` contain browser resource
  lifecycles rather than leaving them in route rendering.
- `routes/run-detail.tsx:RunDetail` delegates its header, actions, model,
  lifecycle-toast, tab-shell, and docked-control responsibilities to the
  `routes/run-detail/**` modules.

Strongest counterevidence: two mapped common paths still concentrate several
responsibilities:
`install-app.tsx:InstallApp` / `useInstallController` contains state,
hydration, submission, step routing, payload construction, and rendering, while
`routes/run-stages.tsx:RunStages` / `buildStageActivity` contains event
interpretation and a large part of stage presentation.

Why adjacent scores do not fit: 4 does not fit because those central route
modules are not merely edge exceptions. 2 does not fit because routes, API
access, queries, mutations, browser effects, and build lifecycle still have
stable homes and dependencies generally point through those homes.

### `simplicity` — 2, High confidence

Evidence:

- The first-run common path is concentrated in
  `install-app.tsx:installReducer`, `useInstallController`, `InstallApp`,
  `LlmStep`, `ObjectStoreStep`, `SandboxStep`, `GithubStep`,
  `buildObjectStorePayload`, and `buildSandboxPayload`.
- The run-stage common path combines
  `routes/run-stages.tsx:selectStageRenderer`,
  `buildStageActivity`, filtering, debug views, waterfall construction, and
  `RunStages`.
- Cross-tab event sharing introduces a second substantial state machine at
  `app/lib/cross-tab-sse.ts:CrossTabSseCoordinator`, beneath the already
  separate shared-event-source logic in `app/lib/sse.ts:subscribeToSharedEventSource`.

Strongest counterevidence: reducers, discriminated unions, shared query hooks,
purpose-named integration hooks, and extracted run-detail modules make many
individual flows explicit and testable.

Why adjacent scores do not fit: 3 does not fit because installation, run-stage
inspection, and live refresh are mapped common paths, not optional edge
machinery. 1 does not fit because each path still has identifiable entry
points, state machines, and tests.

Representative routine change: adding an installation step for telemetry would
touch `install-app.tsx:INSTALL_STEPS`, `InstallState`, `InstallAction`,
`installReducer`, `useInstallController`, `InstallApp`, a new step component,
review-summary helpers, `install-api.ts`, and the generated install API
authority in `docs/public/api-reference/fabro-api.yaml`.

### `domain-model` — 2, High confidence

Evidence:

- Positive mechanisms include generated API types throughout the query and
  route layers, `mode.ts:FabroMode`, and exhaustive display maps such as
  `lib/sandbox-state.ts:SANDBOX_STATE_DISPLAY`.
- The central SSE boundary instead uses
  `lib/sse.ts:EventPayload`, where `event` is optional and all other fields are
  unknown, then extends it as
  `lib/run-events.ts:RunEventPayload` with optional string identifiers and
  another untyped `properties` map.
- `lib/run-events.ts:stageIdFromPayload` accepts `stage_id`, `node_id`, or
  `properties.node_id` as the stage identity.
- `lib/run-sandbox-lifecycle.ts:sandboxLifecycleKind` and `sandboxInstance`
  cast generated values into compatibility shapes and infer lifecycle from
  either `kind`, `instance`, or legacy `runtime` / `provider` fields.

Strongest counterevidence: normal HTTP reads and writes use
`@qltysh/fabro-api-client` types, and `Record<GeneratedEnum, ...>` display maps
make many API vocabulary changes compile-visible.

Why adjacent scores do not fit: 3 does not fit because SSE drives normal run
refresh and stage views while permitting absent event and identity fields with
multiple meanings. 1 does not fit because generated HTTP types and local
discriminated unions still provide a coherent model for most operations.

Representative routine change: making stage identity canonical across live
events would touch the wire authority
`docs/public/api-reference/fabro-api.yaml`,
`lib/sse.ts:EventPayload`, `lib/run-events.ts:RunEventPayload`,
`stageIdFromPayload`, and consumers such as
`routes/run-stages.tsx:buildStageActivity`.

### `duplication-knowledge` — 2, High confidence

Evidence:

- `lib/board-events.ts:BOARD_STATUS_EVENTS` independently decides which run
  events refresh lists, while `lib/run-events.ts:RUN_SUMMARY_EVENTS`,
  `TERMINAL_EVENTS`, and other sets decide detail invalidations.
- `lib/run-phases.ts:deriveRunPhases` independently matches the same lifecycle
  event vocabulary to build the pre-stage timeline.
- `lib/run-events.ts:STAGE_ACTIVITY_EVENT_TYPES` is a positive local authority
  shared with `routes/run-stages.tsx:buildStageActivity`, but it covers only one
  slice of the broader manual event policy.

Strongest counterevidence: list and detail invalidation are genuinely different
consumer decisions, `query-keys.ts:queryKeys` centralizes cache identities, and
the stage-activity list is deliberately shared with its reducer.

Why adjacent scores do not fit: 3 does not fit because a normal lifecycle-event
extension that affects board and run detail requires synchronized policy edits
in separate common subscriptions. 1 does not fit because each consumer's
authority is named, localized, and covered by focused tests.

Representative routine change: adding a `run.suspended` transition that should
refresh both list and detail views would touch
`board-events.ts:BOARD_STATUS_EVENTS`,
`run-events.ts:RUN_SUMMARY_EVENTS` (and possibly `TERMINAL_EVENTS` if its
semantics require it), `board-events.test.tsx`, `run-events.test.tsx`, and the
upstream event/OpenAPI authorities.

## `repository-ci`

### `ownership-boundaries` — 3, High confidence

Evidence:

- `.github/workflows/rust.yml:jobs` owns Rust formatting, lint, generated-doc,
  Linux test, twin-E2E, and manual macOS validation.
- `.github/workflows/typescript.yml:jobs` owns browser/client typecheck, web
  tests, and the embedded-SPA release build.
- Both workflows set top-level empty permissions and grant only
  `contents: read` per job; all third-party actions are commit-pinned.
- Generated-document and embedded-SPA behavior is delegated to
  `cargo dev docs check` and `cargo dev build`, leaving those build procedures
  in `fabro-build-tooling`.

Strongest counterevidence:
`.github/workflows/rust.yml:jobs.clippy.steps[name="Verify legacy auth identity removal"]`
contains an authentication-migration vocabulary grep inside the general CI
workflow, so an auth-domain transition also has a policy home here.

Why adjacent scores do not fit: 4 does not fit because that product-domain
policy crosses into the CI owner and the trigger boundary has drift discussed
under domain model. 2 does not fit because the normal validation jobs and their
delegated build/test authorities remain clearly owned and directional.

Representative routine change: renaming or restoring an authentication identity
would require changing the product types and also the legacy-name authority in
`.github/workflows/rust.yml:jobs.clippy.steps[name="Verify legacy auth identity removal"]`.

### `simplicity` — 3, High confidence

Evidence:

- Each job is a short checkout/setup/command sequence, and the two workflows
  split by the repository's Rust and Bun validation surfaces.
- `.github/workflows/rust.yml:jobs.test` explains the non-obvious twin-mode
  expression and why it must not use the strict E2E profile.
- `.github/workflows/typescript.yml:jobs.build` delegates the mixed Rust/SPA
  build to one repository command rather than reproducing its internals.

Strongest counterevidence: checkout, tool setup, install, permissions, runner,
and cache declarations are repeated across every job; the inline legacy-auth
shell condition is more elaborate than the surrounding declarative checks.

Why adjacent scores do not fit: 4 does not fit because routine maintenance must
scan repeated job scaffolding and one bespoke shell policy. 2 does not fit
because a contributor can still trace each common validation path directly
from one named job to one repository command.

### `domain-model` — 2, High confidence

Evidence:

- `.github/workflows/rust.yml:on.push.paths` and `on.pull_request.paths` contain
  `openapi/**`, but that directory does not exist at the assessed revision.
- The actual contract authority is
  `docs/public/api-reference/fabro-api.yaml`, as named by
  `AGENTS.md:API workflow`,
  `lib/foundation/fabro-api/build.rs:main`, and
  `lib/packages/fabro-api-client/package.json:scripts.generate`.
- Neither `.github/workflows/rust.yml:on.*.paths` nor
  `.github/workflows/typescript.yml:on.*.paths` names that actual contract
  path, even though both generated clients depend on it.

Strongest counterevidence: job names, Rust versus TypeScript scope, twin versus
live test meaning, and toolchain versions are otherwise explicit; the commands
the jobs run correspond to checked-in project commands.

Why adjacent scores do not fit: 3 does not fit because an ordinary edit to the
HTTP source of truth falls outside both central validation trigger models. 1
does not fit because the workflows still have a stable and mostly accurate
vocabulary for jobs, branches, tools, and commands.

Representative routine change: editing only
`docs/public/api-reference/fabro-api.yaml` should exercise Rust generation and
TypeScript typecheck/build, but its meaning would have to be repaired in
`.github/workflows/rust.yml:on.push.paths`,
`.github/workflows/rust.yml:on.pull_request.paths`,
`.github/workflows/typescript.yml:on.push.paths`, and
`.github/workflows/typescript.yml:on.pull_request.paths`.

### `duplication-knowledge` — 2, High confidence

Evidence:

- Each workflow repeats its path set under both `on.push.paths` and
  `on.pull_request.paths`; a new CI-relevant repository path has two authorities
  per language.
- `.github/workflows/rust.yml:jobs.fmt`, `jobs.clippy`,
  `jobs.generated-docs`, `jobs.test`, and `jobs.test-macos` independently repeat
  checkout pins, credential policy, runner/toolchain setup, and often cache
  setup.
- `.github/workflows/typescript.yml:jobs.typecheck`, `jobs.test`, and
  `jobs.build` independently repeat checkout, Bun setup, and frozen install.

Strongest counterevidence: independent jobs preserve failure isolation and
least-privilege permissions, while the substantive docs/build procedures are
delegated to repository commands rather than copied into YAML.

Why adjacent scores do not fit: 3 does not fit because path and tool-bootstrap
knowledge is repeated on every routine trigger or tool-version update. 1 does
not fit because all copies remain confined to two small workflow files and the
substantive check authorities are still identifiable.

Representative routine change: adding a new Rust-relevant `tools/**` tree would
require synchronized edits to
`.github/workflows/rust.yml:on.push.paths` and
`on.pull_request.paths`; updating the Rust checkout/toolchain baseline requires
reviewing the pins in every `rust.yml:jobs.*.steps` copy.

## Lens-Boundary Notes

- The repeated startup carriers in `fabro-workflow` could be labeled ownership
  or simplicity. I counted their unclear amount of machinery under simplicity;
  ownership was judged from whether each phase, resource lifecycle, and
  dependency direction has a named home.
- The workflow's internal `Event` and durable `EventBody` have documented
  distinct meanings. I therefore counted the many synchronized mappings under
  duplication, not domain model. The separate `StageCompleted.status: String`
  finding drives the domain-model score because it admits invalid states.
- Large web route files are not ownership findings merely because they are
  large. They lower simplicity where common behavior is difficult to trace; the
  ownership score moves only where several responsibilities remain concentrated
  despite otherwise clear route/data/effect homes.
- In the web event layer, optional/untyped payload shape is a domain-model
  finding. Repeating lifecycle-event policy across list, detail, and phase
  consumers is a duplication finding.
- In CI, the stale `openapi/**` referent is a domain-model finding because the
  path no longer means the API authority it purports to cover. Repeating trigger
  and setup lists is separately a duplication finding.
- The `fabro-http` builder macro adds local indirection, but its primary effect
  is to make shared async/blocking policy authoritative. I treated it as a
  positive duplication mechanism rather than simplicity friction.
