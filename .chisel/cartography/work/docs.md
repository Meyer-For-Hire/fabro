# Documentation cartography scout

Assessed revision: `2bcf94fed8a9b429f18d9196fa824711d6f4cb0a`

Instructions read: `AGENTS.md`, `CONTRIBUTING.md`, and the Chisel cartography prompt. Scope is every tracked file under `docs/**`, plus `README.md` and `install.md`.

## Inventory

There are **488** scoped tracked files:

| Area | Files |
| --- | ---: |
| `docs/public/**` | 253 |
| `docs/internal/**` | 82 |
| `docs/plans/**` | 88 |
| `docs/brainstorms/**` | 11 |
| `docs/ideation/**` | 3 |
| `docs/superpowers/**` | 49 |
| `README.md`, `install.md` | 2 |

## Proposed components

### `public-documentation` — Public documentation

- **Purpose:** Own the authored Fabro user documentation, Mintlify presentation/configuration, repository landing page, and the maintenance procedure for published web screenshots.
- **Globs:**
  - `README.md`
  - `docs/public/**`
  - `docs/internal/updating-web-screenshots.md`
- **Exclude globs:**
  - `docs/public/api-reference/fabro-api.yaml` — separate source contract
  - `docs/public/changelog/**` — separate published release-history component
  - all 22 generated public Graphviz SVG globs listed under exclusions below
- **Entry points:**
  - `README.md`
  - `docs/public/docs.json`
  - `docs/public/getting-started/introduction.mdx`
  - `docs/public/getting-started/quick-start.mdx`
  - `docs/internal/updating-web-screenshots.md`
- **Owns:**
  - Mintlify theme, navigation, tabs, and page ordering
  - public concepts, guides, tutorials, administration material, and reference prose
  - public documentation images, manually maintained SVG illustrations, logos, syntax definitions, and curated web screenshots
  - repository-facing overview and documentation links
  - web-screenshot capture and verification workflow
- **Depends on candidates:** `fabro-http-api-contract`, `documentation-demo-workflows`, the CLI/config components that refresh fenced reference regions.
- **Evidence:**
  - `AGENTS.md:46-51` mounts `docs/public` as the Mintlify document root.
  - `docs/public/docs.json` declares the Mintlify schema, theme, navigation, OpenAPI tab, and changelog tab.
  - `README.md` links to `docs.fabro.sh` and embeds assets from `docs/public/images` and `docs/public/logo`.
  - `docs/internal/updating-web-screenshots.md` names `docs/public/images/web/` as the screenshot destination, maps files to UI routes and doc consumers, and defines the refresh/verification workflow.
  - `lib/foundation/fabro-dev/src/commands/docs.rs` exposes `cargo dev docs refresh/check`; `docs_cli_reference.rs` and `docs_options_reference.rs` update only fenced regions of `docs/public/reference/cli.mdx` and `docs/public/reference/user-configuration.mdx`. The two whole files remain assigned here because substantial prose outside those fences is authored.
  - `test/docs/extract_dots.py` extracts workflow examples from the public docs for validation.
- **Assigned count:** **112**: 110 public-site files after the API contract, changelog, and 22 generated SVGs are removed, plus `README.md` and the screenshot-maintenance guide.

### `public-release-history` — Published changelog

- **Purpose:** Preserve and publish dated user-facing release/change records independently of current reference documentation.
- **Globs:** `docs/public/changelog/**`
- **Entry points:** `docs/public/docs.json` changelog navigation; newest page at the assessed revision is `docs/public/changelog/2026-07-25.mdx`.
- **Owns:** dated titles, migration warnings, feature summaries, and historical behavior notes.
- **Depends on candidates:** `public-documentation` for Mintlify navigation/presentation.
- **Evidence:**
  - `docs/public/docs.json` gives changelog its own top-level tab and lists every dated page.
  - The 120 `docs.json` changelog page entries exactly match the 120 tracked MDX files.
  - Each page has date/title frontmatter and describes changes for that date.
  - `lib/apps/fabro-server/tests/it/api/docs.rs:45-50` deliberately reads a changelog page as historical documentation.
