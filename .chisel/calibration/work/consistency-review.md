# Chisel Consistency Review: `fabro-checkpoint`

Revision: `6bb6b5efcc0e36b52e3c097f532d9f2c00914c6c`

Scope: `lib/components/fabro-checkpoint/**` only. The scored evidence is the manifest, production source, and unit tests at the pinned revision. I did not inspect callers, sample reviews, adjudication, or any other file under `.chisel/calibration/work/`.

## Scores

| Lens | Score | Confidence |
|---|---:|---|
| `ownership-boundaries` | 2 | Medium |
| `simplicity` | 3 | High |
| `domain-model` | 2 | High |
| `duplication-knowledge` | 2 | High |

Confidence here describes this reading's evidence quality. The rubric's additional requirement that a final High confidence needs independent convergence can only be decided during adjudication.

## `ownership-boundaries`: 2

The central branch lifecycle crosses two public owners. `BranchStore` stores the branch name and owns bootstrap plus normal branch reads and writes (`branch.rs:20-209`), but branch cleanup is exposed only as `Store::delete_ref(branch)` (`git.rs:215-226`). `BranchStore` keeps both its `Store` reference and branch name private and has no cleanup/archive operation. A caller therefore has to retain the same raw branch identity and leave the branch-scoped interface for cleanup. Bootstrap sequencing is also caller-owned: `BranchStore::new` does not establish the branch, writes fail when it is absent, and every writable test explicitly calls `ensure_branch` first (`branch.rs:26-81, 282-343`). This is recurring lifecycle work rather than an isolated edge, especially under decision rule 1. Primary tags: `lifecycle`, `ownership`.

Strongest counterevidence: once initialized, `BranchStore::write_with` keeps the read-modify-write sequence together and delegates only Git object/ref primitives to `Store` (`branch.rs:56-82`). The dependency direction is stable: branch storage depends on the lower-level Git store, not vice versa.

Why adjacent scores do not fit:

- **1 does not fit:** `BranchStore` is a stable, identifiable owner for the common branch-scoped read/write responsibility, and `Store` is a coherent lower-level Git owner.
- **3 does not fit:** the split includes explicit bootstrap and cleanup paths. Decision rule 1 says recurring terminal ownership cannot be treated as isolated merely because the success path is clear.

Confidence is Medium because the split is direct, but the scoped evidence cannot show whether archive, retry, and cleanup are deliberately owned by a higher-level caller.

## `simplicity`: 3

The common write path is directly traceable: `write_entry`/`write_entries` prepare blobs, `write_with` reads the tip tree, applies one mutation, writes one commit, and advances one ref (`branch.rs:56-109`). `Store::read_tree` and `Store::write_tree` use a single flat `TreeEntries` representation with private recursive helpers (`git.rs:39-99, 141-159, 229-310`). These are positive reinforcing mechanisms, not just an absence of complexity.

The remaining simplicity pressure is isolated configuration burden. The manifest declares `fabro-store`, `serde`, and the dev dependency `chrono` (`Cargo.toml:16-28`), but none is referenced anywhere in the component source or tests at this revision. The public `Store::repo` escape hatch (`git.rs:112-114`) and the lower-level object API also add surface area, but normal branch writes do not have to choose among competing implementations. Primary tag: `configuration-sprawl`.

Strongest counterevidence to lowering the score: the component has one linear common mutation path, and its indirection corresponds directly to Git's blob/tree/commit/ref structure.

Why adjacent scores do not fit:

- **2 does not fit:** ordinary reads and writes do not repeatedly traverse competing orchestration paths or configuration machinery; the `BranchStore` to `Store` layering is stable and direct.
- **4 does not fit:** the centralized mutation path is a qualifying positive mechanism, but the unused manifest dependencies are concrete unnecessary configuration rather than necessary machinery.

Confidence is High because all component files are in scope, so the dependency non-use and the full common write path are directly observable.

## `domain-model`: 2

