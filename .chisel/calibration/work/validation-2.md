# Chisel calibration validation 2

Revision reviewed: `6bb6b5efcc0e36b52e3c097f532d9f2c00914c6c`

This is an independent reading of the final rubric. I did not seek or infer
earlier scores.

## Scores

| Component | Lens | Score | Evidence confidence |
|---|---|---:|---|
| `fabro-workflow` | `ownership-boundaries` | 2 | High |
| `fabro-workflow` | `domain-model` | 2 | High |
| `fabro-http` | `domain-model` | 4 | High |
| `fabro-http` | `duplication-knowledge` | 4 | Medium |
| `fabro-web-app` | `ownership-boundaries` | 4 | Medium |
| `repository-ci` | `ownership-boundaries` | 2 | High |
| `repository-ci` | `domain-model` | 2 | High |
| `fabro-checkpoint` | `ownership-boundaries` | 4 | Medium |
| `fabro-checkpoint` | `simplicity` | 4 | Medium |
| `fabro-checkpoint` | `domain-model` | 3 | Medium |
| `fabro-checkpoint` | `duplication-knowledge` | 2 | Medium |

## Disputed assignments

### `fabro-workflow` × `ownership-boundaries` — 2

**Direct evidence.** `WorkflowLifecycle` is a real central owner for engine
callback ordering: it contains the event, hook, fidelity, status, circuit
breaker, git, and artifact delegates and orders them in every callback
(`src/lifecycle/mod.rs:53-80`, `223-470`). The full run lifecycle nevertheless
crosses that owner on normal paths. `WorkflowLifecycle::on_run_end` only runs
the hook (`src/lifecycle/mod.rs:467-469`); `pipeline::finalize` separately builds
and emits the terminal event and stops the sandbox
(`src/pipeline/finalize.rs:524-635`); `RunSession::run` separately owns
initialize/execute/finalize, progress flushing, steering drain, and a second
sandbox cleanup guard (`src/operations/start.rs:796-953`); detached bootstrap
and completion guards own additional terminal-failure paths
(`src/operations/start.rs:956-1139`). A routine change to terminal ordering or
cleanup must account for these owners.

**Strongest counterevidence.** The split is deliberate. In particular,
`finalize` documents why the terminal event must follow metadata flushing, and
the scope guards cover panic/interruption paths that an async lifecycle callback
cannot reliably cover.

**Why adjacent scores do not fit.** Score 3 does not fit because the split is on
every ordinary terminal path, not an isolated compatibility path. Score 1 does
not fit because the owners and dependency direction are identifiable:
`RunSession` is the outer orchestrator and `WorkflowLifecycle` consistently owns
engine callbacks.

**Rule discrimination.** Decision rule 2 is useful here, but “complete routine
lifecycle operations” must include terminal emission and resource cleanup, not
only engine callbacks. Without that reading, the positive orchestrator example
could make 3 and 2 hard to distinguish.

### `fabro-workflow` × `domain-model` — 2

**Direct evidence.** The canonical execution result is the typed
`StageOutcome`, re-exported in `src/outcome.rs:1-12`. The common stage-completion
event instead stores `status: String` (`src/event/events.rs:264-293`).
`EventLifecycle::after_node` converts the typed value to a string for every
successful completion (`src/lifecycle/event.rs:319-378`), and
`event_body_from_event` reparses it into `StageOutcome`
(`src/event/convert.rs:309-348`). Unknown strings are silently reinterpreted as
a non-retryable failure (`src/event/convert.rs:14-24`). The same string
intermediate is used for synthetic terminal stages
(`src/lifecycle/event.rs:183-242`).

**Strongest counterevidence.** Durable `fabro_types::StageCompletedProps` is
typed, and ordinary producers derive the string from a typed value rather than
accepting arbitrary user text.