- **Assigned count:** **120**.

### `fabro-http-api-contract` — Fabro HTTP API contract

- **Purpose:** Define the OpenAPI-first wire contract used by the server, generated clients/types, conformance tests, and published API reference.
- **Globs:** `docs/public/api-reference/fabro-api.yaml`
- **Entry points:** `docs/public/api-reference/fabro-api.yaml`
- **Owns:** HTTP routes, request/response schemas, authentication declarations, and API-facing wire documentation.
- **Depends on candidates:** none at the documentation layer; parent reconciliation should make its consumers depend on this component.
- **Consumers / evidence:**
  - `AGENTS.md:55-61` explicitly calls this file the source of truth and documents the Rust and TypeScript regeneration workflow.
  - `lib/foundation/fabro-api/build.rs:159` consumes it for Rust generation.
  - `lib/packages/fabro-api-client/package.json:7` consumes it for TypeScript Axios generation.
  - `lib/apps/fabro-server/src/server/handler/system.rs:694` embeds it in the server.
  - `lib/apps/fabro-server/tests/it/openapi_conformance.rs:21` reads it for route/spec conformance.
  - `docs/public/docs.json` points Mintlify's API tab at it.
- **Assigned count:** **1**.

### `documentation-demo-workflows` — Executable documentation demos

- **Purpose:** Provide runnable workflow definitions and supporting configuration/prompts used by public tutorials and demonstrations.
- **Globs:**
  - `docs/internal/demo/*.fabro`
  - `docs/internal/demo/*.toml`
  - `docs/internal/demo/prompts/**`
- **Exclude globs:**
  - `docs/internal/demo/*.svg`
  - `docs/internal/demo/*.png`
- **Entry points:**
  - `docs/internal/demo/01-hello.fabro`
  - `docs/internal/demo/14-search-imagegen.toml`
  - tutorial commands of the form `fabro run docs/internal/demo/<name>.fabro`
- **Owns:** small executable example graphs, the image-generation demo run config, and shared demo prompt text.
- **Depends on candidates:** CLI runner, workflow engine/validator, agent tools, and configured sandbox/model providers.
- **Evidence:**
  - Public tutorials such as `docs/public/tutorials/hello-world.mdx`, `parallel-review.mdx`, `multi-model.mdx`, `plan-implement.mdx`, and `ensemble.mdx` invoke these paths directly.
  - `docs/public/core-concepts/models.mdx:250-251` also uses these graphs as runnable model examples.
  - `docs/internal/demo/14-search-imagegen.toml` selects its graph, Daytona environment, snapshot, and output assets.
  - `.fabro` files are complete Graphviz workflow entry documents with `goal`, start, and exit nodes.
- **Assigned count:** **16** (14 `.fabro`, one `.toml`, one prompt).

### `internal-engineering-guidance` — Active engineering policies and architecture references

- **Purpose:** Record active repository-wide engineering policies and maintained architectural/runtime contracts that guide implementation changes.
- **Globs:**
  - `docs/internal/*-strategy.md`
  - `docs/internal/*-policy.md`
  - `docs/internal/events.md`
  - `docs/internal/fabro-event-schema-v2-concrete-shape.md`
  - `docs/internal/llm-client-resolution.md`
  - `docs/internal/run-directory-keys.md`
- **Entry points:**
  - `AGENTS.md:136-146`
  - `docs/internal/events-strategy.md`
  - `docs/internal/testing-strategy.md`
  - `docs/internal/error-handling-strategy.md`
- **Owns:**
  - logging, events, testing, migrations, secret handling, error handling, React-effect, and panic policies
  - the maintained event catalog and implemented V2 event design explanation
  - LLM client-resolution rules, parallel-execution semantics, and run scratch-file reference
