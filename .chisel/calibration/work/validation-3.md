# Chisel calibration validation 3

Revision reviewed: `6bb6b5efcc0e36b52e3c097f532d9f2c00914c6c`.

This is an independent reading of the final rubric and the assigned component
scopes. I traced representative production entry points and direct boundary
callers. I did not inspect prior calibration scores or any other file in
`.chisel/calibration/work/`.

## Score summary

| Component | Lens | Score | Evidence confidence |
| --- | --- | ---: | --- |
| `fabro-workflow` | ownership-boundaries | 2 | High |
| `fabro-workflow` | domain-model | 2 | High |
| `fabro-http` | domain-model | 4 | High |
| `fabro-http` | duplication-knowledge | 3 | Medium |
| `fabro-web-app` | ownership-boundaries | 4 | Medium |
| `repository-ci` | ownership-boundaries | 4 | Medium |
| `repository-ci` | domain-model | 2 | High |
| `fabro-checkpoint` | ownership-boundaries | 2 | High |
| `fabro-checkpoint` | simplicity | 3 | Medium |
| `fabro-checkpoint` | domain-model | 2 | High |
| `fabro-checkpoint` | duplication-knowledge | 3 | Medium |

## `fabro-workflow`

### `ownership-boundaries`: 2 (High)

- **Evidence:** `src/lifecycle/mod.rs:53-80,221-469` provides a real central
  `WorkflowLifecycle` and explicitly orders focused event, hook, fidelity, Git,
  artifact, status, and circuit-breaker delegates. Its terminal callback,
  however, only forwards `on_run_end` to the hook. Normal terminal persistence,
  metadata completion, terminal event emission, and sandbox stopping instead
  live in `src/pipeline/finalize.rs:524-635`. Bootstrap and execution failures
  take another terminal path in `src/operations/start.rs:176-345`, while
  `RunSession::run` also installs cleanup and drain guards at
  `src/operations/start.rs:889-947`. A routine terminal-lifecycle change must
  therefore coordinate the lifecycle orchestrator, finalizer, and detached
  failure/guard paths.
- **Strongest counterevidence:** The normal phase sequence is plainly owned by
  `RunSession::run` (`initialize -> execute -> finalize -> pull_request`), and
  callback ordering inside graph execution has one obvious owner,
  `WorkflowLifecycle`.
- **Why 3 does not fit:** Terminal completion, failure, persistence, and cleanup
  are common paths, not isolated edge compatibility. The split therefore
  remains central even though each individual phase is understandable.
- **Why 1 does not fit:** Stable phase owners and a stable dependency direction
  are readily identifiable; the problem is coordination among them, not the
  absence of ownership.
- **Rule discrimination:** The repository example correctly requires terminal
  inspection and rule 1 makes the common terminal split score-capping. Decision
  rule 2 is less literal here because no single identity is resupplied across
  every split, but the score does not depend on that rule.

### `domain-model`: 2 (High)

- **Evidence:** `src/lifecycle/event.rs:319-390` starts with the typed
  `StageOutcome` on an `Outcome`, serializes it with
  `outcome.status.to_string()`, and stores the result in the
  `Event::StageCompleted.status: String` field declared at
  `src/event/events.rs:264-293`. Every successful stage then passes through
  `src/event/convert.rs:14-24,309-348`, which reparses the string and silently
  converts an unknown value into a non-retryable failure. This is the ordinary
  durable-event path, not an import-only compatibility path.
- **Strongest counterevidence:** The destination event model already has the
  canonical `fabro_types::StageOutcome`, parallel-branch completion carries it
  directly, and other core run concepts use typed IDs, reasons, timings, and an
  opaque `ResumeState` (`src/pipeline/types.rs:252-285`).
- **Why 3 does not fit:** The invalid intermediate occurs for each ordinary
  successful stage before durable interpretation, so it is central rather than
  an isolated escape hatch.
- **Why 1 does not fit:** `StageOutcome` itself has a stable, typed meaning; the
  defect is the recurring string round trip between two typed points.
- **Rule discrimination:** Decision rule 4 is directly discriminating here:
  this is exactly a common-path invalid intermediate.