**Why adjacent scores do not fit.** Score 3 does not fit because the conversion
and invalid intermediate occur on the common event path for every completed
stage. Score 1 does not fit because `StageOutcome` supplies a stable canonical
meaning and most execution code uses it directly.

**Rule discrimination.** Decision rule 4 and the repository example are
decisive. The rule would be non-discriminating if “compatibility escape hatch”
were allowed to describe the central `Event` type merely because the durable
type is healthier.

### `fabro-http` × `domain-model` — 4

**Direct evidence.** `ProxyPolicy` is a closed two-variant vocabulary
(`src/lib.rs:23-27`). The environment boundary parses case-insensitively and
rejects every other value with a typed `HttpClientBuildError`
(`src/lib.rs:29-70`). Explicit policy has a documented precedence in
`resolve_with_env_value`, and both async and blocking builders resolve the
policy immediately before applying it (`src/lib.rs:38-59`, `160-166`,
`172-193`). The common production and test constructors all pass through those
builders (`src/lib.rs:195-213`).

**Strongest counterevidence.** The builders also expose the lower-level
`no_proxy()` and `proxy()` methods (`src/lib.rs:96-106`), so callers can express
transport configuration outside the high-level enum.

**Why adjacent scores do not fit.** Score 3 does not fit because the lower-level
methods are intentional reqwest-facade escape hatches; the common constructors
and environment boundary do not rely on an invalid or ambiguous policy value.
There is positive production enforcement rather than a test-only contract.

**Rule discrimination.** Decision rule 4 discriminates well if “low-level
escape hatch” is read literally. If any alternate builder method were treated
as a second domain meaning, scores 3 and 4 would become difficult to distinguish
for facades.

### `fabro-http` × `duplication-knowledge` — 4

**Direct evidence.** `define_builder!` is one production mechanism for all
shared async/blocking builder methods and for applying proxy policy
(`src/lib.rs:72-170`); the two concrete builders are declarations of that
mechanism (`src/lib.rs:172-193`). `ProxyPolicy::resolve` is the single authority
for explicit-versus-environment precedence (`src/lib.rs:38-59`), and the four
convenience constructors delegate to the builders (`src/lib.rs:195-213`).
Workspace boundary evidence reinforces this authority: `clippy.toml` disallows
raw reqwest client constructors in favor of these functions/builders.

**Strongest counterevidence.** The tokens `system` and `disabled` also appear in
the human-readable error text, and the async/blocking test constructors repeat
the choice of `ProxyPolicy::Disabled`.

**Why adjacent scores do not fit.** Score 3 does not fit because the repeated
tokens and two one-line convenience constructors do not form independent
authorities for a recurring transformation. The macro and resolver are what
enforce behavior.

**Rule discrimination.** Decision rule 5 is useful but leaves a small judgment
gap around repeated diagnostic vocabulary. Here that repetition is
non-discriminating: adding a variant would make the exhaustive application
match fail to compile, while one diagnostic sentence is not a second policy
engine. This is why confidence is Medium rather than High.

### `fabro-web-app` × `ownership-boundaries` — 4

**Direct evidence.** Shared HTTP configuration, authentication redirect, and
error normalization live in `app/lib/api-client.ts:64-160,213-309`. Read state
and cache keys live in `app/lib/queries.ts` and
`app/lib/query-keys.ts`; for example, `useRun` owns the run-detail fetch/cache
lifecycle (`queries.ts:182-187`). Shared run mutations and their cache updates
live in `app/lib/mutations.ts:42-208`. Run SSE connection sharing, cleanup, and
cache invalidation live in `app/lib/sse.ts:42-189` and
`app/lib/run-events.ts:129-308`. Browser resources with more specialized
lifecycles are likewise contained: terminal WebSocket/xterm/listener cleanup is
in `app/hooks/use-terminal-session.ts:62-229`, and install polling owns its
timer, interval, and abort controller in
`app/hooks/use-install-effects.ts:72-127`.

