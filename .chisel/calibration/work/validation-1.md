# Chisel calibration validation 1

Revision: `6bb6b5efcc0e36b52e3c097f532d9f2c00914c6c`

This is an independent reading of only the requested assignments. Scores use the
mapped purposes and the final calibration rubric. Boundary evidence is included
where it establishes whether a scoped mechanism is on a production common path.

## Summary

| Component | Lens | Score | Evidence confidence |
|---|---|---:|---|
| `fabro-workflow` | `ownership-boundaries` | 2 | High |
| `fabro-workflow` | `domain-model` | 2 | High |
| `fabro-http` | `domain-model` | 4 | High |
| `fabro-http` | `duplication-knowledge` | 4 | High |
| `fabro-web-app` | `ownership-boundaries` | 4 | Medium |
| `repository-ci` | `ownership-boundaries` | 4 | Medium |
| `repository-ci` | `domain-model` | 2 | High |
| `fabro-checkpoint` | `ownership-boundaries` | 2 | High |
| `fabro-checkpoint` | `simplicity` | 4 | Medium |
| `fabro-checkpoint` | `domain-model` | 2 | High |
| `fabro-checkpoint` | `duplication-knowledge` | 3 | Medium |

## `fabro-workflow`

### `ownership-boundaries`: 2

- **Evidence:** `lifecycle/mod.rs:53-80` presents `WorkflowLifecycle` as the
  callback owner, and `lifecycle/git.rs:77-93, 397-401` gives `GitLifecycle`
  its own `last_git_sha` state. The normal `RunSession::run` path nevertheless
  creates a second `last_git_sha`, reconstructs it by listening to emitted
  checkpoint, terminal, and Git events, then passes it back into finalization
  (`operations/start.rs:821-856, 914-923`). Terminal responsibility is split
  again: engine outcomes become terminal events in
  `pipeline/finalize.rs:524-596`, while bootstrap, initialization, and
  finalization errors become `run.failed` through the outer operation in
  `operations/start.rs:176-285, 288-346`. These crossings occur on the normal
  run and error paths, not at an optional edge.
- **Strongest counterevidence:** `operations/start.rs:796-953` is a recognizable
  top-level owner for the initialize → execute → finalize → pull-request
  sequence, and `WorkflowLifecycle` explicitly orders focused delegates for
  each executor callback (`lifecycle/mod.rs:221-469`).
- **Why adjacent scores do not fit:** 3 does not fit because the caller always
  mirrors and resupplies Git identity on the common run path, and terminal
  failure handling routinely selects between two owners. 1 does not fit because
  both the executor callback owner and the outer run-session owner are stable
  and traceable; the problem is their competition, not the absence of owners.
- **Rule discrimination:** Decision rule 2 is decisive for the mirrored
  `last_git_sha`. The phrase “complete lifecycle” is otherwise ambiguous about
  whether an executor lifecycle may end before durability finalization; the
  explicit state round-trip makes the result 2 without relying on that
  ambiguity.

### `domain-model`: 2

- **Evidence:** The internal durable event shape stores
  `Event::StageCompleted.status` as `String`
  (`event/events.rs:264-272`). Both synthetic terminal-stage completion and
  ordinary successful stage completion stringify the canonical
  `StageOutcome` (`lifecycle/event.rs:215-240, 355-366`), after which the
  mandatory event conversion reparses it and converts an unknown value to
  `Failed` (`event/convert.rs:14-24, 309-333`). This typed → string → typed path
  is part of every successful stage-completion event.
- **Strongest counterevidence:** `fabro_types::StageOutcome` is a stable
  canonical type, most event fields are typed, and the fallback prevents an
  unrecognized string from escaping into the stored projection.
- **Why adjacent scores do not fit:** 3 does not fit because common production
  completion events depend on the invalid intermediate rather than using it as
  a compatibility edge. 1 does not fit because the canonical status meaning is
  clear and the conversion point is explicit.
- **Rule discrimination:** Decision rule 4 and the rubric's repository example
  make this assignment unambiguous.

