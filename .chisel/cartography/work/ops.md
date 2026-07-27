# Repository operations cartography scout

Assessed revision:
`2bcf94fed8a9b429f18d9196fa824711d6f4cb0a` (`2bcf94fed`).

Owned scope: root-level tracked files plus tracked files under `.ai/**`,
`.cargo/**`, `.claude/**`, `.config/**`, `.fabro/**`, `.github/**`,
`bin/**`, `docker/**`, and `installer/**`. Files under `lib/**`, `apps/**`,
`docs/**`, `test/**`, and `evals/**` were not counted. The
`lib/foundation/fabro-dev/**` and `lib/foundation/build-support/**` trees were
consulted only as boundary and dependency evidence because the foundation
scout owns them.

Applicable instructions read: `AGENTS.md`, its `CLAUDE.md` symlink, and
`CONTRIBUTING.md`.

## Inventory and boundary approach

- `git ls-tree -r --name-only` at the assessed revision yields exactly 143
  tracked files in this scope: 24 root files, four under `.ai/`, one under
  `.cargo/`, eight under `.claude/`, one under `.config/`, 87 under `.fabro/`,
  seven under `.github/`, one under `bin/`, eight under `docker/`, and two
  under `installer/`.
- Repository-wide manifests, tool configuration, and contributor rules are
  grouped as one development-policy component. They form the shared contract
  used by Cargo, Bun, nextest, rustfmt, Clippy, contributors, coding agents,
  and CI; splitting every configuration file would create small boundaries
  without independent entry points.
- Pull-request CI and release automation are separate. The former validates
  changes on branch events, while the latter owns version tags and publication
  of binary, container, GitHub Release, and Homebrew artifacts.
- Product container packaging and operator Compose deployment are grouped
  because the image layout, entrypoint, runtime environment, proxy files, and
  Compose stacks share one deployable artifact contract. The explicit
  split-web proof-of-concept is retained in this proposed component, with the
  question noted below.
- `.fabro/project.toml`, its development image, and the named workflow catalog
  are grouped as the repository's Fabro-native automation surface. They share
  the `fabro run <name>` consumer, project defaults, clone-based execution
  environment, and repository-maintenance lifecycle.
- The smaller `.ai`, `.claude`, and `bin/agent` families are grouped as coding
  agent automation. Their clients differ, but all supply repository-local
  prompts, skills, hooks, or helper commands to agents working on this
  repository.
- Machine-produced state, a backup, vendored policy text, non-runtime review
  assets, legal/overview metadata, and canonical-file symlink aliases are
  excluded with exact counts below.

## Proposed components

### `repository-development-policy` — Repository development policy

- **Assigned file count:** 11
- **Purpose:** Defines the repository-wide Rust and JavaScript workspace,
  dependency, formatting, lint, test, version-control, contributor, and coding
  agent development contract.
- **Globs:**
  - `.cargo/**`
  - `.config/**`
  - `.gitattributes`
  - `.gitignore`
  - `AGENTS.md`
  - `CONTRIBUTING.md`
  - `Cargo.toml`
  - `package.json`
  - `bunfig.toml`
  - `clippy.toml`
  - `rustfmt.toml`
- **Exclude globs:** none
- **Entry points:**
  - `Cargo.toml:[workspace]`
  - `Cargo.toml:[workspace.dependencies]`
  - `Cargo.toml:[workspace.lints]`
  - `package.json:workspaces`
  - `.cargo/config.toml:[alias]`
  - `.config/nextest.toml`
  - `AGENTS.md`
  - `CONTRIBUTING.md`
- **Owns:** Rust and Bun workspace membership; shared Rust dependency and
  version policy; workspace lint and compilation profiles; Bun linker
  selection; Cargo developer aliases and test proxy policy; nextest timeout
  profiles; rustfmt and Clippy policy; tracked/generated path treatment; and
  repository-wide contributor and agent instructions.
- **Depends-on candidates:** `fabro-build-tooling` (the `cargo dev` alias
  dispatches to its feature-gated binary).