## `fabro-http`

### `domain-model`: 4 (High)

- **Evidence:** `src/lib.rs:23-61` gives proxy behavior a closed
  `ProxyPolicy::{System, Disabled}` vocabulary. The environment boundary
  accepts case-insensitive valid names, rejects every other value with a typed
  `HttpClientBuildError`, handles non-Unicode values explicitly, gives explicit
  policy precedence over the environment, and resolves absence to `System`.
  Both generated builders invoke this resolver before constructing a client
  (`src/lib.rs:72-193`), and the test-client entry points select
  `ProxyPolicy::Disabled` rather than passing an unchecked string
  (`src/lib.rs:195-213`).
- **Strongest counterevidence:** The facade deliberately exposes reqwest's
  lower-level `Proxy` and `.no_proxy()` operations, so callers can compose
  transport details outside the two-value environment policy.
- **Why 3 does not fit:** Those operations are typed builder choices, not
  unvalidated representations of the `FABRO_HTTP_PROXY_POLICY` value. Every
  common construction path still validates that boundary before use; I found no
  material meaning or validation friction.
- **Why 1-2 do not fit:** There is one stable meaning, one resolver, and no
  recurring conversion through an invalid intermediate.
- **Rule discrimination:** Decision rule 4 could be read ambiguously if every
  low-level builder method is called a policy escape hatch. The rubric's own
  `ProxyPolicy` example resolves that ambiguity in favor of the closed,
  validated environment-policy model.

### `duplication-knowledge`: 3 (Medium)

- **Evidence:** `define_builder!` at `src/lib.rs:72-193` is one authoritative
  production mechanism for the shared async/blocking builder surface and for
  applying the resolved proxy policy. The four convenience constructors route
  through those builders. The remaining repeated knowledge is narrow:
  `"system"` and `"disabled"` appear both in the parser and in the manually
  maintained `InvalidProxyPolicy` expectation text
  (`src/lib.rs:29-35,63-69`).
- **Strongest counterevidence:** The macro removes the materially risky
  async/blocking synchronization, and the compiler forces the policy-application
  match to cover every enum variant. The two test helpers' use of
  `ProxyPolicy::Disabled` is ordinary reuse, not a second policy authority.
- **Why 4 does not fit:** The user-facing valid-value list is a small second
  representation that can drift from the parser, so there is some isolated
  repeated domain knowledge.
- **Why 2 does not fit:** There is no direct evidence that routine changes
  repeatedly synchronize separate async/blocking implementations. A future
  enum variant is hypothetical, and rule 5 specifically says exhaustive
  compiler-checked branches and hypothetical variants do not establish
  competing authorities.
- **Rule discrimination:** Rule 5 cleanly rules out 2 but is non-discriminating
  between 3 and 4 for a duplicated allowed-value error message. I treat that
  message as real but isolated maintenance friction, hence 3.

## `fabro-web-app`

### `ownership-boundaries`: 4 (Medium)

- **Evidence:** `app/entry.tsx:17-48` owns root creation, global SWR policy,
  build-version guarding, toast mounting, and the single normal/install router
  choice. `app/router.tsx:97-184` owns normal route composition, while
  `app/install-router.tsx:6-22` owns the install graph. Shared HTTP translation
  and unauthorized handling live in `app/lib/api-client.ts:213-309`; shared
  reads such as `useRun` live in `app/lib/queries.ts:182-187`; recurring run
  mutations and cache follow-up live in
  `app/lib/mutations.ts:65-149`. Route components compose these owners.
  Separately, `scripts/build.ts:183-249,289-368` contains the complete
  app-local build, atomic publication, and old-build pruning lifecycle and
  publishes only `apps/fabro-web/dist`; boundary tooling mirrors that output
  into the Rust SPA rather than the web build writing across the boundary.
- **Strongest counterevidence:** Some route-specific CRUD mutations import
  `apiData` and generated API objects directly, and the install feature spans
  `install-app.tsx`, `install-api.ts`, `install-query.ts`, and effect hooks.
  `run-detail.tsx` is also a busy composition point.
