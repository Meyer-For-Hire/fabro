# Chisel Quality Calibration

Calibration v1 · cartography v1 · revision `6bb6b5efcc0e36b52e3c097f532d9f2c00914c6c` · 2026-07-27T15:55:07Z
Sample: `fabro-workflow`, `fabro-http`, `fabro-web-app`, `repository-ci` · Control: `fabro-checkpoint` at `6bb6b5efcc0e36b52e3c097f532d9f2c00914c6c`
Evaluators: GPT-5 (Codex primary and independent reviewers)

## How to Use This Calibration

Judge each mapped component against its purpose and direct repository evidence.
Do not grade on a curve. Apply one score per lens, and count a finding under
only its primary lens.

**Isolated** means contained at an edge; normal callers and routine changes do
not encounter it. **Central** means part of a mapped entry point, common path,
or recurring change. A **routine change** is an ordinary extension or
maintenance task implied by the component's mapped purpose.

Infer routine work from the mapped purpose and traced common paths; a public
method alone does not establish frequency. A directly evidenced central concern
caps the component's lens score rather than being averaged against healthier
sub-responsibilities. Necessary delegation inside a clear owner is not pressure,
and size or internal busyness alone does not lower ownership.

Use **N/E** when evidence is insufficient. Never convert missing evidence into
a numeric score, and do not penalize a missing lifecycle path without evidence
that the mapped purpose requires it. Score 4 requires a positive production
mechanism and no material friction; tests may corroborate that mechanism but
cannot create it or become a second authority merely by asserting its contract.

## Lenses

### `ownership-boundaries` — Ownership and boundaries

**Does each responsibility and lifecycle have a clear home, with dependencies
pointing in the intended direction?** Includes responsibility, state, resource,
dependency, and lifecycle placement; excludes local control flow, naming,
types, API meaning, and repeated policy alone.

### `simplicity` — Simplicity

**Is the implementation no more complex, indirect, or general than necessary?**
Includes common-path traceability, control flow, indirection, abstraction, and
configuration burden; excludes placement, domain meaning, and independently
repeated knowledge.

### `domain-model` — Domain model

**Does each domain concept have one clear meaning and valid shape?** Includes
types, terminology, legal states, conversions, validation, and API semantics;
excludes module placement, lifecycle ownership, and repetition preserving one
meaning.

### `duplication-knowledge` — Duplication of knowledge

**Are policies, invariants, decisions, and transformations authoritative rather
than repeated?** Includes semantic repetition and manual synchronization;
excludes harmless syntax, coincidental similarity, and unification that would
create a parameterized mega-abstraction.

## Observable Anchors

| Score | Ownership and boundaries | Simplicity | Domain model | Duplication of knowledge |
|---:|---|---|---|---|
| 4 | One owner contains the mapped responsibility's state and complete lifecycle. | A production mechanism makes the necessary common path directly traceable. | Canonical types reject invalid states before every common-path interpretation. | One authoritative mechanism enforces each recurring policy, invariant, or transformation. |
| 3 | Ownership friction is isolated outside routine changes. | Unnecessary indirection is isolated outside routine changes. | Meaning or validation friction is isolated outside routine changes. | Repeated knowledge is isolated outside routine changes. |
| 2 | Routine changes coordinate competing owners or reverse the mapped dependency direction. | Routine changes repeatedly navigate competing paths, avoidable layers, or configuration machinery. | Routine changes reconcile recurring meanings, conversions, or invalid intermediate states. | Routine changes manually synchronize the same policy, invariant, or transformation across recurring locations. |
| 1 | No stable owner or dependency direction can be identified for the responsibility. | No stable common path can be traced through the implementation. | No stable meaning or legal shape can be identified for a core concept. | No stable authority can be identified for recurring domain knowledge. |

## Decision Rules

