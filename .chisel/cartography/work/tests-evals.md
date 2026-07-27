# Cartography scout report: tests and evaluations

Assessed revision: `2bcf94fed8a9b429f18d9196fa824711d6f4cb0a` (`2bcf94fed`)

Scope: all 180 tracked files under `test/**` and `evals/**`. Cargo workspace
manifests, test consumers, repository documentation sources, and implementation
entry points were consulted only as boundary evidence and are not included in
this scope's counts.

Applicable instructions read: `AGENTS.md`, `CONTRIBUTING.md`, and
`docs/internal/testing-strategy.md` (`CLAUDE.md` is a symlink to `AGENTS.md`).

## Boundary decisions

- `twin-openai` and `twin-github` are separate components. Each is a distinct
  Cargo workspace member with its own protocol surface, router, state model,
  lifecycle, fixtures, and consumers. Their common use as local fake services
  is not enough to combine OpenAI scenario/stream behavior with GitHub
  repository/authentication behavior.
- The checked-in workflow fixtures outside `test/docs/**` are proposed as a
  shared `workflow-test-corpus` component. They are all user-facing workflow,
  configuration, prompt, and template inputs, and they are intentionally
  consumed across CLI, workflow, graph-language, rendering, and validation
  tests. Keeping them together avoids assigning shared compatibility data to
  one arbitrary production consumer.
- `test/docs/**` is proposed as a separate
  `documentation-workflow-tests` component. It has its own extraction and
  multi-phase runner entry points and owns a documentation-derived but curated
  executable corpus. The tracked fixtures are test source: the checklist
  records extracted, assembled, and adapted cases, and `run_tests.sh` executes
  them directly. They are therefore assigned rather than excluded as generated
  output.
- The SWE-bench tooling is a distinct evaluation component. It owns a
  generation, grading, monitoring, environment-generation, and result-recording
  workflow that is independent of the normal Cargo test lifecycle.
- `evals/swe-bench/scoreboard/**` is not executable evaluation source. The
  evaluation README calls it a Git-tracked permanent record, and
  `record_results.py` writes every tracked file shape beneath it. Those 16
  recorded outputs are proposed as a global exclusion.
- The two distribution shell tests and the benchmark-analysis SQL do not form a
  coherent component together. They are recommended additions to existing
  components, described after the component proposals.

## Proposed components

### `twin-openai` — OpenAI protocol twin

- **File count:** 35 (28 Rust, 5 Markdown, 1 Cargo manifest, 1 `.gitignore`)
- **Purpose:** Provides a deterministic OpenAI-compatible HTTP service for
  black-box and protocol-contract tests, including scripted successes,
  failures, streaming, request inspection, and live shape comparison.
- **Globs:** `test/twin/openai/**`
- **Exclude globs:** `[]`
- **Entry points:** `test/twin/openai/src/main.rs:main`,
  `test/twin/openai/src/lib.rs:build_app`,
  `test/twin/openai/src/lib.rs:build_app_with_config`,
  `test/twin/openai/src/app.rs:router`
- **Owns:** server bind/configuration lifecycle; `/v1/responses` and
  `/v1/chat/completions` request/response contracts; bearer-token namespaces;
  FIFO scenario queues; deterministic response IDs; normalized request logs;
  SSE construction and transport-failure behavior; admin reset/scenario APIs;
  debug UI and snapshots; local and opt-in live contract suites.
- **Depends on candidates:** `fabro-http`, `fabro-static`
- **Evidence:**
  - `Cargo.toml` — lists `test/twin/openai` as a workspace member and exposes
    `twin-openai` as a workspace dependency.
  - `test/twin/openai/Cargo.toml` — declares a non-published library/binary
    package described as a fake OpenAI-compatible server.
  - `test/twin/openai/src/app.rs:router` and
    `test/twin/openai/src/openai/mod.rs:router` — compose the health, OpenAI,
    admin, and debug HTTP surfaces.
  - `test/twin/openai/src/state.rs:AppState` — owns namespaced response
    counters, scenario queues, and request logs.
  - `test/twin/openai/src/engine/scenario.rs:ScenarioScript` — defines scripted
    success, application-error, delay, partial/malformed stream, and hang
    behavior.
  - `test/twin/openai/tests/common/mod.rs:spawn_server` and the eight sibling
    contract suites — exercise the service as a protocol boundary; the ignored
    `live_openai_contract.rs` compares supported protocol shapes with the live
    API.
  - `lib/foundation/fabro-test/Cargo.toml` and
    `lib/foundation/fabro-test/src/lib.rs:twin_openai` — show the shared
    integration-test harness consuming this package as an in-process service.