## `fabro-http`

### `domain-model`: 4

- **Evidence:** `ProxyPolicy` is a closed `System | Disabled` vocabulary;
  parsing rejects every other boundary value
  (`src/lib.rs:23-35`). Resolution gives explicit configuration precedence over
  the environment, defaults absence to `System`, and rejects non-Unicode input
  (`src/lib.rs:38-60`). Every async and blocking builder reaches that resolver
  before construction (`src/lib.rs:160-166, 172-193`), while the deterministic
  test helpers select the typed `Disabled` value
  (`src/lib.rs:195-213`).
- **Strongest counterevidence:** The builder also exposes raw `no_proxy()` and
  `proxy()` operations (`src/lib.rs:96-106`), so callers can combine an
  underlying reqwest choice with `ProxyPolicy`; Unix-socket production callers
  do use `no_proxy()` (`lib/foundation/fabro-client/src/client.rs:2123-2134`).
- **Why adjacent scores do not fit:** 3 does not fit because the common
  policy-controlled constructors never interpret an invalid policy: they
  return `HttpClientBuildError`. The raw builder operations represent valid
  per-client transport configuration, not a second string vocabulary. 2 and 1
  do not fit because no common-path conversion or unstable meaning is present.
- **Rule discrimination:** Decision rule 4 is potentially non-discriminating
  if every forwarded low-level builder method is called an “escape hatch.”
  Here `no_proxy()` carries no invalid intermediate and does not weaken
  `ProxyPolicy::resolve`, so treating it as ordinary typed builder
  configuration preserves the rule's distinction.

### `duplication-knowledge`: 4

- **Evidence:** `define_builder!` holds the complete shared async/blocking
  builder policy once, including proxy resolution and construction
  (`src/lib.rs:72-170`), and is instantiated for the two reqwest client kinds
  (`src/lib.rs:172-193`). The four convenience constructors delegate to those
  builders rather than reproducing policy (`src/lib.rs:195-213`).
- **Strongest counterevidence:** The generated facade necessarily lists each
  forwarded reqwest method, and the test and non-test convenience constructors
  have similar bodies.
- **Why adjacent scores do not fit:** 3 does not fit because the similar
  forwarding and wrappers are syntax over one policy authority, not separately
  maintained transport knowledge. 2 does not fit because a proxy-policy change
  is made once in the macro/resolver, not synchronized across async and
  blocking implementations. 1 does not fit because the authority is explicit.
- **Rule discrimination:** The rubric's `define_builder!` example directly
  distinguishes shared macro expansion from semantic duplication; no material
  ambiguity remains.

## `fabro-web-app`

### `ownership-boundaries`: 4

- **Evidence:** `entry.tsx:17-49` owns browser startup, chooses the normal or
  installation route graph once, and installs shared SWR runtime policy.
  `router.tsx:97-184` owns normal route composition. Shared transport and error
  handling live in `lib/api-client.ts:64-160, 213-310`; shared reads such as
  `useRun` and `useRunState` live in `lib/queries.ts:182-193`; run mutations and
  their cache lifecycle live in `lib/mutations.ts:65-132`; and run-scoped SSE
  subscription, invalidation, resync, and cleanup live in
  `lib/run-events.ts:129-309`. The representative busy route composes those
  owners rather than reimplementing them
  (`routes/run-detail.tsx:79-145, 313-379`).
- **Strongest counterevidence:** Some route-local CRUD actions call the shared
  API facade directly, and `run-detail.tsx:193-205` coordinates delete state,
  cache invalidation, toast, and navigation in the route.
- **Why adjacent scores do not fit:** 3 does not fit because the counterevidence
  is local page UX ownership; it does not split a shared transport, read,
  mutation, or subscription lifecycle. 2 does not fit because routine run-page
  changes use the established owners rather than coordinating competing ones.
  1 does not fit because startup, routing, transport, caching, and streaming
  each have readily identifiable homes.