The common tree-entry producer accepts invalid intermediate path states. `TreeEntries` hides its map, but its public `set` accepts any `Into<String>` without validating a relative Git path (`git.rs:46-61`). Both `BranchStore::write_entry` and `write_entries` feed caller-provided `&str` paths directly into it (`branch.rs:84-109`), and `build_dir_node` later assigns meaning by splitting the strings on `/` (`git.rs:270-294`). Empty components, leading/trailing separators, and file/directory prefix collisions are therefore representable in the canonical intermediate type and reach late Git-tree construction rather than being rejected at the common boundary. Branch identity is likewise an arbitrary `String` until `git2` receives the synthesized ref name (`branch.rs:20-38`, `git.rs:182-197`). This is central invalid-state pressure under decision rule 3, not an isolated low-level escape hatch. Primary tag: `invalid-states`.

The small helper `sharded_path` is corroborating boundary evidence: its contract says the input is a hex ID, but its public signature accepts any `&str` and slices at a caller-provided byte offset (`branch.rs:211-220`), so a non-ASCII input can panic rather than be rejected as invalid input.

Strongest counterevidence: `FileMode` is a closed enum and `TreeEntries` keeps ordering and representation private (`git.rs:13-99`). `Error` also distinguishes a missing branch from generic Git failures (`error.rs:5-18`). The component therefore has stable concepts even though common constructors do not preserve all their invariants.

Why adjacent scores do not fit:

- **1 does not fit:** branch storage, tree entries, file modes, authors, and trailers all have recognizable, stable meanings.
- **3 does not fit:** raw paths and branch names enter the common public read/write boundary, so validation friction is not isolated outside routine use.

Confidence is High because the accepting producers and their downstream interpretation are both visible within the scoped common path.

## `duplication-knowledge`: 2

The transformation “find a path in a commit tree, treat only `NotFound` as absence, load the entry as a blob, and copy its bytes” is independently implemented by `BranchStore::read_entry`, `BranchStore::read_entries`, and `Store::read_blob_at` (`branch.rs:119-158`, `git.rs:200-213`). An ordinary maintenance change to missing-entry or entry-kind behavior must synchronize all three common read locations. Ref qualification is also repeated in `update_ref`, `resolve_ref`, and `delete_ref` (`git.rs:182-226`).

Trailer grammar supplies independent corroboration at the commit-message edge: `": "` formatting/detection is separately encoded by `append`, `parse`, `format_message`, and `has_trailing_trailer_block` (`trailer.rs:9-25, 28-42, 45-65, 68-87`). Primary tags: `repeated-transformation`, `repeated-policy`.

Strongest counterevidence: important write knowledge is authoritative. `BranchStore::write_with` centralizes tip loading, parent linkage, commit creation, and ref advancement, while `GitAuthor::default` centralizes the fallback identity (`branch.rs:56-82`, `author.rs:13-35`).

Why adjacent scores do not fit:

- **1 does not fit:** the repeated implementations currently agree, and stable authorities exist for branch mutation, author defaults, and file-mode conversion.
- **3 does not fit:** the repeated blob-read transformation appears on the public latest-entry and multi-entry common paths, so a routine storage-policy change encounters it centrally rather than only at an edge.

Confidence is High because the repeated transformations and the mechanisms that are already centralized can both be enumerated completely inside the scoped component.

## Rubric wording audit

The following rules or anchors were ambiguous or non-discriminating in this application. I resolved each explicitly rather than silently choosing an interpretation.

1. **One component score across several responsibilities.** The instruction says to judge “each mapped component,” while the anchors use singular phrases such as “a mapped responsibility” and “a core concept.” It does not say whether to average sub-responsibilities, take the worst concern, or weight by centrality. I scored the mapped checkpoint-storage responsibility and let a directly evidenced central concern cap the lens; isolated author/trailer helpers could affect a score only at 3 versus 4.

2. **How to establish “routine” and “central” with component-only evidence.** A public method may be a mapped entry point without being frequent, and scoped evidence cannot establish caller frequency. I treated bootstrap, latest reads/writes, and cleanup as routine because they are ordinary lifecycle operations implied by branch storage. I did not infer frequency for unrelated external call sites.

3. **N/E threshold versus an absent lifecycle path.** “Use N/E when evidence is insufficient” does not say whether a missing archive/retry API is negative evidence, out of scope, or grounds for N/E. I scored paths that are directly present (bootstrap, normal operation, cleanup), did not penalize an unobserved archive/retry design, and lowered ownership confidence for the coverage gap.