### `twin-github` — GitHub protocol twin

- **File count:** 20 (17 Rust, 2 PEM fixtures, 1 Cargo manifest)
- **Purpose:** Provides an in-process fake GitHub service with seeded mutable
  state and temporary Git repositories for black-box GitHub App, OAuth, API,
  GraphQL, and smart-HTTP tests.
- **Globs:** `test/twin/github/**`
- **Exclude globs:** `[]`
- **Entry points:** `test/twin/github/src/server.rs:TestServer::start`,
  `test/twin/github/src/server.rs:build_router`,
  `test/twin/github/src/state.rs:AppState`,
  `test/twin/github/src/fixtures.rs:FixtureState::into_app_state`
- **Owns:** ephemeral listener and shutdown lifecycle; temporary bare Git
  repositories; fake apps, installations, repositories, branches, pull
  requests, releases, projects, comments, webhook configuration, manifest
  conversions, access tokens, OAuth codes/tokens/users; GitHub authentication
  checks; bundled test RSA key pair.
- **Depends on candidates:** `fabro-http`
- **Evidence:**
  - `Cargo.toml` — lists `test/twin/github` independently as a workspace member
    and workspace dependency.
  - `test/twin/github/Cargo.toml` — declares a non-published library package
    described as a fake GitHub API server.
  - `test/twin/github/src/handlers/mod.rs:build_router` — registers the GitHub
    App, installation, branch, pull-request, manifest, OAuth, user, release,
    GraphQL, and Git smart-HTTP routes.
  - `test/twin/github/src/state.rs:AppState` — owns the central seeded and
    mutable GitHub-domain state.
  - `test/twin/github/src/server.rs:TestServer::start` — initializes temporary
    Git repositories, binds an ephemeral listener, and controls graceful
    shutdown.
  - `test/twin/github/src/fixtures.rs:FixtureState` and
    `test/twin/github/src/testdata/*.pem` — define reusable seeded service data
    and the owned authentication fixtures.
  - `lib/foundation/fabro-test/src/lib.rs:TwinGitHub` and
    `lib/apps/fabro-cli/tests/it/support/auth_harness.rs` — show this twin
    serving the CLI/server authentication integration boundary.

### `workflow-test-corpus` — Shared workflow compatibility fixtures

- **File count:** 42
  - 8 root `test/*.fabro` workflows
  - 14 `test/attractor/*.dot` compatibility graphs
  - 3 `test/dot-compatibility/*.fabro` graphs
  - 17 templating/configuration files under the four templated fixture trees
- **Purpose:** Supplies reusable user-facing workflow, compatibility,
  configuration, prompt, partial, and template inputs to cross-crate parser,
  validator, renderer, workflow, and CLI tests.
- **Globs:** `test/*.fabro`, `test/attractor/**`,
  `test/dot-compatibility/**`, `test/templated_inputs/**`,
  `test/templated_unbound_imported/**`,
  `test/templated_unbound_partial/**`, `test/templates/**`
- **Exclude globs:** `[]`
- **Entry points:** `test/simple.fabro`,
  `test/attractor/simple_example.dot`,
  `test/dot-compatibility/acp-agent-chain.fabro`,
  `test/templates/static_dependencies/workflow.fabro`,
  `test/templates/sibling_partial/workflow.fabro`
- **Owns:** representative valid and invalid workflow shapes; branching,
  conditions, parallelism, styles, and legacy syntax cases; Attractor DOT
  compatibility graphs; shared DOT parse/render/validation cases; template
  input, import, include, sibling-partial, static-dependency, and
  missing-dependency fixture trees.
- **Depends on candidates:** `fabro-cli`, `fabro-graphviz`, `fabro-template`,
  `fabro-test`, `fabro-validate`, `fabro-workflow`
- **Evidence:**
  - `docs/internal/testing-strategy.md` — explicitly recognizes checked-in
    user-facing workflows, configs, prompts, and repository contents as shared
    fixtures.
  - `lib/foundation/fabro-test/src/lib.rs:TestContext::install_fixture` —
    resolves named inputs from the repository `test/` directory for isolated
    CLI tests.
  - `lib/apps/fabro-cli/tests/it/cmd/validate.rs` and
    `lib/apps/fabro-cli/tests/it/workflow/dry_run_examples.rs` — consume the
    root workflows and all templating fixture trees as black-box CLI inputs.
  - `lib/components/fabro-workflow/tests/it/attractor_compat.rs` — enumerates
    and parses every graph in `test/attractor/**`.
  - `lib/components/fabro-graphviz/src/render.rs:dot_compatibility_fixtures`
    and
    `lib/components/fabro-validate/src/lib.rs:dot_compatibility_fixtures` —
    independently enumerate the same three `test/dot-compatibility/**` inputs,
    establishing that corpus as shared rather than crate-local.