`RunDetail` composes these owners and retains view-local state and interaction
ordering (`app/routes/run-detail.tsx:79-145,148-379`). Its size does not make it
the owner of transport or resource cleanup.

**Strongest counterevidence.** Several feature routes perform feature-local
create/edit/delete calls and SWR invalidation directly, and `RunDetail` owns the
delete dialog, pending state, toast, list invalidation, and navigation
(`run-detail.tsx:193-205`) rather than using a single mutation hook for that
entire interaction.

**Why adjacent scores do not fit.** Score 3 does not fit without a concrete
isolated lifecycle that has competing owners. The direct route mutations keep
their feature interaction lifecycle local and still use the shared transport;
they are not evidence that ordinary reads, SSE, or browser resources leak into
route composition.

**Rule discrimination.** The final repository example is discriminating:
“busy route” must not itself count as boundary leakage. Confidence remains
Medium because the application scope is broad, although the representative
read, mutation, live-update, terminal, install, and route boundaries converge.

### `repository-ci` × `ownership-boundaries` — 2

**Direct evidence.** The Rust workflow’s Clippy job owns a repository-wide
“legacy auth identity removal” guard that scans `lib/apps`, `lib/components`,
`lib/foundation`, `apps`, `lib/packages`, and the OpenAPI document
(`.github/workflows/rust.yml:80-91`). The workflow’s path filters do not include
`apps/**`, `lib/packages/**`, or
`docs/public/api-reference/fabro-api.yaml`
(`rust.yml:3-35`). A routine change in a scanned TypeScript/package/API path can
therefore introduce a forbidden identity without starting the job that owns the
guard. The policy lifecycle is placed under a narrower Rust trigger than the
responsibility it claims.

**Strongest counterevidence.** The primary Rust and TypeScript build/test
responsibilities otherwise have clear workflow homes, read-only permissions,
and stable concurrency ownership (`rust.yml:38-147`;
`typescript.yml:30-77`). The TypeScript production build’s Rust step is a
legitimate composition point because it builds the Rust binary with the
embedded SPA.

**Why adjacent scores do not fit.** Score 3 does not fit because the trigger
mismatch affects ordinary changes in multiple scanned source areas, not an
isolated maintenance path. Score 1 does not fit because the two main language
workflows and their jobs still have stable owners and dependency direction.

**Rule discrimination.** No final rule explicitly says how to classify a check
whose declared scan scope exceeds its trigger scope. The ownership lens’s
“complete lifecycle” language is sufficient, but an explicit trigger/target
coverage rule would make 2 versus 3 less ambiguous.

### `repository-ci` × `domain-model` — 2

**Direct evidence.** Every value in `.github/zizmor.yml` is a line-addressed
identifier: `rust.yml:37`, `rust.yml:49`, and `rust.yml:62`
(`.github/zizmor.yml:1-6`). At this revision those lines are respectively a
blank separator, the `fmt` job key, and a `run:` step—not action references.
Thus none is a current target for the configured `stale-action-refs` ignores.
Routine edits to `rust.yml` can change the accidental referents again without
changing the selectors.

**Strongest counterevidence.** The syntax still communicates an intended
workflow-and-line selector, and the main workflow job/status vocabulary is
otherwise stable.

**Why adjacent scores do not fit.** Score 3 does not fit because all three
values in the entire scoped zizmor configuration lack their intended current
referent; this is not one isolated compatibility value. Score 1 does not fit
because the selector format and intended concept remain identifiable even
though the instances are stale.

**Rule discrimination.** Decision rule 6 is decisive and correctly keeps this
under domain model rather than ownership. It would not by itself distinguish 2
from 3; the fact that every configured identifier is stale and line edits make
the condition recur supplies that distinction.

## Control: `fabro-checkpoint`

### `fabro-checkpoint` × `ownership-boundaries` — 4