- **Evidence:**
  - `Cargo.toml` — declares all Rust workspace members, default members,
    workspace package metadata, shared dependencies, lint policy, and build
    profiles.
  - `package.json` and `bunfig.toml` — declare the JavaScript workspace and
    deterministic Bun workspace linker contract.
  - `.cargo/config.toml` — exposes `cargo dev` as the CLI entry to
    `fabro-dev`, defines the test alias, and supplies the repository test
    proxy-policy environment.
  - `.config/nextest.toml`, `clippy.toml`, and `rustfmt.toml` — are direct
    configuration inputs to the repository's test, lint, and formatting
    commands.
  - `AGENTS.md` and `CONTRIBUTING.md` — define the repository-wide build/test
    commands, architectural policies, and contribution workflow.
  - `.gitattributes` and `.gitignore` — actively define generated-file
    classification and the source/output boundary used by developers and CI.
  - `lib/foundation/fabro-dev/Cargo.toml` and
    `lib/foundation/fabro-dev/src/lib.rs:Command` — out-of-scope evidence that
    the Cargo alias targets a distinct internal development CLI.

### `repository-ci` — Pull-request and branch continuous integration

- **Assigned file count:** 3
- **Purpose:** Runs branch and pull-request validation for the Rust and
  TypeScript workspaces and configures static validation of GitHub Actions
  workflows.
- **Globs:**
  - `.github/workflows/rust.yml`
  - `.github/workflows/typescript.yml`
  - `.github/zizmor.yml`
- **Exclude globs:** none
- **Entry points:**
  - `.github/workflows/rust.yml`
  - `.github/workflows/typescript.yml`
  - `.github/zizmor.yml`
- **Owns:** branch/path trigger policy; Rust format, lint, generated-doc, test,
  and twin-E2E jobs; TypeScript typecheck, test, and production-build jobs;
  concurrency cancellation; CI test profile selection; and repository-local
  workflow-linter exceptions.
- **Depends-on candidates:** `repository-development-policy`,
  `fabro-build-tooling`, `fabro-web-app`,
  `fabro-api-client-generation`, and `twin-openai`. The workflows are also
  integration consumers of the full Rust workspace rather than a production
  runtime dependency of each Rust component.
- **Evidence:**
  - `.github/workflows/rust.yml` — path-gates Rust-relevant changes and runs
    the pinned formatter, Clippy, generated-document check, workspace nextest
    suite, and selected twin-mode E2E packages.
  - `.github/workflows/typescript.yml` — installs the frozen Bun workspace,
    typechecks the web app and generated-client package, runs web tests, and
    invokes `cargo dev build` for the release-style embedded-SPA build.
  - `.github/zizmor.yml` — is consumed alongside those workflows and names
    workflow-specific action-reference exceptions.
  - `lib/foundation/fabro-dev/src/commands/build.rs` — out-of-scope evidence
    that `cargo dev build` refreshes the SPA and then forwards to Cargo build.

### `release-distribution-automation` — Release and package publication

- **Assigned file count:** 4
- **Purpose:** Cuts nightly releases and publishes versioned CLI archives,
  GitHub Releases, multi-architecture container images, attestations, and
  stable/nightly Homebrew formulas.
- **Globs:**
  - `.github/workflows/nightly.yml`
  - `.github/workflows/release.yml`
  - `installer/**`
- **Exclude globs:** none
- **Entry points:**
  - `.github/workflows/nightly.yml`
  - `.github/workflows/release.yml`
  - `installer/fabro.rb.template`
  - `installer/fabro-nightly.rb.template`
- **Owns:** scheduled nightly tag creation; cross-platform release target
  matrix; CLI archive/checksum generation; provenance attestations; GitHub
  Release creation; release container publication; stable and nightly release
  channel selection; and Homebrew formula template substitution/publication.
- **Depends-on candidates:** `repository-development-policy`,
  `fabro-build-tooling`, `container-packaging-and-deployment`, `fabro-cli`,
  and `fabro-spa`.
- **Evidence:**
  - `.github/workflows/nightly.yml` — mints the release-app token and invokes
    `cargo --locked dev release --nightly` after ensuring the current commit
    does not already have a nightly tag.
  - `.github/workflows/release.yml` — is triggered by version tags, compiles
    and packages five targets, attests archives and container images, creates
    the GitHub Release, publishes the multi-architecture image, and updates
    stable or nightly Homebrew formulas.
  - `installer/fabro.rb.template` and
    `installer/fabro-nightly.rb.template` — define the platform archive URLs,
    checksum placeholders, installed binary, and Homebrew smoke test consumed
    by the release workflow.
  - `lib/foundation/fabro-dev/src/commands/release.rs` — out-of-scope evidence
    that the developer CLI owns release version computation, test smoke,
    `Cargo.toml`/`Cargo.lock` update, commit, tag, and push before the tag
    workflow publishes artifacts.
  - `lib/foundation/fabro-dev/src/commands/docker_build.rs` — out-of-scope
    evidence that local image construction intentionally shares the release
    pipeline's `tmp/docker-context/<arch>/fabro` layout.