- **Rule discrimination:** “One owner” is mildly non-discriminating for a large
  browser application unless responsibility is evaluated at lifecycle
  granularity. Using the rubric's `apiData`/`useRun` example, route composition
  is not itself a second owner. Confidence is Medium because this is the
  largest sampled scope.

## `repository-ci`

### `ownership-boundaries`: 4

- **Evidence:** `rust.yml:3-40` owns Rust branch/PR/manual triggers and
  concurrency, while its jobs contain format, lint, generated-doc, Linux test,
  twin E2E, and manual macOS lifecycles (`rust.yml:48-147`).
  `typescript.yml:3-34` owns the corresponding TypeScript triggers and
  concurrency, and its jobs contain typecheck, test, and integrated SPA/Rust
  build lifecycles (`typescript.yml:36-77`). Delegation to `cargo dev` is the
  mapped dependency on build tooling, not reverse ownership.
- **Strongest counterevidence:** The TypeScript build invokes a Rust build
  (`typescript.yml:75-77`), and invalid path selectors mean some intended
  changes do not start the declared workflows.
- **Why adjacent scores do not fit:** 3 does not fit because the cross-language
  build is the intentional embedded-SPA integration boundary, not friction, and
  selector validity is classified under domain model by decision rule 6. 2
  does not fit because no routine job requires coordination between competing
  CI owners. 1 does not fit because the two language validation homes and their
  dependency direction are explicit.
- **Rule discrimination:** The score-4 phrase “complete lifecycle” is
  non-discriminating for hosted CI if it is read to require repository
  ownership of GitHub's runner lifecycle. This score treats the checked-in
  trigger/job lifecycle as the mapped responsibility and the platform as an
  intended boundary.

### `domain-model`: 2

- **Evidence:** Both Rust trigger selectors name `openapi/**`
  (`rust.yml:18,34`), but that revision has no tracked target there; the actual
  API contract is `docs/public/api-reference/fabro-api.yaml`, which the
  TypeScript client generation command consumes
  (`lib/packages/fabro-api-client/package.json:7`). The real contract path is
  absent from both workflow path filters. In addition, all three zizmor
  `stale-action-refs` identifiers target `rust.yml:37`, `:49`, and `:62`
  (`zizmor.yml:1-6`), which are respectively the end of trigger setup, the
  `fmt` job key, and a `run` command—not action references at this revision.
  These invalid identifiers sit directly in trigger and static-validation
  configuration.
- **Strongest counterevidence:** The workflow/job vocabulary itself is stable,
  all jobs and action pins have clear meanings, and changes under the large
  valid Rust and TypeScript source selectors do trigger their expected suites.
- **Why adjacent scores do not fit:** 3 does not fit because the dead OpenAPI
  selector is present in both routine branch and PR paths, while every scoped
  zizmor exception lacks a current target. 1 does not fit because the overall
  workflow and job model remains stable; the defect is a recurring set of
  invalid identifiers.
- **Rule discrimination:** Decision rule 6 is decisive that these are domain
  pressure rather than ownership or duplication. It does not state when one or
  more dead selectors move from 3 to 2; centrality in both trigger modes and
  total staleness of the scoped zizmor selectors supply that discrimination
  here.

## Control: `fabro-checkpoint`

### `ownership-boundaries`: 2

- **Evidence:** The mapped component claims metadata branches, but its
  production boundary consumer owns the metadata writer's branch, parent OID,
  discovery, remote, and push lifecycle
  (`fabro-workflow/src/run_metadata.rs:272-282, 313-439`). On every snapshot,
  that caller validates entries, individually drives `Store` through blobs,
  tree, commit, and ref update, and retains the parent identity for the next
  write (`run_metadata.rs:313-350`). `BranchStore` provides a contained
  read-modify-write owner (`branch.rs:17-24, 42-81`) but has no production
  caller at this revision.
- **Strongest counterevidence:** The dependency direction is intended
  (`fabro-workflow` depends on `fabro-checkpoint`), and the low-level `Store`
  consistently owns Git object/ref operations (`git.rs:101-227`).