**Direct evidence.** `git::Store` owns the `git2::Repository` and the low-level
blob/tree/commit/ref operations (`src/git.rs:101-227`).
`branch::BranchStore` owns branch identity, author identity, and the complete
local read-modify-write lifecycle, including parent resolution, tree read,
commit, and ref update (`src/branch.rs:17-82`). Author and trailer concerns are
focused modules rather than state hidden in callers (`src/author.rs`;
`src/trailer.rs`). Boundary evidence points in the intended direction:
`fabro-workflow` depends on these primitives, while its
`RunMetadataWriter` owns the additional temp repository, remote discovery,
credentials, push, and degradation lifecycle. That is a higher-level owner
using a lower-level delegate, not a reverse dependency.

**Strongest counterevidence.** The production metadata writer uses `Store`
directly and manually sequences blob, tree, commit, and ref operations
(`fabro-workflow/src/run_metadata.rs:313-361`) instead of using `BranchStore`.
The crate name/description can make that look like the mapped checkpoint
lifecycle has escaped the component.

**Why adjacent scores do not fit.** Score 3 does not fit if responsibilities are
classified by their actual state: `Store` owns local Git mechanics,
`BranchStore` owns local branch writes, and `RunMetadataWriter` owns remote run
metadata. No concrete resource is acquired by one of those owners and released
by another.

**Rule discrimination.** Decision rule 2 is ambiguous for intentionally
low-level facades. Passing a branch to `Store::update_ref` should not alone mean
“resupplying identity” when the caller owns the higher-level remote branch
lifecycle and `Store` never claimed it. If the mapped purpose is instead read
as all run-checkpoint lifecycle, this assignment could become 2; that purpose
boundary should be fixed before using the control for strict agreement.

### `fabro-checkpoint` × `simplicity` — 4

**Direct evidence.** The local branch write path is linear in
`BranchStore::write_with`: resolve parent, read tree, apply one caller mutation,
write tree, commit, update ref (`src/branch.rs:56-81`). Single-file,
multi-file, and delete operations are thin delegates to that path
(`src/branch.rs:84-117`). The lower-level tree conversion is one direct
flat-to-nested algorithm (`src/git.rs:229-309`), and trailer formatting/parsing
uses straightforward local control flow (`src/trailer.rs:9-87`).

**Strongest counterevidence.** `BranchStore` has no external production caller
at this revision; the actual metadata path uses the lower-level `Store` API.
There is also some unused-looking surface such as `MetadataError` and generic
branch read/list/log helpers.

**Why adjacent scores do not fit.** Score 3 does not fit because no direct
production evidence shows routine changes navigating the unused surface or
competing implementations. The production `Store` call sequence is itself
linear. The rubric explicitly says a public method alone does not establish
frequency, so unused API breadth cannot by itself create common-path
indirection.

**Rule discrimination.** The score-4 requirement for a “production mechanism”
is mildly ambiguous when the clearest high-level mechanism has no production
caller but its lower-level mechanism does. Treating compiled non-test code as
sufficient would make the rule non-discriminating; this score instead relies on
the directly used `Store` path also being traceable.

### `fabro-checkpoint` × `domain-model` — 3

**Direct evidence.** The common metadata boundary validates every path before
putting it into `TreeEntries`
(`fabro-workflow/src/run_metadata.rs:319-336,471-481`), explicitly selects
`FileMode::Blob`, and converts author strings with the fallible
`git2::Signature::now` before committing (`run_metadata.rs:337-345`). Within the
control, `FileMode` and `TreeEntries` give Git tree entries a stable meaning
(`src/git.rs:13-99`), and Git failures stay typed (`src/error.rs:3-32`).

There is nevertheless isolated model friction. `TreeEntries::set` accepts any
string path with no invariant-bearing path type (`src/git.rs:59-61`);
`FileMode::from_i32` maps every unrecognized Git mode to `Blob`
(`src/git.rs:30-35`); `GitAuthor` has public raw string fields
(`src/author.rs:6-11`); and `BranchStore` says trees grow monotonically while
also exposing `delete_entry` (`src/branch.rs:17-19,111-117`).