### `container-packaging-and-deployment` — Container packaging and deployment

- **Assigned file count:** 16
- **Purpose:** Packages the Fabro CLI/server as a runtime container and
  defines supported local, production, Tailscale, and split-web Compose
  deployments around that image.
- **Globs:**
  - `.dockerignore`
  - `.env.example`
  - `Dockerfile`
  - `docker-compose*.yaml`
  - `docker/**`
- **Exclude globs:** none
- **Entry points:**
  - `Dockerfile`
  - `docker/entrypoint.sh`
  - `docker/preflight.sh`
  - `docker-compose.yaml`
  - `docker-compose.prod.yaml`
  - `docker-compose.tailscale.yaml`
  - `docker-compose.split-web.yaml`
- **Owns:** staged multi-architecture binary image layout; runtime package and
  unprivileged-user setup; storage-home and Docker-socket group handoff;
  deployment environment contract; preflight resource/daemon/network checks;
  Caddy proxy/TLS behavior; Compose services, volumes, ports, and health
  checks; and the split static-web/API deployment configuration.
- **Depends-on candidates:** `fabro-cli`, `fabro-server`, `fabro-web-app`, and
  `fabro-build-tooling`.
- **Evidence:**
  - `Dockerfile` — consumes the architecture-specific binary staged under
    `tmp/docker-context`, installs runtime dependencies, and installs the
    shared entrypoint.
  - `docker/entrypoint.sh` — owns storage permissions, Docker socket group
    mapping, and privilege drop before launching Fabro.
  - `docker/preflight.sh` — is a standalone deployment readiness entry point
    for Docker version/daemon, Compose, CPU, memory, disk, port, and registry
    reachability.
  - `docker-compose.yaml`, `docker-compose.local.yaml`,
    `docker-compose.prod.yaml`, and `docker-compose.tailscale.yaml` — define
    distinct operator compositions around the same Fabro image and runtime
    state.
  - `docker-compose.split-web.yaml` and `docker/split-web/**` — jointly own the
    alternate edge/API/static-web composition; the local README documents its
    request ownership and validation commands.
  - `.github/workflows/release.yml` and
    `lib/foundation/fabro-dev/src/commands/docker_build.rs` — release and local
    developer consumers both stage the same per-architecture context consumed
    by the root Dockerfile.

### `fabro-repository-automation` — Fabro-native repository automation

- **Assigned file count:** 41
- **Purpose:** Configures Fabro's own development environment and supplies the
  named workflow graphs, prompts, permissions, and project defaults used for
  repository maintenance, integration demonstrations, and workflow examples.
- **Globs:**
  - `.fabro/Dockerfile`
  - `.fabro/project.toml`
  - `.fabro/workflows/**`
- **Exclude globs:**
  - `.fabro/workflows/goal/workflow.svg`
- **Entry points:**
  - `.fabro/project.toml`
  - `.fabro/workflows/*/workflow.toml`
  - `.fabro/workflows/*/workflow.fabro`
  - `.fabro/workflows/implement-plan/workflow.fabro`
  - `.fabro/workflows/patch-cves/workflow.fabro`
  - `.fabro/workflows/pr-simplify/workflow.fabro`
  - `.fabro/workflows/smoke/workflow.fabro`
- **Owns:** repository-level pull-request defaults; the `fabro-dev` Daytona
  environment and resource/lifecycle labels; its browser-capable Rust/Bun
  development image; named workflow graph catalog; workflow-local prompts;
  GitHub integration permissions; and repository verification/maintenance
  command sequences.
- **Depends-on candidates:** `fabro-cli`, `fabro-config`, `fabro-workflow`,
  `fabro-graphviz`, `fabro-sandbox`, `fabro-github`,
  `fabro-build-tooling`, and `repository-development-policy`.