- **Why adjacent scores do not fit:** 3 does not fit because the lifecycle
  crossing occurs on every metadata snapshot, not in an isolated adapter. 1
  does not fit because low-level Git ownership and the caller's higher-level
  writer ownership are both stable; the problem is the split between them.
- **Rule discrimination:** Decision rule 2 applies because the caller retains
  and resupplies branch/parent identity to complete successive writes. The
  rubric does not say whether a deliberately low-level `Store` narrows the
  mapped ownership claim; the explicit mapped claim to metadata branches makes
  this crossing discriminating.

### `simplicity`: 4

- **Evidence:** The production `Store` has direct blob, tree, commit, and ref
  operations (`git.rs:123-226`). Tree conversion is a single read recursion and
  a single bottom-up write path (`git.rs:229-310`). At the higher level,
  `BranchStore::write_with` is a linear resolve → read → mutate → write → commit
  → update sequence (`branch.rs:56-81`), and entry operations are small
  delegates (`branch.rs:84-117`). Necessary Git layering is visible rather than
  hidden behind competing configuration machinery.
- **Strongest counterevidence:** There are two entry levels, and the production
  metadata writer uses the lower-level `Store` instead of `BranchStore`.
- **Why adjacent scores do not fit:** 3 does not fit because choosing the
  low-level entry is required for replace-whole-tree and remote-parent behavior,
  not unnecessary indirection. 2 does not fit because the scoped common
  operations do not navigate competing implementations or configuration. 1
  does not fit because both paths are directly traceable.
- **Rule discrimination:** Ownership rule 2 could otherwise cause the
  out-of-scope metadata writer's machinery to be counted again as simplicity
  friction. The lens exclusions make that non-discriminating evidence here;
  within the scoped implementation, the production primitives are direct.

### `domain-model`: 2

- **Evidence:** `TreeEntries::set` accepts any `String` path without validation
  (`git.rs:46-60`), and `write_tree` later interprets it by splitting on `/`
  (`git.rs:149-153, 270-293`). The common metadata caller must therefore define
  and apply `validate_metadata_path` outside this component before every
  `TreeEntries` construction
  (`fabro-workflow/src/run_metadata.rs:313-332, 471-480`). The component also
  maps every unrecognized Git file mode to `Blob`
  (`git.rs:21-35, 229-250`) rather than rejecting an unsupported state.
- **Strongest counterevidence:** `FileMode` is otherwise a closed enum, Git
  object IDs use `git2::Oid`, and the current production metadata caller does
  reject empty, absolute, dot-segment, and empty-segment paths before writing.
- **Why adjacent scores do not fit:** 3 does not fit because external path
  validation is mandatory on every common metadata snapshot and the canonical
  `TreeEntries` shape can always hold an invalid path. 1 does not fit because
  the intended path and mode meanings remain clear and production does have a
  validation step.
- **Rule discrimination:** Decision rule 4 clearly places the caller-validated
  `TreeEntries` intermediate at 2. Whether unknown Git modes are a compatibility
  escape hatch is ambiguous by itself, but it is not needed to choose the
  score.

### `duplication-knowledge`: 3

- **Evidence:** Branch-to-full-ref formatting is repeated in `Store::update_ref`,
  `resolve_ref`, and `delete_ref` (`git.rs:182-225`), and the boundary metadata
  writer has another `full_ref` transformation
  (`fabro-workflow/src/run_metadata.rs:364-439`). `BranchStore::read_entry`,
  `read_entries`, `list_entries`, and `tip_tree` also repeat parts of branch-tip
  resolution (`branch.rs:119-184`). These repetitions are local and stable, but
  there is no single helper enforcing them.
- **Strongest counterevidence:** Mutation sequencing is authoritative in
  `BranchStore::write_with` (`branch.rs:56-81`), metadata branch naming has one
  `META_BRANCH_PREFIX` constant (`lib.rs:7`), Git-author defaults have one
  `Default` implementation (`author.rs:13-20`), and the repeated ref syntax is a
  fixed Git protocol form rather than frequently changing Fabro policy.
