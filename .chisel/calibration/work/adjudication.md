# Calibration Adjudication

Revision: `6bb6b5efcc0e36b52e3c097f532d9f2c00914c6c`

Cartography: v1 at `2bcf94fed8a9b429f18d9196fa824711d6f4cb0a`.
The only later commit adds cartography artifacts, so the mapped code paths are
unchanged at the assessed revision.

Sample: `fabro-workflow`, `fabro-http`, `fabro-web-app`, `repository-ci`.
Control: `fabro-checkpoint`.

## Independent Score Matrix

Cells list reviewer 1 / reviewer 2 / reviewer 3.

| Component | Ownership and boundaries | Simplicity | Domain model | Duplication of knowledge |
|---|---:|---:|---:|---:|
| `fabro-workflow` | 2 / 3 / 4 | 2 / 2 / 2 | 2 / 3 / 2 | 2 / 2 / 2 |
| `fabro-http` | 4 / 4 / 4 | 4 / 4 / 4 | 4 / 3 / 4 | 3 / 4 / 4 |
| `fabro-web-app` | 4 / 4 / 3 | 2 / 2 / 2 | 2 / 2 / 2 | 2 / 2 / 2 |
| `repository-ci` | 4 / 4 / 3 | 3 / 3 / 3 | 2 / 3 / 2 | 2 / 2 / 2 |

Unanimous pairs establish that central machinery may still have a stable path:
`fabro-workflow` is 2 for simplicity and duplication; `fabro-web-app` is 2 for
simplicity, domain model, and duplication; and `repository-ci` is 3 for
simplicity and 2 for duplication. `fabro-http` is unanimously 4 for ownership
and simplicity.

## Material Disagreements

### `fabro-workflow` × ownership and boundaries — 2 / 3 / 4

- **Evidence:** `pipeline/mod.rs` and `pipeline/types.rs` give the normal run
  explicit phase owners; `lifecycle/mod.rs:WorkflowLifecycle` owns callback
  ordering through focused delegates.
- **Counterevidence:** terminal completion and failure are also constructed in
  `pipeline/finalize.rs:build_terminal_event`,
  `operations/start.rs:emit_workflow_run_failed`,
  `operations/start.rs:persist_terminal_engine_failure`, completion/drop
  guards, retry, and archive operations.
- **Ambiguous rule:** two reviewers judged the clear normal path; one judged
  whether the same lifecycle has one home across normal and exceptional paths.
- **Discriminator:** inspect every recurring terminal path. A routine
  terminal-contract change crossing several operation owners is score-2
  ownership pressure even when the success path is well partitioned.
- **Draft adjudication:** 2.

### `fabro-workflow` × domain model — 2 / 3 / 2

- **Evidence:** `pipeline/types.rs` encodes phase states and canonical product
  records are reused.
- **Counterevidence:** `event/events.rs:Event::StageCompleted` carries a string
  status; `lifecycle/event.rs:EventLifecycle::after_node` serializes a typed
  outcome and `event/convert.rs:stage_status_from_string` reparses it with an
  unknown-value fallback.
- **Ambiguous rule:** whether a typed durable event isolates an invalid
  intermediate representation on the common producer path.
- **Discriminator:** common-path invalid intermediate states are central even
  when the durable result is typed.
- **Draft adjudication:** 2.

### `fabro-http` × domain model — 4 / 3 / 4

- **Evidence:** `ProxyPolicy`, `resolve_with_env_value`, and
  `HttpClientBuildError` form a closed policy with explicit precedence and
  rejection.
- **Counterevidence:** public builders expose both
  `proxy_policy(ProxyPolicy::Disabled)` and lower-level `no_proxy()`.
- **Ambiguous rule:** whether a lower-level transport control creates a second
  meaning for the repository policy.
- **Discriminator:** an escape hatch does not split the canonical concept when
  the typed policy remains closed and its precedence is enforced.
- **Draft adjudication:** 4.