1. A directly evidenced central concern caps the component's lens score; do not average it against healthier sub-responsibilities.
2. Judge ownership against the map, not type names; when routine callers reconstruct a mapped lifecycle from low-level primitives, ownership fits 2.
3. A check owns trigger coverage for every path it scans; non-triggering routine targets are ownership pressure, while nonexistent selector values are domain-model pressure.
4. An unused production dependency or parallel entry layer is isolated simplicity friction, capping 4 at 3 when the common path remains direct.
5. Caller validation or a typed destination does not isolate an invalid-capable mapped entry; routine common-path use of that shape fits 2.
6. Concrete second semantic representations cap 4 at 3; score 2 only when an ordinary mapped change must synchronize them, not merely because call sites repeat.

## Confidence

Confidence describes evidence quality, not severity. **High** requires direct
evidence across relevant common and boundary paths; final High also requires
independent readings to converge. **Medium** has a material ambiguity or
coverage gap. **Low** is partial or substantially inferential.

## Classifying a Finding

- Where should this responsibility or lifecycle live? → `ownership-boundaries`
- Why is this much machinery necessary? → `simplicity`
- What does this name, type, state, or API value mean? → `domain-model`
- Why is this knowledge authoritative in several places? → `duplication-knowledge`

Tags are diagnostic metadata, not additional scores:

```text
abstraction-burden   boundary-leakage      configuration-sprawl
control-flow         conversion-sprawl     dependency-direction
generality           indirection           invalid-states
lifecycle            misplaced-responsibility
ownership            repeated-invariant    repeated-policy
repeated-test-knowledge                    repeated-transformation
state-coupling       type-sprawl           vocabulary-drift
```

## Repository Examples

### `ownership-boundaries`

- `lib/components/fabro-workflow/src/lifecycle/mod.rs:WorkflowLifecycle` shows a central orchestrator can own callback order through focused delegates; reviewers must still inspect terminal paths before calling lifecycle ownership contained.
- `apps/fabro-web/app/lib/api-client.ts:apiData` and `apps/fabro-web/app/lib/queries.ts:useRun` keep shared transport and read lifecycles out of route composition; a busy route alone is not boundary leakage.

### `simplicity`

- `lib/foundation/fabro-http/src/lib.rs:define_builder!` makes async and blocking construction traceable through one necessary mechanism; local macro indirection can reinforce simplicity.
- `lib/components/fabro-workflow/src/operations/start.rs:RunSession::run` exposes a linear phase sequence, while service reshaping across phase inputs shows that a stable path can still carry recurring machinery.

### `domain-model`

- `lib/components/fabro-workflow/src/event/events.rs:Event::StageCompleted` uses string status before `lib/components/fabro-workflow/src/event/convert.rs:stage_status_from_string` reparses it; a typed durable result does not isolate this common-path intermediate.
- `lib/foundation/fabro-http/src/lib.rs:ProxyPolicy` and `ProxyPolicy::resolve_with_env_value` demonstrate a closed policy vocabulary whose invalid boundary values are rejected.

### `duplication-knowledge`

- `lib/components/fabro-workflow/src/event/names.rs:event_name` and `lib/components/fabro-workflow/src/event/convert.rs:event_body_from_event` show manual mappings that a routine event extension must synchronize, even when exhaustive matches detect omissions.
- `.github/workflows/rust.yml:on.push.paths` and `.github/workflows/rust.yml:on.pull_request.paths` demonstrate duplicated trigger knowledge: one source-area change requires two manual policy edits.

## Control Baseline

`fabro-checkpoint` at `6bb6b5efcc0e36b52e3c097f532d9f2c00914c6c`:

| Lens | Score | Confidence |
|---|---:|---|
| Ownership and boundaries | 2 | High |
| Simplicity | 3 | High |
| Domain model | 2 | Medium |
| Duplication of knowledge | 3 | Medium |

## Recalibration Triggers

Recalibrate only for a rubric change, a material cartography change, a model
change with demonstrated drift, or inconsistent scores on the control sample.

## Open Questions

None.