- **Why adjacent scores do not fit:** 4 does not fit because ref normalization
  and branch-tip traversal are still represented in several places. 2 does not
  fit because there is no direct evidence that a routine checkpoint change
  must alter those stable protocol transformations in sync; the repetitions are
  isolated implementation knowledge. 1 does not fit because each policy has an
  identifiable local authority even where a helper is absent.
- **Rule discrimination:** Decision rule 5 leaves a real 3-versus-4 ambiguity:
  repeated `refs/heads/` can be classified as harmless protocol syntax. I score
  3 because the same branch-to-ref transformation crosses the component
  boundary, but do not score 2 without evidence of routine synchronization.

## Overall rubric observations

- Decision rule 2 successfully distinguishes focused delegates from a lifecycle
  that sends identity back through an event/caller round trip.
- Decision rule 6 prevents dead CI selectors from being double-counted as
  ownership defects, but needs centrality/recurrence evidence to distinguish 2
  from 3.
- “One owner” and “complete lifecycle” need responsibility-sized interpretation
  for route trees and hosted CI; otherwise healthy composition cannot reach 4.
- Decision rule 5 correctly keeps stable protocol repetition from automatically
  becoming score 2, but the line between harmless syntax and a repeated
  transformation remains the least discriminating part of this sample.

## Round 2 revalidation

| Component | Lens | Score | Confidence |
|---|---|---:|---|
| `fabro-http` | `duplication-knowledge` | 3 | Medium |
| `repository-ci` | `ownership-boundaries` | 2 | High |
| `fabro-checkpoint` | `ownership-boundaries` | 2 | High |
| `fabro-checkpoint` | `simplicity` | 3 | High |
| `fabro-checkpoint` | `domain-model` | 2 | High |
| `fabro-checkpoint` | `duplication-knowledge` | 3 | Medium |

### `fabro-http` × `duplication-knowledge`: 3

- **Decisive evidence:** Proxy disabling has two concrete semantic
  representations in the mapped entry layer: callers may set
  `ProxyPolicy::Disabled` (`src/lib.rs:23-27, 90-94`), or call the separately
  exposed `no_proxy()` builder operation (`src/lib.rs:96-100`). The former is
  interpreted by calling the same underlying `inner.no_proxy()` transformation
  during `build` (`src/lib.rs:160-165`). Both forms are used on direct boundary
  paths: test constructors select the enum (`src/lib.rs:199-213`), while the
  Unix-socket transport selects `no_proxy()`
  (`lib/foundation/fabro-client/src/client.rs:2123-2134`).
- **Adjacent scores:** 4 does not fit revised rule 6 because there is a concrete
  second representation of the same no-proxy decision. 2 does not fit because
  an ordinary proxy-policy extension does not require manually synchronizing
  those call sites; async and blocking policy construction still share the one
  `define_builder!` mechanism (`src/lib.rs:72-193`). 1 does not fit because the
  resolver remains a stable authority.
- **Remaining ambiguity:** `no_proxy()` can reasonably be viewed as a lower-level
  reqwest operation rather than a second Fabro policy. Revised rule 6 makes 3
  the conservative result because `ProxyPolicy::Disabled` is implemented by
  that exact operation, but this classification keeps confidence at Medium.

### `repository-ci` × `ownership-boundaries`: 2

- **Decisive evidence:** The Rust check explicitly scans
  `docs/public/api-reference/fabro-api.yaml` in its legacy-identity guard
  (`rust.yml:80-92`), but neither push nor pull-request triggers include that
  real path (`rust.yml:3-35`); they include the nonexistent `openapi/**`
  selector instead (`rust.yml:18,34`). A routine API-contract change can
  therefore change a scanned target without starting its owning check.
- **Adjacent scores:** 3 does not fit because the non-triggering target is on a
  routine branch/PR check path, not an isolated manual edge. 1 does not fit
  because the workflow, jobs, and intended trigger owner remain identifiable.
  4 is directly excluded by revised rule 3's trigger-coverage requirement.