**Strongest counterevidence.** These are not merely hypothetical invalid
shapes: low-level public callers can bypass the production metadata-path
validation, and Git supports meaningful modes omitted by `FileMode`.

**Why adjacent scores do not fit.** Score 4 does not fit because the low-level
types themselves do not reject invalid paths/authors or preserve every Git
mode. Score 2 does not fit because the directly traced production metadata path
validates before interpretation and does not depend on the fallback
`from_i32`; the friction is in lower-level escape paths and the currently
unused `BranchStore`, not every common snapshot.

**Rule discrimination.** Decision rule 4 is useful but ambiguous about whether
a common caller validating raw values before a low-level API counts as a
“common-path invalid intermediate.” The rule should distinguish an actually
reparsed/ambiguous value from a raw value that has already passed one boundary
check but lacks an invariant-bearing Rust type.

### `fabro-checkpoint` × `duplication-knowledge` — 2

**Direct evidence.** The branch-name-to-full-ref transformation
`refs/heads/{branch}` is repeated independently in `Store::update_ref`,
`Store::resolve_ref`, and `Store::delete_ref`
(`src/git.rs:182-225`). The direct production boundary repeats it again in
`RunMetadataWriter::full_ref`
(`fabro-workflow/src/run_metadata.rs:425-439`). A routine addition or change to
branch ref handling must preserve the same transformation in each location.
The trailer grammar has a second, smaller recurrence: `": "` is independently
formatted, parsed, and detected in `append`, `parse`, `format_message`, and
`has_trailing_trailer_block` (`src/trailer.rs:11-12,28-40,45-59,68-86`).

**Strongest counterevidence.** Both grammars are tiny and stable, tests cover
the trailer forms, and the three Store methods currently agree. A helper could
look like cosmetic deduplication rather than a material abstraction.

**Why adjacent scores do not fit.** Score 3 does not fit because branch
resolution/update/deletion are ordinary Store operations and direct boundary
code already supplies a fourth recurrence; this is not only a hypothetical
future variant. Score 1 does not fit because the repeated transformations are
stable and readily identifiable even though they lack a single authority.

**Rule discrimination.** Decision rule 5 is decisive only if “direct evidence
of routine recurrence” includes several current operations applying the same
transformation. If it instead requires historical change evidence, the final
rule would be non-discriminating for a revision-only review and this assignment
would move toward 3.

## Round 2 revalidation

These scores supersede the corresponding Round 1 scores.

### `fabro-http` × `duplication-knowledge` — 3 (Medium)

**Decisive evidence.** `ProxyPolicy::parse` is the behavioral authority for the
external `system`/`disabled` vocabulary, while
`HttpClientBuildError::InvalidProxyPolicy` separately enumerates those values
in its diagnostic (`src/lib.rs:29-35,63-66`). The builder macro remains one
authority for applying the policy to both client kinds (`src/lib.rs:72-193`).

**Adjacent scores and ambiguity.** Score 4 does not fit because the diagnostic
is a concrete second representation that can drift. Score 2 does not fit
because proxy behavior is not independently reimplemented: the shared
resolver and macro enforce it, and the two no-proxy convenience constructors
are call sites rather than separate authorities (`src/lib.rs:195-213`). The
remaining ambiguity is whether changing the closed proxy vocabulary is routine
enough to make the diagnostic synchronization central; I treat it as isolated.

### `repository-ci` × `ownership-boundaries` — 2 (High)

**Decisive evidence.** The Rust workflow's legacy-auth check scans `apps`,
`lib/packages`, and `docs/public/api-reference/fabro-api.yaml`
(`rust.yml:80-91`), but its push and pull-request path filters omit all three
(`rust.yml:3-35`). Under decision rule 3, that check owns trigger coverage for
every path it scans, so routine changes in those targets bypass its lifecycle.