- **Evidence:**
  - `.fabro/project.toml` — is the project-level Fabro configuration entry,
    selecting the Daytona environment, `.fabro/Dockerfile`, resource limits,
    lifecycle, labels, and pull-request defaults.
  - `.fabro/Dockerfile` — supplies the clone-based workflow environment with
    Git, ripgrep, browser/desktop support, GitHub CLI, pinned Rust tooling,
    nextest, and Bun.
  - `.fabro/workflows/*/workflow.toml` — provides per-workflow graph selection,
    environment overrides, pull-request behavior, and GitHub token
    permissions.
  - `.fabro/workflows/*/workflow.fabro` — provides independently runnable
    Graphviz workflow entries for demos, human interaction, GitHub
    operations, implementation, verification, maintenance, and smoke tests.
  - `.fabro/workflows/implement-plan/workflow.fabro` — invokes the
    repository's Cargo/Bun verification contract and `cargo dev` generated-doc
    and SPA lifecycle, tying maintenance workflows to the same developer
    tooling as CI.
  - `.fabro/workflows/patch-cves/**` and
    `.fabro/workflows/pr-simplify/**` — pair bundled prompts with the explicit
    GitHub permissions and pull-request behavior needed by repository
    maintenance runs.
  - `AGENTS.md` — documents `fabro run <name>` as resolving
    `.fabro/workflows/<name>/workflow.toml`, establishing the catalog's common
    consumer.

### `coding-agent-automation` — Repository coding-agent automation

- **Assigned file count:** 11
- **Purpose:** Supplies repository-local code-review prompts, documentation
  and changelog skills, edit hooks, and an image-generation helper to external
  coding-agent clients.
- **Globs:**
  - `.ai/prompts/**`
  - `.claude/settings.json`
  - `.claude/skills/**`
  - `bin/agent/**`
- **Exclude globs:**
  - `.claude/skills/*/watermark`
- **Entry points:**
  - `.ai/prompts/code-review-fast.md`
  - `.ai/prompts/code-review-deep-1.md`
  - `.claude/skills/changelog/SKILL.md`
  - `.claude/skills/docs/SKILL.md`
  - `.claude/settings.json`
  - `bin/agent/imagegen`
- **Owns:** fast and multi-stage deep code-review orchestration prompts;
  changelog selection and MDX formatting procedure; code-to-documentation
  mapping and update procedure; post-edit Rust formatting hook; and the
  command-line Gemini image request/output flow.
- **Depends-on candidates:** `public-documentation` and
  `public-release-history` are data/format consumers of the two skills; the
  remaining prompts and helper use external agent, GitHub CLI, Git, and Gemini
  interfaces rather than product runtime components.
- **Evidence:**
  - `.ai/prompts/code-review-deep-{1,2,3}.md` — define a three-artifact review
    pipeline from candidate discovery through analysis and false-positive
    filtering.
  - `.ai/prompts/code-review-fast.md` — defines pull-request eligibility,
    parallel review/confidence filtering, and the GitHub comment output
    contract.
  - `.claude/skills/changelog/SKILL.md` and its references — define the
    Git-history-to-Mintlify changelog workflow and output format.
  - `.claude/skills/docs/SKILL.md` and its mapping reference — define the
    Git-history-to-public-doc update workflow and map implementation paths to
    published documentation pages.
  - `.claude/settings.json` — registers the repository-local post-edit Rust
    formatting hook.
  - `bin/agent/imagegen` — is an executable helper that loads repository
    environment credentials, calls the Gemini image endpoint, and writes the
    decoded image.

## Evidence-backed exclusions

### Vendored Rust style-guide skill

- **Glob:** `.fabro/skills/rust-style-guide/**`
- **Count:** 44 tracked files.
- **Reason/evidence:** Commit `9af0296469b902c9780a983dee5bee07b0abbcdf`
  explicitly records all 44 files as vendored from
  `brynary/rust-style-guide` commit `8fd2a4f`, trimmed to the runtime skill
  payload. The files are copied policy/procedure content rather than authored
  implementation owned by this repository. The skill entry point also routes
  readers across the copied `guidelines/**` and `workflows/**` payload.

### Dependency resolution outputs

- **Paths:** `Cargo.lock`, `bun.lock`
- **Count:** two tracked files.
- **Reason/evidence:** These are machine-maintained dependency resolution
  snapshots. `lib/foundation/fabro-dev/src/commands/release.rs` explicitly
  runs `cargo update --workspace` and stages `Cargo.lock`, while all CI/release
  consumers use Cargo `--locked` or Bun `--frozen-lockfile`; the manifests and
  policies that generate and consume them remain assigned.

### Skill watermarks