### `documentation-workflow-tests` — Documentation workflow conformance

- **File count:** 55 (40 Fabro workflows, 7 shell files, 5 Markdown files, 2
  run TOML files, 1 Python extractor)
- **Purpose:** Extracts, curates, validates, preflights, and executes workflow
  examples and companion files derived from Fabro documentation.
- **Globs:** `test/docs/**`
- **Exclude globs:** `[]`
- **Entry points:** `test/docs/run_tests.sh`,
  `test/docs/extract_dots.py:main`, `test/docs/CHECKLIST.md`
- **Owns:** documentation-example corpus layout; prompt and script stubs;
  variable-bearing run configurations; extraction naming and stub generation;
  validate/preflight/dry-run/live phase selection; parallel execution and
  temporary result/run directories; the documented corpus checklist.
- **Depends on candidates:** `fabro-cli`, `fabro-workflow`, the final
  documentation-site component
- **Evidence:**
  - `test/docs/run_tests.sh:run_one` — discovers all 40 tracked `*.fabro`
    examples and invokes the built `fabro` binary in validate, preflight,
    dry-run, model-specific, or full execution modes.
  - `test/docs/extract_dots.py:main` — reads documentation Markdown, extracts
    complete DOT graphs, and creates companion prompt stubs and run
    configurations under `test/docs`.
  - `test/docs/CHECKLIST.md` — documents the 40-example corpus, distinguishes
    extracted and assembled cases, records companion-file needs, and provides
    the runner commands.
  - `.claude/skills/docs/SKILL.md` — instructs documentation changes containing
    full DOT graphs to run `./test/docs/run_tests.sh validate`, tying this
    harness to the documentation change lifecycle.

### `swe-bench-evaluation` — SWE-bench evaluation workflow

- **File count:** 9 (6 Python scripts, 1 Fabro workflow, 1 requirements file, 1
  README)
- **Purpose:** Generates Fabro patches for SWE-bench Lite instances, grades
  them through Daytona or the official harness, monitors runs, and records
  normalized result summaries.
- **Globs:** `evals/swe-bench/*.py`, `evals/swe-bench/*.fabro`,
  `evals/swe-bench/*.txt`, `evals/swe-bench/README.md`
- **Exclude globs:** `[]` (the sibling scoreboard is a global exclusion)
- **Entry points:** `evals/swe-bench/run_eval.py:main`,
  `evals/swe-bench/evaluate_daytona.py:main`,
  `evals/swe-bench/evaluate.py:main`,
  `evals/swe-bench/record_results.py:main`,
  `evals/swe-bench/status.py:main`,
  `evals/swe-bench/gen_dockerfile.py:main`
- **Owns:** SWE-bench Lite dataset selection; per-instance goal/workflow/TOML
  generation; Daytona snapshot and sandbox specifications; Fabro subprocess
  orchestration and timeout cleanup; patch extraction; official and
  Daytona-based grading; progress summaries; scoreboard record schema and
  leaderboard regeneration.
- **Depends on candidates:** `fabro-cli`, `fabro-sandbox`,
  `fabro-workflow`
- **External dependencies:** Hugging Face `datasets`, the `swebench` harness,
  Daytona, and optionally Docker through the official harness.
- **Evidence:**
  - `evals/swe-bench/README.md` — defines the three-stage generate, evaluate,
    and record lifecycle, the two grading backends, and raw-versus-recorded
    result locations.
  - `evals/swe-bench/run_eval.py:run_instance` — creates per-instance Fabro
    workflows/configs, invokes `fabro run`, and extracts produced patches.
  - `evals/swe-bench/evaluate_daytona.py` — creates grading workflows and
    executes held-out tests in Daytona snapshots.
  - `evals/swe-bench/evaluate.py:main` — exposes the alternative official
    Docker-backed `swebench.harness.run_evaluation` path.
  - `evals/swe-bench/gen_dockerfile.py:generate_dockerfile` — translates
    SWE-bench repository/version specs into reusable sandbox images.
  - `evals/swe-bench/record_results.py:main` and
    `regenerate_leaderboard` — define and write the tracked scoreboard record
    formats.

## Recommended additions to existing components