**Adjacent scores and ambiguity.** Score 3 does not fit because the missing
triggers affect several routine source and contract paths, not an isolated
edge. Score 1 does not fit because the Rust and TypeScript workflow owners and
dependency direction remain stable. No material ambiguity remains under the
new trigger-coverage rule.

### `fabro-checkpoint` × `ownership-boundaries` — 2 (High)

**Decisive evidence.** The map assigns checkpoint commits, trees, metadata
branches, authorship, and trailers to this component. The routine
`RunMetadataWriter` caller reconstructs that mapped lifecycle from `Store`
primitives: it writes blobs and a tree, creates the commit and author/message,
updates the ref, and pushes
(`fabro-workflow/src/run_metadata.rs:313-361`). Decision rule 2 therefore
places ownership at 2 even though the crate dependency points toward
`fabro-checkpoint`.

**Adjacent scores and ambiguity.** Score 3 does not fit because this is the
common metadata snapshot path, not an edge case. Score 1 does not fit because
the dependency direction and the low-level `Store` role are stable, and
`BranchStore::write_with` demonstrates a coherent lifecycle owner inside the
crate (`src/branch.rs:56-81`). The only remaining ambiguity is how specialized
the metadata commit is, but the map explicitly includes metadata branches.

### `fabro-checkpoint` × `simplicity` — 3 (High)

**Decisive evidence.** `fabro-store` and `serde` are production dependencies
with no source use (`Cargo.toml:16-24`), and `BranchStore` is a parallel
high-level entry layer with no production caller outside this crate. Decision
rule 4 makes those isolated simplicity frictions and caps 4 at 3.

**Adjacent scores and ambiguity.** Score 4 does not fit because the unused
production edges and parallel layer are concrete. Score 2 does not fit because
the production `Store` path remains direct; normal callers do not navigate the
unused dependencies or `BranchStore`. Whether `BranchStore` is retained for a
future caller is ambiguous, but the unused dependencies alone sustain 3.

### `fabro-checkpoint` × `domain-model` — 2 (Medium)

**Decisive evidence.** The mapped Git-tree entry accepts any `String` path
through `TreeEntries::set` (`src/git.rs:44-61`), while the routine metadata
writer must validate paths before constructing those entries
(`fabro-workflow/src/run_metadata.rs:319-336,471-481`). Decision rule 5 says
caller validation does not isolate an invalid-capable mapped entry.
`FileMode::from_i32` also collapses every unrecognized mode to `Blob`
(`src/git.rs:29-35`).

**Adjacent scores and ambiguity.** Score 3 does not fit because raw paths cross
the common write boundary. Score 1 does not fit because tree entries, modes,
and authors retain stable meanings and the caller does validate its input.
Confidence is Medium because a deliberately low-level Git store can reasonably
leave some path constraints to higher-level schemas, although the revised rule
weighs against that interpretation.

### `fabro-checkpoint` × `duplication-knowledge` — 3 (Medium)

**Decisive evidence.** The `refs/heads/{branch}` transformation is repeated in
three `Store` operations and once at the workflow boundary
(`src/git.rs:182-225`; `fabro-workflow/src/run_metadata.rs:425-439`).
Trailer formatting, parsing, and block detection also encode the `": "`
convention separately (`src/trailer.rs:11-12,28-40,45-59,68-86`). These are
concrete second representations, so decision rule 6 caps 4 at 3.

**Adjacent scores and ambiguity.** Score 2 does not fit on the current evidence:
adding a Store operation or trailer key may repeat a call-site convention, but
does not require an ordinary mapped change to modify all existing locations.
Score 4 does not fit because the representations are nevertheless concrete and
can drift. The remaining ambiguity is whether broader trailer-syntax support
would be routine maintenance; if so, its formatter/parser/detector
synchronization would support 2.