### `fabro-http` × duplication of knowledge — 3 / 4 / 4

- **Evidence:** `define_builder!` is the shared async/blocking authority and
  `ProxyPolicy::resolve` owns precedence.
- **Counterevidence:** adding a policy variant synchronizes the enum, parser,
  expected-value error text, behavior match, and tests.
- **Ambiguous rule:** whether co-location and exhaustive matching make all
  policy vocabulary authoritative.
- **Discriminator:** hypothetical variants do not establish routine
  recurrence; exhaustive compiler-checked behavior remains one authority
  unless direct evidence shows recurring manual synchronization.
- **Draft adjudication:** 4.

### `fabro-web-app` × ownership and boundaries — 4 / 4 / 3

- **Evidence:** `entry.tsx`, route graphs, `lib/api-client.ts`, queries,
  mutations, effect hooks, and the build script give shared responsibilities
  visible homes.
- **Counterevidence:** `install-app.tsx` and `routes/run-stages.tsx` contain
  several central transformations and presentation concerns.
- **Ambiguous rule:** whether a busy but clearly identified route owner is
  boundary pressure or simplicity pressure.
- **Discriminator:** do not lower ownership for internal complexity unless
  routine changes cross another owner or reverse the mapped dependency
  direction.
- **Draft adjudication:** 4.

### `repository-ci` × ownership and boundaries — 4 / 4 / 3

- **Evidence:** Rust and TypeScript workflows have distinct validation jobs,
  narrow permissions, and delegate build procedures to repository commands.
- **Counterevidence:** the Rust clippy job embeds the repository's legacy-auth
  vocabulary check.
- **Ambiguous rule:** whether enforcement of a product migration invariant is
  misplaced when CI owns validation but not the underlying vocabulary.
- **Discriminator:** a named invariant check may live in CI, but its product
  vocabulary must remain authoritative elsewhere; this isolated boundary
  friction fits 3.
- **Draft adjudication:** 3.

### `repository-ci` × domain model — 2 / 3 / 2

- **Evidence:** job, runner, permission, and test-mode vocabulary is otherwise
  coherent.
- **Counterevidence:** `rust.yml:on.*.paths` names nonexistent `openapi/**`
  rather than `docs/public/api-reference/fabro-api.yaml`, and
  `zizmor.yml:rules.stale-action-refs.ignore` identifies exceptions by stale
  line positions.
- **Ambiguous rule:** whether configuration references are domain vocabulary
  or only duplicated operational data.
- **Discriminator:** identifiers that control central behavior are domain
  vocabulary; missing or stale referents create score-2 pressure.
- **Draft adjudication:** 2.

## Draft Anchor Decisions

- Anchor score 4 on a positive enforcing mechanism, never absence of a defect.
- Separate owner clarity from the amount of machinery inside that owner.
- Treat invalid common-path intermediate states as domain-model pressure.
- Treat repeated semantic decisions as duplication only when routine changes
  require manual synchronization.
- Treat mapped configuration identifiers as domain vocabulary.
- Reserve N/E for a lens without direct evidence; no sampled pair required it.

## Consistency Review

The fresh reviewer applied only the written draft to `fabro-checkpoint` and
reported:

| Lens | Score | Evidence confidence |
|---|---:|---|
| Ownership and boundaries | 2 | Medium |
| Simplicity | 3 | High |
| Domain model | 2 | High |
| Duplication of knowledge | 2 | High |

The control exposed four material wording problems:

1. The draft did not say how a component-level score combines several
   responsibilities, or whether positive mechanisms and friction can coexist
   at score 4.
2. Necessary layered delegation could satisfy the original ownership and
   simplicity score-2 wording.
3. The domain rules did not say when a public low-level API is an escape hatch
   or what score a common invalid intermediate implies.
4. Decision rule 5 contradicted the duplication anchor by assigning routine
   string synchronization to score 3.