- **Why 3 does not fit:** The direct calls remain at the route-specific UX
  owner and still use the shared transport/error boundary; shared read and
  recurring run-lifecycle responsibilities are not reimplemented there.
  Install state, transport, query, and browser effects have distinct homes.
  I found no isolated lifecycle that must leave its owner and resupply identity.
- **Why 1-2 do not fit:** Runtime, routing, transport, queries, route UX, and
  build publication all have stable owners with dependencies pointing from
  composition toward shared services.
- **Rule discrimination:** The final repository example is useful and
  discriminating: a large route is not by itself boundary leakage. The score
  would change if direct routes reimplemented shared transport or cache
  lifecycles, but representative boundary checks did not show that.

## `repository-ci`

### `ownership-boundaries`: 4 (Medium)

- **Evidence:** `.github/workflows/rust.yml:48-147` owns Rust formatting,
  lint/architecture checks, generated docs, Linux tests, twin E2E selection,
  and manual macOS tests. `.github/workflows/typescript.yml:36-77` owns web and
  generated-client typechecks, web tests, and the production embedded-SPA
  integration build. Each workflow owns its concurrency and least-privilege job
  permissions. The TypeScript workflow's `cargo dev build` is the intentional
  integration boundary that consumes the web bundle; it does not create a
  competing implementation of the web build.
- **Strongest counterevidence:** The TypeScript build job invokes Rust build
  tooling, path scopes overlap around `lib/apps/fabro-spa/**`, and
  `.github/zizmor.yml` is configuration whose consumer is not shown in these
  files.
- **Why 3 does not fit:** Cross-language integration is part of the mapped CI
  purpose and has one concrete home. The stale configuration values discussed
  below are domain-model findings, while duplicated push/pull selectors are
  duplication findings; counting either again as ownership friction would
  violate the rubric's primary-lens rule.
- **Why 1-2 do not fit:** The Rust and TypeScript responsibilities and their
  dependency direction are stable. Routine validation changes have an obvious
  workflow owner rather than requiring competing lifecycle owners.
- **Rule discrimination:** The instruction not to penalize an unevidenced
  missing lifecycle matters for the unseen zizmor consumer. The rubric is
  otherwise discriminating once repeated selector policy is kept out of the
  ownership lens.

### `domain-model`: 2 (High)

- **Evidence:** Both Rust trigger selectors name `openapi/**`
  (`.github/workflows/rust.yml:6-19,22-35`), but that revision has no tracked
  `openapi/` target. The actual Rust generator and TypeScript generator consume
  `docs/public/api-reference/fabro-api.yaml`
  (`lib/foundation/fabro-api/build.rs:159` and
  `lib/packages/fabro-api-client/package.json:7`), a path omitted from both
  workflow trigger models. This makes a core API-spec change invisible to the
  intended CI trigger. In addition, all three
  `.github/zizmor.yml:4-6` line selectors target
  `.github/workflows/rust.yml` lines 37, 49, and 62, which are respectively
  `workflow_dispatch`, the `fmt` job key, and a shell `run`, not action
  references for `stale-action-refs`.
- **Strongest counterevidence:** Most configured branches, paths, action SHAs,
  runner labels, job names, and commands have clear current targets, and both
  workflow documents have a stable overall schema.
- **Why 3 does not fit:** The dead OpenAPI selector sits in both central Rust
  push and pull-request triggers and omits the actual source of truth. It is not
  merely an isolated stale lint suppression.
- **Why 1 does not fit:** The CI configuration language and almost all values
  remain interpretable; the problem is recurring invalid/no-target identifiers,
  not the absence of a stable configuration model.
- **Rule discrimination:** Decision rule 6 correctly classifies the no-target
  identifiers as domain pressure, but it does not itself distinguish 2 from 3.
  The centrality of the API source-of-truth trigger is what selects 2.

## Control: `fabro-checkpoint`

### `ownership-boundaries`: 2 (High)