These files are assigned in the coverage accounting but do not justify new
components:

| File | Recommended component | Reason |
| --- | --- | --- |
| `test/bin/install_test.sh` | documentation/web scout's marketing-site component | It is a black-box shell contract test whose sole product target is `apps/marketing/public/install.sh`; it owns a fake `gh` executable and temporary install home only for that test. |
| `test/bin/release_test.sh` | `fabro-build-tooling` | It is an executable release-mode shell contract and changes with the repository release-automation lifecycle. |
| `test/analysis/bench-tests-diff.sql` | `fabro-build-tooling` | Its documented inputs are the two CSVs produced by `cargo dev bench-tests`, whose implementation is `lib/foundation/fabro-dev/src/commands/bench_tests.rs`. |

## Global exclusion

### Recorded SWE-bench scoreboards

- **Globs:** `evals/swe-bench/scoreboard/**`
- **Tracked files:** 16 (1 leaderboard JSON plus 5 run directories containing
  one `README.md`, one `meta.json`, and one `instances.jsonl` each)
- **Reason:** committed evaluation records generated by
  `evals/swe-bench/record_results.py`, not executable evaluation source.
- **Evidence:** `evals/swe-bench/README.md` calls the directory a Git-tracked
  permanent record; `record_results.py` writes `instances.jsonl`, `meta.json`,
  each run `README.md`, and regenerates `leaderboard.json`.

Raw `evals/swe-bench/results/**` data is also described as generated output,
but it is not tracked at the assessed revision and therefore is not part of
the 180-file inventory.

No `test/docs/**` files are excluded. Although the extractor derives some
files from documentation, the tracked corpus includes assembled/adapted
executable fixtures and companion stubs/configuration, and the runner consumes
those files as test inputs.

## Coverage

| Assignment | Tracked files |
| --- | ---: |
| `twin-openai` | 35 |
| `twin-github` | 20 |
| `workflow-test-corpus` | 42 |
| `documentation-workflow-tests` | 55 |
| `swe-bench-evaluation` | 9 |
| Recommended addition to marketing-site component | 1 |
| Recommended additions to `fabro-build-tooling` | 2 |
| Global exclusion: SWE-bench scoreboards | 16 |
| **Scoped inventory** | **180** |

- **Assigned:** 164 (161 in proposed test/evaluation components and 3 additions
  to existing components)
- **Excluded:** 16
- **Unmapped:** 0
- **Overlap:** 0
- **Accounting check:** `164 + 16 + 0 = 180`
- **Unmapped files:** `[]`

## External boundary evidence consulted

These files are outside the scoped inventory and are neither assigned nor
counted as unmapped:

- `Cargo.toml` — workspace membership and workspace dependency declarations for
  both twin services.
- `lib/foundation/fabro-test/Cargo.toml` and
  `lib/foundation/fabro-test/src/lib.rs` — shared fixture installation and twin
  service consumption.
- `lib/apps/fabro-cli/tests/it/**` — black-box workflow fixture and twin-GitHub
  consumers.
- `lib/components/fabro-workflow/tests/it/attractor_compat.rs` — Attractor
  corpus consumer.
- `lib/components/fabro-graphviz/src/render.rs` and
  `lib/components/fabro-validate/src/lib.rs` — shared DOT compatibility corpus
  consumers.
- `docs/internal/testing-strategy.md` — repository test-layer and fixture
  ownership policy.
- `.claude/skills/docs/SKILL.md` — documentation test-runner invocation policy.
- `apps/marketing/public/install.sh` — install shell-test target.
- `lib/foundation/fabro-dev/src/commands/bench_tests.rs` — benchmark CSV
  producer consumed by the analysis SQL.

## Genuine boundary questions

1. Should `workflow-test-corpus` remain a distinct 42-file shared data
   component, as proposed, or should reconciliation distribute its three
   subcorpora to `fabro-cli` (25 general/template fixtures),
   `fabro-workflow` (14 Attractor fixtures), and `fabro-graphviz` (3 shared DOT
   compatibility fixtures)? The cross-crate consumers support a shared
   boundary, while the production behaviors they exercise support attachment.
2. Should `documentation-workflow-tests` remain a separate executable harness,
   or should its 55 files be included in the documentation-site component?
   Its runner and phase lifecycle support separation; its source derivation and
   documentation-change trigger support inclusion with documentation.
3. Should `test/bin/release_test.sh` be assigned to `fabro-build-tooling` as a
   release-lifecycle contract, or remain separately unmapped until the final
   map determines which current release entry point owns that shell contract?
