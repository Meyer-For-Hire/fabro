# Parallel fan-out / fan-in strategy

Status: proposed (not yet implemented). Line numbers reference the tree at the
time of writing and will drift.

This document specifies the intended product behavior of parallel fan-out
(`shape=component`) and fan-in (`shape=tripleoctagon`). Appendix A catalogs
pre-existing bugs this design resolves. Appendix B lists behavior changes
relative to today's implementation.

## 1. Introduction: goals and current weaknesses

The goal of this design is a parallel execution model that is **simple**,
**coherent**, and **correct**:

- **Simple** — a user should be able to predict what a fan-out/fan-in does
  from the graph alone. One mental model ("branches produce candidates; fan-in
  picks a commit; downstream sees all responses"), no new syntax for
  synthesis, no incantations (special fidelity settings, escape-hatch
  attributes) to make the flagship patterns work.
- **Coherent** — the same rules apply regardless of node type or channel.
  What holds for a sequential command node's output should hold for a branch
  command node's output; the workspace (git) facet and the text (context)
  facet should follow parallel logic; selection should live in exactly one
  place.
- **Correct** — the engine must do what the graph, the docs, and the recorded
  run state say it does. Nodes drawn in the graph must run; documented outputs
  must exist; a judge asked to pick the best candidate must be shown the
  candidates.

Today's implementation misses all three. Issue #490 is the visible symptom:
a synthesis node after fan-in never sees branch responses — only
`{id, status, head_sha}` metadata — while two tutorials promise the opposite.
Investigation showed a broader incoherence:

- **Inherited spec gap.** The Attractor spec (fabro's ancestor) deliberately
  isolates branch context and never defines a channel for branch output text.
  Its fan-in pseudocode judges candidates it cannot see (`llm_evaluate`
  receives only statuses) and sorts by a `score` field nothing sets. Fabro
  inherited this gap faithfully.
- **Missing compensation.** Kilroy (a sibling Attractor implementation)
  compensates with a file/git handoff: the post-merge node's prompt is
  injected with each branch's `worktree_dir`, `logs_root`, and `head_sha` plus
  instructions to read/merge them. Fabro has no equivalent, but its tutorials
  promise kilroy-like behavior ("the synth node receives all four perspectives
  in its preamble").
- **Accidental semantics.** Several adjacent behaviors are unprincipled
  accidents rather than decisions: branches execute a single node and silently
  skip chained nodes; two handlers fast-forward to two different definitions
  of "winner"; branch nodes run with a stale preamble built for the fan-out
  node; selection is vacuous in both modes; nested fan-outs silently lose git
  isolation. Appendix A catalogs these.

## 2. Core model: candidates

A parallel branch is an isolated unit of execution that produces a
**candidate**. A candidate has exactly three facets:

| Facet | Content | Carried by |
|---|---|---|
| Commit | Workspace state produced by the branch | Per-branch git commit (`head_sha`) |
| Response | Text output (LLM response, command output) | Branch stage records + context keys |
| Verdict | Terminal status plus optional numeric `score` | `parallel.results` entries |

Fan-out produces N candidates in isolation. Fan-in selects **one commit** to
continue on. Downstream nodes (synthesis) get **all responses**. Selection and
synthesis are distinct concerns: selection needs a node (`tripleoctagon`);
synthesis is any ordinary downstream node, because responses propagate.

The post-fan-in contract, in one sentence: **after fan-in, the run looks as if
every branch had run sequentially, and the winner ran last.**

## 3. Topology: branches are subgraphs

Each outgoing edge of a fan-out node starts a branch. A branch executes as a
subgraph walk: the engine traverses nodes and edges from the branch entry node
until it reaches the join node (the fan-in). Multi-node chains
(`fork -> plan_a; plan_a -> impl_a; impl_a -> merge`) run every node in the
chain. This matches the Attractor spec (`execute_subgraph`) and kilroy
(`runSubgraphUntil`); today fabro executes exactly one node per branch and
silently skips the rest (Appendix A.2).

Structural validation (lint):

- Every path from a branch entry must converge on the run's join node.
  A branch path that escapes the join (reaches exit, or a node outside the
  fan-out region) is a validation error.
- A nested `component` node inside a branch is rejected until
  worktree-from-worktree isolation is implemented (today it silently runs with
  no git isolation; Appendix A.6).
- `fidelity="full"` on a fan-in's outgoing edge gets a lint warning: branches
  run on different threads, so full fidelity can never carry branch outputs
  (it drops the preamble entirely).

## 4. Node types in branches

No type-based restrictions. Restriction is structural (§3), not by allowlist:

- **Agent / prompt nodes** — the primary case.
- **Command / script / tool nodes** — first-class. Deterministic fan-out
  (test matrices, benchmark bake-offs across worktrees) is a supported pattern
  with no LLM anywhere: branches emit `score` via status fields, heuristic
  selection picks the winner by measurement.
- **Conditionals** — meaningful under subgraph branches: they route *within*
  the branch.
- **Human gates** — allowed; each branch may pause independently.
- **Nested parallel** — rejected by lint until isolation composes (§3).

## 5. Git isolation

Unchanged mechanics, with ownership fixed:

1. Before fan-out, checkpoint the sandbox to produce `base_sha`.
2. Each branch gets a worktree on a branch ref
   (`fabro/run/parallel/<run>/<node>/pass<N>/<branch>`), rooted at `base_sha`.
   The branch's `internal.work_dir` points at the worktree.
3. After a branch completes, `git add -A` + commit (`--allow-empty`) yields the
   candidate's `head_sha`.
4. Worktrees are removed after the join. Loser branch refs are **kept** so
   downstream nodes and humans can `git show`/`git diff` any candidate.
5. **Fan-in exclusively owns the fast-forward.** The parallel handler performs
   no merge. After selection, fan-in fast-forwards the primary workspace to the
   winner's `head_sha`. (Today both handlers fast-forward, to potentially
   different winners; Appendix A.3.)

Degradation without git (no repo, or git isolation disabled): branches share
the primary sandbox with no workspace isolation, `head_sha` is absent from
candidates, and fan-in performs no merge. Response and verdict facets work
unchanged — prompt-only ensembles do not require git.

## 6. Execution and stage recording

Branch nodes execute as **real stages**, recorded through the normal
`ExecutionState::record` path and namespaced under the fan-out
(e.g. stage `a@1` within `fork@1`). Consequences (all fixes to current
behavior):

- Branch prompts/responses appear in events, `fabro dump`, and the web UI as
  ordinary stages.
- Each branch node gets a **freshly built preamble** for its own position, via
  the standard lifecycle, instead of reusing the fan-out node's stale preamble.
- Branch stages participate in the standard retry policy per node.

## 7. Context merge-back at fan-in

When fan-in completes, branch context updates are applied to the parent
context with a collision rule:

- **Per-node keys** (`response.<branch_node_id>`, structured-output fields
  namespaced by node) apply for **all** branches. Branch node IDs are unique,
  so no collisions.
- **Singleton keys** (`last_stage`, `last_response`, `command.output`,
  un-namespaced status fields) are taken from the **winner only**.
- Failed branches' `response.<id>` values are still applied (a synthesis node
  analyzing disagreement wants to see the failure text). Their singletons are
  never applied.

Fan-in additionally writes (as today):

- `parallel.results` — one entry per candidate: `{id, status, head_sha?,
  score?}`.
- `parallel.branch_count`, `parallel.fan_in.best_id`,
  `parallel.fan_in.best_outcome`, `parallel.fan_in.best_head_sha`.

No file is materialized into the run workspace. `parallel.results` reaches LLM
consumers through the preamble's context section, and agents can `git show`
any candidate via its `head_sha`. (The current docs claim
`parallel_results.json` is available to downstream nodes; that claim is false
today and should be corrected rather than implemented — see open question 4
for the one consumer this leaves unserved.)

## 8. Selection

Fan-in selects the winning candidate. Two modes, as today, but with real
signal:

- **Heuristic** (no prompt on the fan-in node): rank by status
  (succeeded < partially_succeeded < failed), then `score` descending, then
  lexical id. `score` becomes settable: branches emit it via structured-output
  / status fields, which now survive into `parallel.results` (§7).
- **LLM judge** (fan-in node has a `prompt` and a backend is configured): the
  judge prompt includes, per candidate: id, status, score, a bounded response
  excerpt, and `git diff --stat` vs. `base_sha` when git isolation is active.
  Today the judge sees only `{id, status, head_sha}` and cannot possibly
  discriminate (Appendix A.4).

Selection determines the commit facet only. It does not suppress loser
responses (§7) or loser refs (§5).

## 9. Downstream visibility (preamble)

Prompt templates render once at manifest build time with `{goal, inputs}`
only; the preamble is the sole channel for runtime context into a
fresh-session node. Therefore:

- At **`compact`** (default) fidelity, branch stage summaries render their
  responses **inline**, bounded, with a `See: <blob path>` reference when
  truncated — the same treatment `command.output` already receives at compact.
  Rationale: post-fan-in branch responses are unrecoverable through any
  fidelity setting (different threads), exactly like command output.
- The per-branch response budget is larger than command output's 25-line tail
  (ensemble analyses front-load their substance; a small tail amputates it).
  Exact budget TBD at implementation; must remain bounded so N long branches
  cannot blow the downstream context. Agent-type synthesis nodes can read the
  full text from the blob/artifact reference.
- `summary:high` renders the same with its larger budget. `truncate` carries
  goal only (explicit opt-out). `full` remains the degenerate case and lints
  (§3).

## 10. Join policies

- `wait_all` (default): all branches run to completion; join proceeds when all
  are terminal. Succeeds if no branch failed, else partially succeeds (fan-in
  fails only when *all* candidates failed).
- `first_success`: join proceeds at the first successful branch. Remaining
  branches are cancelled; cancelled branches record a terminal cancelled stage
  (their partial responses are not merged back). The sole successful branch is
  the winner.
- `k_of_n` / `quorum` (kilroy has them): deliberately **not** added now. The
  surface stays minimal until a concrete need appears.

## Open questions

1. Implementation phasing: subgraph branches (§3) are the largest lift.
   Response propagation (§6–§9) fixes #490 and both tutorials on its own and
   can ship first.
2. `first_success` cancellation semantics for in-flight agent sessions
   (graceful stop vs. abort; what the cancelled stage records).
3. Exact preamble budget per branch response (§9).
4. Context access for deterministic post-fan-in consumers. Command/script
   nodes have no channel to context (no preamble, no template rendering in
   `script`), so a deterministic aggregator after fan-in cannot learn
   candidate `head_sha`s. Candidate mechanisms: a results file under a
   checkpoint-excluded workspace path, or an env var (e.g.
   `FABRO_PARALLEL_RESULTS`) pointing at a file outside the workspace. A bare
   workspace file is ruled out: checkpoint commits `git add -A`, so it would
   leak into run history and PRs. Design alongside the deterministic fan-out
   pattern (§4).

---

## Appendix A: pre-existing bugs

Cataloged against the current tree; line numbers will drift.

1. **Branch outputs dropped (#490).** The fan-out task reads only
   `outcome.status` and `head_sha` from each branch; branch `context_updates`
   (including `response.<id>`) are discarded with the forked context
   (`fabro-workflow/src/handler/parallel.rs:389-397,465-470`). No channel
   carries branch text to downstream nodes. Confirmed by live repro on
   fabro-testing (run `01KX148ZAMMJRAADHK1HBF7PC3`, server 0.287.0-nightly.0).
2. **Chained branch nodes silently skipped.** Branches execute exactly one
   node; the engine then jumps to the join (`parallel.rs:388-397,612,628`;
   `fabro-core/src/executor.rs:421`). In `fork -> a; a -> a2; a2 -> merge`,
   `a2` never runs and nothing warns.
3. **Double fast-forward with two different winner definitions.** The parallel
   handler fast-forwards the *lexically first* successful branch
   (`parallel.rs:511-538`); fan-in then fast-forwards *its* selected winner
   (`fan_in.rs:120-131`). If selection ever picks a non-lexical-first branch,
   the second `--ff-only` merge cannot succeed (sibling commits diverge).
   Masked today only because selection is vacuous (A.4).
4. **Selection is vacuous.** Heuristic tie-breaks on a `score` field nothing
   can set (scores would arrive via branch context updates, which are dropped
   per A.1). The LLM judge prompt is `serde_json::to_string_pretty` of
   `parallel.results` — id/status/head_sha only (`fan_in.rs:247-250`). Both
   modes reduce to "first successful branch, alphabetically."
5. **Branch preambles are stale.** Branches run via `dispatch_handler`,
   bypassing the lifecycle's per-node preamble rebuild; each branch node
   inherits `current.preamble` as computed for the fan-out node itself.
6. **Nested parallel silently loses git isolation.** Branch `EngineServices`
   are built with `git_state: RwLock::new(None)` (`parallel.rs:380`), so a
   `component` node inside a branch runs its own branches with no worktrees
   and no warning.
7. **Docs contradict the engine.** `tutorials/ensemble.mdx:85` and
   `tutorials/parallel-review.mdx:81` claim the post-merge node receives all
   branch perspectives in its preamble (false, per A.1).
   `workflows/stages-and-nodes.mdx:195` claims merged results are available to
   downstream nodes as `parallel_results.json` (the file exists only under
   `stages/<fork>@1/` in dumps, not in any node's working directory).
8. **`fidelity="full"` across a fan-in is a trap.** It drops the preamble
   (metadata included) and cannot attach to any branch thread; raising
   fidelity strictly reduces what the downstream node sees. No lint warns.

## Appendix B: behavior changes vs. today

Changes a user could observe if this spec is implemented as written.

1. **Chained branch nodes execute.** Graphs that (unknowingly) relied on
   single-node branch semantics will now run the full chain (fixes A.2; may
   lengthen existing runs).
2. **New validation errors.** Branch paths that don't converge on the join,
   and nested `component` nodes inside branches, become lint failures for
   graphs that previously ran (with wrong or silently degraded semantics).
3. **Fan-in owns the fast-forward.** The workspace after fan-in may land on a
   different commit than today whenever selection (scores, LLM judge)
   disagrees with lexical-first order. The parallel handler no longer merges.
4. **Post-fan-in context is richer.** `response.<id>` for every branch,
   winner-sourced singletons (`last_stage`, `last_response`,
   `command.output`), and `score` in `parallel.results`. Today those
   singletons retain their pre-fork values; workflows with edge conditions
   over them could route differently.
5. **Preambles after fan-in grow.** Branch responses render inline at
   `compact` fidelity (bounded). Downstream nodes see more tokens per run;
   snapshot tests over preambles will churn.
6. **Branch executions become visible stages.** Events, dumps, the web UI, and
   the stage list gain per-branch stages (`a@1` under `fork@1`). Consumers of
   `events.jsonl` / the API will see new stage records.
7. **`parallel_results.json` docs claim is corrected, not implemented.**
   `workflows/stages-and-nodes.mdx:195` is updated to describe the real
   channels (context key + preamble); no file appears in the workspace
   (open question 4 covers deterministic consumers).
8. **Loser branch refs are documented as retained** and become part of the
   product contract instead of an accident of not deleting them.
9. **`first_success` cancels losers explicitly** and records cancelled stages;
   today's exact cancellation behavior is unspecified.
10. **LLM judge prompts change shape.** Fan-in nodes with prompts now send
    candidate excerpts and diff stats to the judge — more tokens, different
    (better) selections than today's id-only prompt.