- **Evidence:** Inside the component, `BranchStore` owns a branch string and
  author and delegates Git objects to `Store`
  (`src/branch.rs:17-82`), which is a sensible direction. At the production
  boundary, however, no production caller constructs `BranchStore`.
  `fabro-workflow/src/run_metadata.rs:272-451` instead keeps `Store`, branch,
  author, `parent_oid`, and discovery state as separate fields, manually writes
  blobs and trees, supplies parents to `Store::write_commit`, resupplies the
  branch to `Store::update_ref`, and owns fetch/push discovery. Other checkpoint
  commit and trailer lifecycle work also remains in `fabro-workflow`. Thus the
  mapped checkpoint/metadata-branch lifecycle crosses the scoped owner on the
  normal production path.
- **Strongest counterevidence:** `Store` is itself a mapped public entry point,
  the dependency direction remains `fabro-workflow -> fabro-checkpoint`, and
  remote authentication/push orchestration reasonably belongs near a workflow
  run rather than in a low-level Git object store.
- **Why 3 does not fit:** The caller-held branch and parent identity are used on
  every metadata snapshot, not only in an isolated migration or uncommon
  fallback.
- **Why 1 does not fit:** Low-level Git ownership and the higher workflow
  orchestration are both stable and understandable; they simply split one
  routine persistence lifecycle.
- **Rule discrimination:** Decision rule 2 is directly discriminating:
  `RunMetadataWriter` retains and repeatedly resupplies the identity needed to
  complete operations on `Store`. The mapped breadth of “metadata branches”
  makes this more than ordinary parameter passing.

### `simplicity`: 3 (Medium)

- **Evidence:** The production low-level path is traceable:
  `Store::write_blob -> TreeEntries::set -> Store::write_tree ->
  Store::write_commit -> Store::update_ref`
  (`src/git.rs:123-188`). `BranchStore::write_with` also gives branch-oriented
  writes one linear read/modify/write implementation
  (`src/branch.rs:56-117`). The recursive flat-tree conversion is justified by
  Git's nested tree representation. The friction is isolated: `BranchStore` is
  a sizeable second entry layer with tests but no production caller at this
  revision, and `Cargo.toml:18` declares `fabro-store` although scoped
  production code does not reference it.
- **Strongest counterevidence:** The two entry points represent legitimate
  abstraction levels, and the mapped cartography names both. None of the normal
  `Store` operations requires navigating configuration machinery or dynamic
  dispatch.
- **Why 4 does not fit:** The unused higher layer/dependency is concrete,
  avoidable surface and configuration burden, even though it is off the current
  production common path.
- **Why 2 does not fit:** Routine production writes do not repeatedly choose
  between `Store` and `BranchStore`; the observed caller consistently uses
  `Store`, and that path is direct.
- **Rule discrimination:** The “public method alone does not establish
  frequency” rule prevents treating `BranchStore` as a competing common path.
  It is less discriminating between 3 and 4; the concrete unused dependency and
  unused entry layer are why I select 3.

### `domain-model`: 2 (High)

- **Evidence:** `GitAuthor::from_options` accepts arbitrary name/email strings
  (`src/author.rs:22-30`), while `BranchStore::new` only interprets them by
  calling `Signature::now(...).expect(...)`
  (`src/branch.rs:26-39`). `TreeEntries` stores paths as unrestricted `String`
  and `BranchStore::write_entry/write_entries` put caller strings into it
  without validation (`src/git.rs:46-90`,
  `src/branch.rs:84-109`); interpretation and possible rejection occur later
  while rebuilding Git trees. `FileMode::from_i32` also maps every unknown Git
  mode to `Blob` (`src/git.rs:21-36`) rather than preserving or rejecting an
  unknown shape. These invalid-capable intermediates sit on the mapped storage
  entry paths.
- **Strongest counterevidence:** `FileMode` is closed for values the component
  writes, normal metadata callers validate paths before constructing
  `TreeEntries`, Git itself rejects malformed signatures/trees, and object IDs
  use git2's typed `Oid`.
- **Why 3 does not fit:** Raw author and path values are carried by the ordinary
  entry-point types and interpreted later; they are not confined to a separate
  compatibility importer.
- **Why 1 does not fit:** Authors, tree entries, modes, branches, and commits all
  have stable intended meanings. The issue is delayed validation and lossy
  fallback, not an unidentifiable core concept.
