# Shared-checkout parallel execution strategy

Status: implemented.

This document defines Fabro's parallel fan-out (`shape=component`) and fan-in
(`shape=tripleoctagon`) behavior.

## 1. Execution model

A parallel node dispatches one branch for each outgoing edge. A branch executes
the single target node on that edge; parallel branches are not subgraph walks.
Every branch:

- receives an independent fork of the parent workflow context;
- receives the same `Arc<dyn Sandbox>` as the parent run;
- inherits the same sandbox working directory and `internal.work_dir`;
- runs through the normal handler dispatch path, including dry-run behavior;
- retains its branch identity, lifecycle events, and hook scope.

Branches execute concurrently. `max_parallel` limits the number that may run at
once and defaults to 4. The parallel node always waits for every branch task,
even when a branch fails or run cancellation begins. There is no early-success
join mode.

The parent context is not used as shared mutable branch state. A branch can
change its context fork without exposing those changes as top-level values to
other branches or to the parent.

## 2. Shared checkout

All branches use the run's existing sandbox and checkout. Parallel execution
creates no branch-specific:

- Git refs or branches;
- worktrees;
- base checkpoints;
- commits;
- cleanup operations;
- merges or fast-forwards.

Normal run-level checkpointing still occurs after the parallel node. Any files
left in the shared checkout by its branches are captured together by that
checkpoint.

Read-only parallel work is best effort: an agent or command can still write if
its configured capabilities permit it. Concurrent writes are allowed and are
entirely user-managed. Fabro does not lock files, enforce read-only access,
detect overlapping edits, or warn about races. Workflows that write in parallel
should coordinate externally or assign disjoint paths.

## 3. Branch results

The shared result type is:

```rust
ParallelBranchResult {
    id: String,
    status: String,
    context_updates: BTreeMap<String, serde_json::Value>,
}
```

The parallel handler stores one result per outgoing edge in
`parallel.results`. Results preserve outgoing-edge order, independent of branch
completion order. `parallel.branch_count` stores the number of dispatched
branches.

`context_updates` includes changes made in the branch context and updates
returned by the branch outcome. This applies to successful and failed branches,
including structured values, `response.<node_id>`, and `command.output`.
Engine-internal context keys are omitted. A task failure or panic cannot provide
updates that were never returned, but its result still preserves the original
branch ID and index.

Branch updates remain nested in their result. Fabro never merges them into the
parent's top-level context, so branches cannot collide through context keys.

The parallel stage outcome is:

- `succeeded` when every branch succeeds;
- `failed` when every branch fails;
- `partially_succeeded` for mixed outcomes, partial outcomes, and zero branches.

## 4. Artifacts and downstream context

Large context values use the normal artifact store. Offloading recursively
replaces oversized leaf values while retaining the object and array structure
of `parallel.results`.

When Fabro constructs execution or prompt context, it resolves nested textual
blob references under `response.*` and `command.output`, including those keys
inside a branch result's `context_updates`. This lets a prompted fan-in inspect
complete branch text without flattening branch state into the parent context.

`parallel.results` is runtime context. Fabro does not materialize a
`parallel_results.json` file in the workspace. Diagnostic run dumps may export
stage projection data, but that export is not a workflow handoff mechanism and
is not visible as a checkout file to downstream nodes.

## 5. Fan-in

Fan-in is an explicit join node.

A fan-in node without a nonblank prompt validates that `parallel.results`
exists and deserializes as typed branch results. It then succeeds with a joined
branches note. It is a no-op barrier: it does not alter context or workspace
state.

A fan-in node with a prompt delegates to the standard prompt handler. It sees
the aggregated runtime context in the normal prompt preamble and records the
normal prompt-stage outputs:

- `response.<fan_in_id>`;
- `last_response`;
- model usage and timing;
- prompt and response events.

A prompted fan-in synthesizes results. It does not rank branches, select a
winner, restore files, or choose workspace state.

## 6. Events and projections

Parallel execution emits:

- `parallel.started` with `visit` and `branch_count`;
- `parallel.branch.started` with stable branch identity and index;
- `parallel.branch.completed` with index, duration, and status;
- `parallel.completed` with counts and the ordered typed result array.

Every branch task emits one terminal branch completion event, including handler
failure, cancellation before semaphore acquisition, panic, or join failure.
The final typed array is also projected into
`StageProjection.parallel_results`.

## 7. Cancellation

Semaphore acquisition observes the run cancellation token. Branches waiting for
a permit can terminate as cancelled rather than waiting indefinitely. Branches
already executing continue through their handler's cooperative cancellation
path. The parallel handler joins every task before returning cancellation to the
run executor.

Cancellation does not trigger branch Git cleanup because no branch Git state is
created.

## 8. Product constraints

- Branches remain single-node executions.
- `max_parallel` remains supported.
- Results are deterministic in outgoing-edge order.
- There is no branch-selection score, SHA, model-usage mode, notice, or UI.
- Server-owned independent checkout/worktree behavior is separate and remains
  unchanged.