- **Depends on candidates:** the runtime, server, CLI, web, configuration/auth, and workflow components whose contracts it describes. These are documentation dependencies rather than build edges.
- **Evidence:**
  - `AGENTS.md:136-146` makes seven strategy/policy documents mandatory reading before related changes.
  - `docs/internal/events-strategy.md` distinguishes durable product events from tracing and identifies their consumers.
  - `docs/internal/events.md` is the maintained serialized event catalog and was updated near the assessed revision.
  - `docs/internal/fabro-event-schema-v2-concrete-shape.md:5` says `Status: implemented`; it also says the hand-written Rust types, not this document, are the actual contract source of truth.
  - `docs/internal/parallel-strategy.md:3` says `Status: implemented` and was updated with the shared-checkout behavior at the assessed revision.
  - `lib/foundation/fabro-vault/src/store.rs:359` links implementation documentation back to `docs/internal/migrations-strategy.md`.
- **Assigned count:** **13**.

### `product-context` — Internal product framing

- **Purpose:** Maintain concise product intent, audience, current shape, success signals, and stable technical/product constraints.
- **Globs:** `docs/internal/product/**`
- **Entry points:**
  - `docs/internal/product/product-description.md`
  - `docs/internal/product/current-state.md`
- **Owns:** business problem, personas, product description, current-state snapshot, success metrics, and product-level technical requirements.
- **Depends on candidates:** none as a build edge; it informs product and documentation work across the repository.
- **Evidence:**
  - The six documents have complementary named roles rather than dated implementation tasks.
  - `docs/internal/product/current-state.md` explicitly describes a deliberately brief current product snapshot.
  - `docs/internal/product/technical-requirements.md` explicitly calls its contents stable constraints product changes should respect.
- **Assigned count:** **6**.

## Cross-scope assignment

### `install.md` -> marketing-site component

- **Count:** **1**.
- `install.md` is a tracked mode-`120000` symlink to `apps/marketing/public/install.md`.
- Commit `0cc02c294dac23e3ace7646528431e758e37eea1` states that Vercel deploys the marketing subtree, so the real file lives there and the repository-root path is a symlink.
- `apps/marketing/src/pages/index.astro` advertises `https://fabro.sh/install.md`.
- The root alias should therefore be claimed by the component that owns `apps/marketing/public/install.md`, rather than by `public-documentation`.

## Evidence-backed exclusions

### Historical brainstorm, plan, audit, and design records — 159 files

These are point-in-time requirements, ideation, implementation plans, handoffs, one-time QA instructions, measurements, audits, or superseded proposals. They remain useful history but are not active source contracts or maintained policy components.

| Glob/path | Count | Evidence |
| --- | ---: | --- |
| `docs/brainstorms/**` | 11 | Dated `*-requirements.md` brainstorm artifacts. |
| `docs/ideation/**` | 3 | Dated ideation records. |
| `docs/plans/**` | 88 | Dated implementation plans and handoffs. |
| `docs/superpowers/plans/**` | 45 | Dated execution plans. |
| `docs/superpowers/specs/**` | 4 | Dated feature/design specs. |
| `docs/internal/cargo-target-apfs-churn-plan.md` | 1 | Checkbox execution plan with an unfilled results section. |
| `docs/internal/cli-workflow-coupling-audit.md` | 1 | Snapshot audit organized around completed and remaining couplings. |
| `docs/internal/event-schema-competitive-analysis.md` | 1 | Dated comparative research report. |
| `docs/internal/fabro-event-schema-v2-proposal.md` | 1 | Explicit `Status: proposal`; the implemented concrete-shape document supersedes its framing. |
| `docs/internal/mcp-server-qa-test-plan.md` | 1 | Explicitly says it is a one-time manual QA pass, not a reusable testing template. |
| `docs/internal/plan-events-as-source-of-truth-follow-ups.md` | 1 | Prerequisite implementation plan. |
| `docs/internal/plan-events-as-source-of-truth.md` | 1 | Implementation plan/summary rather than current contract reference. |
| `docs/internal/slow-test-opportunities-2026-04-07.md` | 1 | Dated measurement dataset and implementation-status record. |