- **Rule discrimination:** Decision rule 4 is discriminating here: these are
  common-path invalid-capable intermediate shapes rather than a low-level
  escape hatch unused by the entry path.

### `duplication-knowledge`: 3 (Medium)

- **Evidence:** Important transformations are mostly authoritative:
  `FileMode::{as_i32,from_i32}` contains the mode mapping,
  `BranchStore::write_with` contains branch read/modify/write, and
  `GitAuthor::default` contains the default identity. The narrow repeated
  knowledge is the bare-branch to full-ref transformation
  `format!("refs/heads/{branch}")` in each of
  `Store::{update_ref,resolve_ref,delete_ref}`
  (`src/git.rs:182-225`), with another full-ref rendering at the direct
  workflow metadata boundary. Trailer rendering also spells
  `"{}: {}"` in both `append` and `format_message`
  (`src/trailer.rs:9-65`).
- **Strongest counterevidence:** The repeated ref syntax is stable low-level Git
  syntax, the three ref methods implement different operations, and the
  apparent duplication in single-entry/multi-entry or tip/commit reads has
  intentionally different result shapes. Unifying those operations would risk
  a parameterized mega-helper.
- **Why 4 does not fit:** Full-ref and trailer-line rendering have small but real
  second representations rather than one helper/type enforcing each
  transformation.
- **Why 2 does not fit:** There is no direct evidence of routine changes
  repeatedly synchronizing those stable renderings, and hypothetical future ref
  methods do not satisfy decision rule 5. The repeated knowledge is isolated
  from ordinary checkpoint-format extension.
- **Rule discrimination:** Rule 5 usefully rules out 2 but is
  non-discriminating between 3 and 4 for repeated, stable protocol syntax. I
  score 3 because the repetitions are concrete, while keeping confidence
  Medium because their maintenance materiality is limited.

## Round 2 revalidation

I independently reapplied the simplified decision rules to only the requested
assignments. Scores below supersede the corresponding Round 1 judgments for
this revalidation.

| Component | Lens | Round 2 score | Confidence |
| --- | --- | ---: | --- |
| `fabro-http` | duplication-knowledge | 3 | High |
| `repository-ci` | ownership-boundaries | 2 | High |
| `fabro-checkpoint` | ownership-boundaries | 2 | High |
| `fabro-checkpoint` | simplicity | 3 | High |
| `fabro-checkpoint` | domain-model | 2 | High |
| `fabro-checkpoint` | duplication-knowledge | 3 | Medium |

### `fabro-http` × `duplication-knowledge`: 3 (High)

- **Decisive evidence:** `define_builder!` remains the one mechanism for the
  materially recurring async/blocking builder policy
  (`src/lib.rs:72-193`). The parser and `InvalidProxyPolicy` message still hold
  a concrete second representation of the allowed `"system"`/`"disabled"`
  vocabulary (`src/lib.rs:29-35,63-69`).
- **Adjacent scores:** 4 does not fit because revised rule 6 explicitly caps a
  concrete second semantic representation at 3. Score 2 does not fit because an
  ordinary mapped change does not currently synchronize separate async and
  blocking implementations; adding a future policy variant is not direct
  recurrence evidence.
- **Remaining ambiguity:** None material. Revised rule 6 now resolves the prior
  3-versus-4 uncertainty.

### `repository-ci` × `ownership-boundaries`: 2 (High)

- **Decisive evidence:** The Rust workflow's architecture check scans
  `apps`, `lib/packages`, and
  `docs/public/api-reference/fabro-api.yaml`
  (`.github/workflows/rust.yml:80-91`), but its push and pull-request triggers
  omit all three routine target paths (`rust.yml:6-19,22-35`). Its Cargo jobs
  also consume the real API specification through
  `lib/foundation/fabro-api/build.rs`, yet that specification does not trigger
  the workflow. The TypeScript workflow likewise consumes the generated API
  client and performs the embedded integration build without making the source
  specification a trigger. Under revised rule 3, each check owns this coverage;
  the omitted routine targets are therefore central ownership pressure.