The revision now says that a central concern caps rather than averages, score 4
requires a positive production mechanism without material friction, public
surface alone does not establish routine work, and missing paths are not
negative without mapped-purpose evidence. The anchors now distinguish competing
owners from necessary delegation and maintainer navigation from runtime
layering. Decision rules 2–6 resolve scoped lifecycle handoff, necessary
delegation, common-path invalid states, direct evidence of recurring
synchronization, and configuration identifiers. Tests corroborate production
authorities but are not second authorities merely because they restate a
contract.

All 16 wording observations in `consistency-review.md` are covered by those
changes or by the existing primary-lens and confidence sections. No consistency
objection remains open before validation.

## Validation

### Round 1

| Assignment | Validator 1 | Validator 2 | Validator 3 | Result |
|---|---:|---:|---:|---|
| `fabro-workflow` × ownership | 2 | 2 | 2 | Resolved |
| `fabro-workflow` × domain | 2 | 2 | 2 | Resolved |
| `fabro-http` × domain | 4 | 4 | 4 | Resolved |
| `fabro-http` × duplication | 4 | 4 | 3 | Repeated adjacent split |
| `fabro-web-app` × ownership | 4 | 4 | 4 | Resolved |
| `repository-ci` × ownership | 4 | 2 | 4 | Non-adjacent split |
| `repository-ci` × domain | 2 | 2 | 2 | Resolved |
| Control × ownership | 2 | 4 | 2 | Non-adjacent split |
| Control × simplicity | 4 | 4 | 3 | Adjacent split |
| Control × domain | 2 | 3 | 2 | Adjacent split |
| Control × duplication | 3 | 2 | 3 | Adjacent split |

The sample's workflow lifecycle, event status, HTTP policy model, web
composition, and CI identifier anchors now converge. Six assignments require
the permitted final simplification:

- HTTP diagnostic allowed-value text is a concrete second semantic
  representation, even though the macro is the behavioral authority.
- A CI check owns trigger coverage for every path its embedded policy scans;
  this is distinct from the domain meaning of a nonexistent selector.
- Control ownership is judged against the mapped metadata-branch purpose, not
  against narrower names on `Store` and `BranchStore`.
- The control's unused dependency and unused parallel entry layer are isolated
  simplicity friction rather than evidence-free public breadth.
- Validation in an external caller does not make an invalid-capable mapped
  entry type enforce its own legal shape.
- Repeated fixed Git protocol syntax is a concrete second representation, but
  multiple current call sites alone do not make changing that protocol an
  ordinary mapped change.

Decision rules 2–6 now state those discriminators directly. Round 2 will
re-score only the six unresolved assignments.

### Round 2

| Assignment | Validator 1 | Validator 2 | Validator 3 | Result |
|---|---:|---:|---:|---|
| `fabro-http` × duplication | 3 | 3 | 3 | Resolved |
| `repository-ci` × ownership | 2 | 2 | 2 | Resolved |
| Control × ownership | 2 | 2 | 2 | Resolved |
| Control × simplicity | 3 | 3 | 3 | Resolved |
| Control × domain | 2 | 2 | 2 | Resolved |
| Control × duplication | 3 | 3 | 3 | Resolved |

All round-2 scores converge. The final control baseline is ownership 2
(High), simplicity 3 (High), domain model 2 (Medium), and duplication of
knowledge 3 (Medium). Domain confidence remains Medium because one validator
found a material ambiguity over whether low-level Git path validation belongs
inside the component. Duplication confidence remains Medium because stable
protocol syntax is concrete repetition but has limited demonstrated change
burden.

Across both validation rounds, the final disputed sample scores are:

| Component | Ownership and boundaries | Domain model | Duplication of knowledge |
|---|---:|---:|---:|
| `fabro-workflow` | 2 | 2 | — |
| `fabro-http` | — | 4 | 3 |
| `fabro-web-app` | 4 | — | — |
| `repository-ci` | 2 | 2 | — |

No non-adjacent or repeated adjacent split remains.

## Open Questions

None.