This exclusion does **not** include `docs/public/changelog/**`: the changelog is a live, complete Mintlify publication surface and is mapped as its own component.

### Generated Graphviz renderings — 44 files

| Glob/path | Unique count | Evidence |
| --- | ---: | --- |
| `docs/internal/demo/*.svg` | 11 | Every file contains `Generated by graphviz`; each has a same-stem `.fabro` source. |
| `docs/internal/demo/*.png` | 11 | Same-stem raster renderings were introduced alongside the `.fabro` and generated SVG files; their pixel dimensions match the SVG point dimensions at Graphviz's 96-DPI raster scale. |
| `docs/public/images/*-workflow.svg` | 9 | Every matching tracked file contains `Generated by graphviz`. |
| `docs/public/images/tutorial-*.svg` | 10 | Every matching tracked file contains `Generated by graphviz`; one file overlaps the previous glob. |
| `docs/public/images/brave-search-research.svg` | 1 | Contains `Generated by graphviz`. |
| `docs/public/images/how-fabro-works.svg` | 1 | Contains `Generated by graphviz`. |
| `docs/public/images/nlspec-conformance.svg` | 1 | Contains `Generated by graphviz`. |
| `docs/public/images/plan-implement-readme.svg` | 1 | Contains `Generated by graphviz`. |

The public SVG rows resolve to **22 unique files** because `tutorial-sub-workflow.svg` matches both broad globs. Curated UI screenshots and hand-authored SVG illustrations remain assigned to `public-documentation`; `docs/internal/updating-web-screenshots.md` establishes their manual capture and verification workflow.

The fenced regions in `docs/public/reference/cli.mdx` and `docs/public/reference/user-configuration.mdx` are generated, but the files are mixed authored/generated documents. Cartography operates at file granularity, so both whole files stay assigned to `public-documentation`.

### Vendored third-party legal text — 1 file

- **Glob:** `docs/internal/licenses/graphviz-14.1.5-LICENSE`
- **Count:** **1**.
- **Evidence:** the filename pins Graphviz 14.1.5, the contents are the verbatim Eclipse Public License 2.0 plus secondary-license text, and the introducing commit is `chore: add vendored Graphviz license to docs-internal/licenses`.

## Unmapped files

- **Glob:** `docs/internal/assets/**`
- **Count:** **15**.
- These form a coherent collection of logos, palette mockups, headers, and HTML/PNG social-card pairs, but no tracked file consumes these exact paths at the assessed revision.
- `docs/internal/updating-web-screenshots.md` identifies `docs/public/logo/dark.svg` and `docs/public/logo/light.svg`, not the internal assets, as the source-of-truth logos.
- The collection has no manifest, status marker, or documented update workflow establishing whether it is maintained brand source, derived output, or historical design collateral. It should remain unmapped until that ownership is confirmed.

## Coverage

| Disposition | Count |
| --- | ---: |
| Assigned to proposed documentation components | 268 |
| Cross-scope assignment (`install.md` to marketing site) | 1 |
| **Assigned total** | **269** |
| Excluded historical records | 159 |
| Excluded generated renderings | 44 |
| Excluded vendored license | 1 |
| **Excluded total** | **204** |
| Unmapped internal brand collateral | 15 |
| **Scoped relevant total** | **488** |

`269 + 204 + 15 = 488`; every scoped tracked file is assigned, excluded, or explicitly unmapped.

## Open questions

1. Are the 15 files under `docs/internal/assets/**` maintained brand sources, or intentionally retained historical collateral? A component should be added only if an owner/update workflow confirms the former.
2. Should the parent map keep `docs/internal/fabro-event-schema-v2-concrete-shape.md` in active engineering guidance, as proposed here based on `Status: implemented` and recent updates, or treat it as an implemented design record now that Rust event types and `events.md` carry the live contract?
3. Confirm the final marketing component ID that will claim the `install.md` symlink together with `apps/marketing/public/install.md`.