- **Adjacent scores:** 3 does not fit because API, app, and package changes are
  routine targets of checks the workflow actually runs, not isolated edge
  inputs. Score 1 does not fit because Rust and TypeScript job ownership and
  dependency direction otherwise remain stable.
- **Remaining ambiguity:** None material. The nonexistent `openapi/**` value is
  still a separate domain-model finding; the ownership finding rests on the
  real scanned/consumed paths that fail to trigger.

### `fabro-checkpoint` × `ownership-boundaries`: 2 (High)

- **Decisive evidence:** The mapped higher owner is `BranchStore`, but the
  routine production metadata caller instead retains `Store`, branch, author,
  parent, and discovery state and reconstructs blob/tree/commit/ref lifecycle
  from `Store` primitives in
  `fabro-workflow/src/run_metadata.rs:272-451`. Revised rule 2 names this shape
  directly.
- **Adjacent scores:** 3 does not fit because reconstruction occurs on every
  metadata snapshot, not at an isolated edge. Score 1 does not fit because the
  low-level `Store` and workflow-level caller are stable, identifiable owners;
  the concern is the lifecycle split between them.
- **Remaining ambiguity:** The workflow reasonably owns remote authentication,
  but that does not remove its reconstruction of the mapped checkpoint and
  metadata-branch persistence lifecycle.

### `fabro-checkpoint` × `simplicity`: 3 (High)

- **Decisive evidence:** The current production `Store` write sequence is
  linear and direct (`src/git.rs:123-188`). `BranchStore` is a parallel mapped
  entry layer with no production caller at this revision, and `Cargo.toml:18`
  declares the unused production dependency `fabro-store`. Revised rule 4
  classifies exactly this as isolated simplicity friction that caps 4 at 3.
- **Adjacent scores:** 4 does not fit because the parallel unused layer and
  dependency are concrete. Score 2 does not fit because routine callers do not
  navigate competing paths or machinery; they consistently follow the direct
  `Store` path.
- **Remaining ambiguity:** None material after rule 4. `BranchStore` being a
  mapped entry does not make it frequent when the boundary search finds no
  production caller.

### `fabro-checkpoint` × `domain-model`: 2 (High)

- **Decisive evidence:** Mapped entry shapes accept unrestricted author and path
  strings: `GitAuthor::from_options` stores raw values before
  `BranchStore::new` interprets them with `Signature::now(...).expect(...)`
  (`src/author.rs:22-30`, `src/branch.rs:26-39`), and
  `TreeEntries`/`write_entry` carry unchecked string paths until Git-tree
  construction (`src/git.rs:46-90`, `src/branch.rs:84-109`). Revised rule 5
  says caller validation and a typed destination do not isolate this
  invalid-capable mapped entry.
- **Adjacent scores:** 3 does not fit because the invalid-capable shapes are on
  mapped entry paths, not a compatibility-only edge. Score 1 does not fit
  because the intended meanings of authors, paths, modes, and commits remain
  stable.
- **Remaining ambiguity:** None material. Normal callers supplying valid values
  does not make the entry type canonical by construction.

### `fabro-checkpoint` × `duplication-knowledge`: 3 (Medium)

- **Decisive evidence:** Bare branch names are independently rendered as
  `refs/heads/{branch}` in `Store::update_ref`, `resolve_ref`, and `delete_ref`
  (`src/git.rs:182-225`), and trailer lines are independently rendered in
  `trailer::append` and `format_message` (`src/trailer.rs:9-65`). These are
  concrete second semantic representations, so revised rule 6 excludes 4.
- **Adjacent scores:** 4 does not fit because the second renderings are real.
  Score 2 does not fit because no evidenced ordinary mapped change must
  synchronize the stable Git ref or trailer syntax across those locations;
  future ref operations are hypothetical, while the existing operations have
  distinct behavior.
- **Remaining ambiguity:** Limited ambiguity remains over whether stable
  protocol syntax is material enough to count as semantic repetition at all.
  Rule 6 does not define that threshold, so confidence remains Medium; if it
  counts, 3 is the rule-directed score.