4. **Score 3 and score 4 overlap in every lens.** A positive reinforcing mechanism can coexist with isolated friction, so the score-4 requirement and score-3 anchor can both be true. I treated any evidenced unnecessary/frictional mechanism as a cap at 3; score 4 requires both a positive mechanism and no material friction in the mapped responsibility. This is why the unused manifest dependencies keep simplicity at 3 despite `write_with`.

5. **What qualifies as a “positive reinforcing mechanism.”** The rubric does not say whether tests, encapsulation alone, or a production authority qualifies. I required an operative production mechanism that funnels behavior or rejects invalid construction. Tests alone did not qualify.

6. **Ownership score 2 versus ordinary delegation.** “Cross recurring owners or dependency boundaries” could penalize every layered implementation. Decision rule 2 partly resolves this, but “same responsibility” remains subjective. I treated `BranchStore` calling `Store` during a write as ordinary delegation; I counted cleanup only because the caller must leave the branch-scoped owner and supply its identity again.

7. **Decision rule 1 when terminal operations live at a lower abstraction.** The rule says not to isolate recurring terminal owners but does not define whether a lower-level deletion primitive is a second owner or a delegate. Because `BranchStore` offers no cleanup interface and keeps the needed state private, I treated `Store::delete_ref` as a lifecycle-owner crossing, not merely internal machinery.

8. **Simplicity score 2’s “repeatedly traverse.”** It is unclear whether this means runtime calls passing through multiple necessary layers, or maintainers choosing among competing paths repeatedly. I used the latter interpretation, consistent with the lens question and decision rule 2; necessary Git layers did not lower the score.

9. **Decision rule 2’s “simplicity pressure.”** The rule labels machinery inside an owner as pressure even though the lens expressly permits necessary complexity and gives no score consequence for “pressure.” I treated machinery as evidence to test for necessity, not as an automatic deduction.

10. **Domain score 4 versus decision rule 4’s escape hatch.** “Every common boundary” is not defined, and a public low-level API can be called common or an escape hatch depending on external usage. I treated `TreeEntries::set` as common because `BranchStore::write_with`, `write_entry`, and `write_entries` use it directly; `Store::repo` was treated as an escape hatch.

11. **Decision rule 3 does not identify a score boundary.** It says a typed durable value does not “repair domain pressure,” but does not say whether a common invalid intermediate means 2 or merely prevents 4. I mapped common-path invalid intermediates to the score-2 anchor (“routine changes reconcile ... invalid intermediate states”); isolated invalid intermediates would map to 3.

12. **Duplication score 2 versus decision rule 5.** Rule 5 says to score 3 when a routine vocabulary change requires synchronization, while the score-2 anchor says routine synchronization of the same policy/invariant/transformation is score 2. Those statements conflict unless “vocabulary” is an unstated special case. I treated rule 5 narrowly as an exception for localized, string-only vocabulary at an edge. The score-2 finding here rests instead on repeated behavioral blob-read transformations on common paths.

13. **What test repetition counts as knowledge duplication.** The `repeated-test-knowledge` tag suggests tests can count, but the anchors do not distinguish duplicated policy from assertions that intentionally restate expected behavior. I did not count an assertion of a production contract as a second authority. Repeated test fixture setup was only isolated counterevidence and did not drive a numeric score.

14. **Decision rule 6 lacks a lens and defines neither “current referent” nor “line selector.”** Its opening phrase points toward `domain-model`, while duplicated CI selectors could point toward `duplication-knowledge`; its mandatory score 2 also bypasses centrality analysis. It had no referent in this component, so I did not apply it. If applicable, I would classify a single invalid identifier under domain model and synchronized copies under duplication.

15. **The “primary lens only” rule does not explain multi-causal facts.** Raw strings can simultaneously expose invalid states, repeat vocabulary, and force lifecycle handoffs. I assigned each negative fact once by its primary question: lifecycle handoff to ownership, unused dependencies to simplicity, raw path legality to domain, and repeated lookup/ref/trailer behavior to duplication.

16. **Confidence High cannot be finalized by one reviewer.** “Final High also requires independent readings to converge” is not decidable during an independent review. I reported evidence-quality confidence now and left final convergence to adjudication.

All other score-1 versus score-2 distinctions were discriminating here: the component consistently has identifiable owners, paths, concepts, and intended policies, so none of the “no stable ... can be identified” anchors fit.