- **Glob:** `.claude/skills/*/watermark`
- **Count:** two tracked files.
- **Reason/evidence:** Each file is a commit SHA used as generated progress
  state. `.claude/skills/changelog/SKILL.md` and
  `.claude/skills/docs/SKILL.md` each explicitly instruct their workflow to
  overwrite its watermark with `git rev-parse HEAD`.

### Project configuration backup

- **Path:** `.fabro/project.toml.bak`
- **Count:** one tracked file.
- **Reason/evidence:** The `.bak` file preserves the previous inline
  `[run.sandbox.daytona]`/snapshot configuration, while
  `.fabro/project.toml` is the canonical current project configuration and
  points to the separate `.fabro/Dockerfile`.

### Non-runtime workflow and review assets

- **Paths:** `.fabro/workflows/goal/workflow.svg`, `.github/assets/**`
- **Count:** three tracked files: one SVG workflow illustration and two PNG
  screenshots.
- **Reason/evidence:** The goal workflow's runtime TOML points to
  `workflow.fabro`, not the SVG, and the SVG has no tracked runtime consumer.
  Commit `ac32963538f4441d40a47fcfcd868ca290d2b899` identifies the two PNGs as
  live screenshots captured for a web-feature pull request and says they are
  safe to remove from that change; no tracked source references them at the
  assessed revision.

### Canonical-file symlink aliases

- **Paths:** `CLAUDE.md`, `install.sh`, `install.md`
- **Count:** three tracked symlinks.
- **Reason/evidence:** Git records each with mode `120000`. Their targets are
  `AGENTS.md`, `apps/marketing/public/install.sh`, and
  `apps/marketing/public/install.md`, respectively. The canonical instruction
  file is assigned above, while the canonical install resources are owned by
  the web scout's `fabro-marketing-site`; excluding aliases prevents the same
  content from being assessed twice.

### Root overview and legal metadata

- **Paths:** `README.md`, `LICENSE.md`
- **Count:** two tracked files.
- **Reason/evidence:** `README.md` is the repository/product landing document
  and routes readers to the public installation and documentation surfaces;
  it does not define an independently executable or state-owning boundary.
  `LICENSE.md` is the repository's MIT legal text. Neither should form a
  quality-scored implementation component on its own.

## Coverage ledger

| Classification | Files |
| --- | ---: |
| `repository-development-policy` | 11 |
| `repository-ci` | 3 |
| `release-distribution-automation` | 4 |
| `container-packaging-and-deployment` | 16 |
| `fabro-repository-automation` | 41 |
| `coding-agent-automation` | 11 |
| Vendored Rust style-guide skill | 44 |
| Dependency resolution outputs | 2 |
| Skill watermarks | 2 |
| Project configuration backup | 1 |
| Non-runtime workflow and review assets | 3 |
| Canonical-file symlink aliases | 3 |
| Root overview and legal metadata | 2 |
| **Total** | **143** |

Computed scope coverage:

- **Relevant tracked files:** 143
- **Assigned to proposed components:** 86
- **Excluded with evidence:** 57
- **Unmapped:** 0

The component and exclusion patterns above were resolved against the assessed
revision's `git ls-tree` inventory. They are disjoint, and
`86 + 57 + 0 = 143`.

## Open boundary questions

1. Should `repository-development-policy` remain one repository-wide
   developer contract, or should the final map separate executable
   workspace/tool configuration from the contributor/agent governance in
   `AGENTS.md` and `CONTRIBUTING.md`?
2. Should `container-packaging-and-deployment` split into an image-packaging
   component and an operator Compose-deployment component? The root
   `Dockerfile` has a release/local-build lifecycle, while the Compose/Caddy
   files own runtime topology, but both share the image and entrypoint
   contract.
3. Should the explicitly named split-web proof-of-concept remain inside the
   container deployment component, become a separate experimental deployment
   component, or be excluded as non-production material?
4. Should `.fabro/project.toml` and `.fabro/Dockerfile` remain with the named
   workflow catalog? They share the Fabro project/run consumer today, but the
   environment image and project defaults could change independently from
   individual graphs.
5. Should the small `.ai`, `.claude`, and `bin/agent` families remain grouped
   as `coding-agent-automation`, or does the final map need separate
   review-automation and documentation-maintenance boundaries despite their
   small file counts?
6. Should root `README.md` remain excluded as repository overview metadata,
   or should it be folded into the docs scout's `public-documentation`
   component even though it sits outside `docs/**`?