- **Remaining ambiguity:** `typescript.yml:76` also invokes a Rust build from a
  narrower trigger set, but that broader interpretation is unnecessary; the
  explicitly scanned, non-triggering API contract is sufficient for 2.

### `fabro-checkpoint` × `ownership-boundaries`: 2

- **Decisive evidence:** The mapped owner exposes low-level `Store` primitives,
  while the routine metadata caller reconstructs the mapped branch lifecycle:
  `RunMetadataWriter` owns branch, parent, and discovery state
  (`fabro-workflow/src/run_metadata.rs:272-282`), then validates entries and
  sequences blob, tree, commit, ref update, and retained parent state on every
  snapshot (`run_metadata.rs:313-350`). No production boundary uses the
  component's higher-level `BranchStore`.
- **Adjacent scores:** 3 does not fit because every metadata snapshot traverses
  the split. 1 does not fit because the low-level Git owner and caller-side
  lifecycle are both stable. 4 is directly excluded by revised rule 2: the
  routine caller reconstructs a lifecycle the map assigns to this component.
- **Remaining ambiguity:** A narrower map that assigned only Git object
  primitives to `fabro-checkpoint` could make this healthy delegation, but the
  actual map explicitly assigns metadata branches and checkpoint commits.

### `fabro-checkpoint` × `simplicity`: 3

- **Decisive evidence:** `Cargo.toml:16-24` carries `fabro-store` as a production
  dependency, but scoped production code does not use it. The component also
  exposes `BranchStore` as a parallel entry layer (`branch.rs:17-24`) that has
  no production caller at this revision; the common metadata path uses `Store`
  directly. The active `Store` path itself remains linear and direct
  (`git.rs:123-226`).
- **Adjacent scores:** 4 is explicitly capped at 3 by revised rule 4 for the
  unused production dependency and parallel unused entry layer. 2 does not fit
  because routine production work does not repeatedly navigate those unused
  elements; its `Store` path is direct. 1 does not fit because a stable common
  path is easy to trace.
- **Remaining ambiguity:** Either isolated fact independently supplies the
  revised rule's cap, so there is no material score ambiguity.

### `fabro-checkpoint` × `domain-model`: 2

- **Decisive evidence:** `TreeEntries::set` accepts arbitrary string paths
  (`git.rs:46-60`) before `write_tree` interprets them structurally
  (`git.rs:149-153, 270-293`). Every common metadata snapshot must validate
  those paths outside the mapped entry before constructing `TreeEntries`
  (`fabro-workflow/src/run_metadata.rs:313-332, 471-480`).
- **Adjacent scores:** 3 does not fit revised rule 5 because caller validation
  does not isolate an invalid-capable mapped entry used on every snapshot. 1
  does not fit because path meaning is stable and the caller does enforce it.
  4 is excluded because the canonical entry type itself admits invalid states.
- **Remaining ambiguity:** Unknown Git modes also collapse to `Blob`
  (`git.rs:21-35`), but that compatibility question is not needed for the
  score; the routine path shape is decisive.

### `fabro-checkpoint` × `duplication-knowledge`: 3

- **Decisive evidence:** The short branch name is converted to
  `refs/heads/{branch}` independently in `Store::update_ref`, `resolve_ref`, and
  `delete_ref` (`git.rs:182-225`), while the routine boundary writer carries a
  second `full_ref` conversion
  (`fabro-workflow/src/run_metadata.rs:364-439`). These are concrete repeated
  representations, but of stable Git protocol knowledge.
- **Adjacent scores:** 4 does not fit revised rule 6 because the
  branch-to-full-ref transformation has a concrete second representation. 2
  does not fit because no ordinary mapped change is shown to require
  synchronizing the stable Git namespace transformations; repeated call sites
  alone are insufficient. 1 does not fit because the transformation and its
  local authorities are clear.
- **Remaining ambiguity:** The literal can also be classified as harmless Git
  syntax, which the lens excludes. Its repetition across the mapped boundary
  supports 3, but the harmless-syntax distinction keeps confidence at Medium.
