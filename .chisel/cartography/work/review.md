# Independent cartography review

Assessed revision: `2bcf94fed8a9b429f18d9196fa824711d6f4cb0a`

Reviewed artifact:
`.chisel/cartography/work/candidate-codebase-map.json`.

This review is limited to component boundaries, dependencies, evidence, and
file disposition. It does not assess implementation quality.

## Independent inventory check

I resolved the fixed tree with `git ls-tree -r --name-only` and matched every
component glob, component exclusion, global exclusion, and declared unmapped
path independently of the candidate's renderer.

- Tracked files: **3,104**
- Candidate claims: **2,256**
- Candidate global exclusions: **848**
- Candidate unmapped files: **0**
- Files without a disposition: **0**
- Files claimed by multiple components, or both claimed and globally excluded:
  **0**
- Overlap between separate global-exclusion entries: **0**

The candidate's mechanical accounting is therefore correct as written.
Component IDs are unique, all named dependencies resolve, all globs resolve,
and every evidence/entry-point path exists in the fixed tree. A symbol-text
check also found no missing Rust/TypeScript symbols among the qualified
references; the one non-symbol qualifier is the valid JSON property reference
`package.json:scripts.generate`.

Mechanical coverage does not settle whether each disposition or boundary is
architecturally correct. The supported corrections below change the
classification of 15 files but leave the total inventory unchanged.

## Supported corrections

### 1. Move `docs/internal/assets/**` from global exclusion to `unmapped_files`

All 15 files under `docs/internal/assets/**` are currently excluded because
they have no tracked consumer or documented update workflow. That establishes
that ownership is unresolved; it does not establish that the SVG, HTML, and
raster files are generated, vendored, build output, or historical records.
The candidate's own open question likewise asks whether they are maintained
brand sources.

Until that question is answered, exclusion asserts more than the evidence
supports. Preserve the open question and list the 15 exact tracked paths as
unmapped. This changes coverage to **2,256 assigned, 833 excluded, 15
unmapped**.

### 2. Restore `fabro-spa` as a separate component

`lib/apps/fabro-spa/Cargo.toml` declares an independent Rust package with the
specific responsibility “Embedded production SPA assets for Fabro.”
`lib/apps/fabro-spa/src/lib.rs` exposes the server-facing `get` and
`AssetBytes` interface, owns compile-time embedding and hashes, and is consumed
directly by `lib/apps/fabro-server/src/static_files.rs` and
`lib/apps/fabro-server/src/csp.rs`.

Folding those two assigned files into `fabro-web-app` combines a browser
application with a Rust server adapter that has a different entry point,
consumer, toolchain, and reason to change. It also turns the precise dependency
`fabro-server -> fabro-spa` into the over-broad
`fabro-server -> fabro-web-app`.

Add a `fabro-spa` component for `lib/apps/fabro-spa/Cargo.toml` and
`lib/apps/fabro-spa/src/**`; retain `lib/apps/fabro-spa/assets/**` as the
evidence-backed generated-output exclusion. Remove those assigned paths from
`fabro-web-app`, replace the server's web-app edge with
`fabro-server -> fabro-spa`, and let the SPA refresh tooling express the
build-time connection to the browser app.

The two-file size is not by itself a reason to hide this package: it has a
manifest, public interface, owned compile-time lifecycle, and independent
consumer boundary, the same kind of evidence used to retain other small Rust
components in the candidate.

### 3. Separate `fabro-build-support` from `fabro-build-tooling`

`lib/foundation/build-support/Cargo.toml` is an independent package whose only
responsibility is build-script Git/profile metadata.
`lib/foundation/build-support/git_metadata.rs` exposes that public API, and
the direct consumers are `lib/apps/fabro-cli/build.rs` and
`lib/apps/fabro-server/build.rs`.

The remaining `fabro-dev` package is an executable repository-development CLI
with SPA, documentation, release, benchmark, and container command
lifecycles. Combining these packages hides shared compile-time infrastructure
inside an unrelated command application; the candidate purpose has to join
“runs repository ... automation” with “supplies compile-time Git metadata” to
cover both.

Add a `fabro-build-support` component for
`lib/foundation/build-support/**`. Keep `lib/foundation/fabro-dev/**`,
`test/bin/release_test.sh`, and `test/analysis/bench-tests-diff.sql` in the
existing development-tooling component. Add
`fabro-cli -> fabro-build-support` and
`fabro-server -> fabro-build-support`, which are explicit Cargo build
dependencies.

### 4. Correct the shared fixture dependency direction

`workflow-test-corpus` is inert input data. The candidate evidence identifies
the readers:

- `fabro-test` resolves files beneath `../../../test/`;
- `fabro-cli` source/tests install the root and template fixtures;
- `fabro-graphviz` and `fabro-validate` enumerate
  `test/dot-compatibility`;
- `fabro-workflow` enumerates `test/attractor`.

Those consumers depend on the corpus, just as the generated API clients depend
on their source contract. The candidate currently records the reverse and
also names `fabro-template`, for which there is no direct corpus read.

Make `workflow-test-corpus.depends_on` empty, add
`workflow-test-corpus` to the five direct consumer components above, and omit
the unsupported `fabro-template` edge. This correction does not require
redistributing the shared files.

### 5. Add direct operational dependencies omitted from
`fabro-build-tooling`

The candidate's purpose and evidence include operations whose source contains
explicit repository-component dependencies, but its dependency list contains
only Cargo library dependencies:

- `docs_cli_reference.rs` invokes `fabro-cli` and writes
  `docs/public/reference/cli.mdx`;
- `docs_options_reference.rs` writes the same public-documentation surface;
- `spa_refresh.rs` invokes the build in `apps/fabro-web` and mirrors its output
  into `lib/apps/fabro-spa/assets`;
- `docker_build.rs` runs the root container build;
- `release.rs` reads and updates the root Cargo workspace contract.

Add dependencies from `fabro-build-tooling` to `fabro-cli`,
`public-documentation`, `fabro-web-app`, the restored `fabro-spa`,
`container-packaging-and-deployment`, and
`repository-development-policy`. These are the same operational dependency
kind already used for CI, release, repository-workflow, and documentation
components; omitting them only for the development CLI makes the graph
inconsistent.

### 6. Add `public-documentation -> public-release-history`

`docs/public/docs.json`, owned by `public-documentation`, enumerates every
changelog page and gives the collection its top-level publication surface.
The existing `public-release-history -> public-documentation` edge captures
the changelog's dependence on Mintlify presentation, but it omits the direct
navigation/configuration dependency in the other direction. Retain the
existing edge and add the reciprocal edge.

## Optional boundary questions

These are plausible alternatives, but the fixed revision does not require
them as corrections:

1. **First-run web installer.** The 14 install/mode files have a distinct
   router, reducer, API facade, storage token, and lifecycle, so a
   `fabro-web-install` component is supportable now; it does not need a
   separate binary entry point to qualify. On the other hand, it is selected
   by the shared browser entry and imports the app's common UI/runtime. For the
   recommended map, keep it in `fabro-web-app` and preserve this as an open
   boundary question. Splitting it would raise the component count by one.
2. **Shared workflow corpus ownership.** Its cross-crate consumers justify the
   shared corpus component. Distributing the root/template, Attractor, and DOT
   compatibility subcorpora to their consumers is also possible, but would
   make the DOT corpus arbitrarily owned by one of two readers. Retain the
   shared component unless later assessment proves its combined boundary
   noisy.
3. **Workflow, LLM, store, and server subcomponents.** The candidate's broad
   components have recognizable internal areas, but their crate facades,
   shared state, and integration lifecycles currently support the retained
   crate/service boundaries. No additional split is required at this
   revision.

No candidate component is supported for removal or merger. In particular, the
single-file OpenAPI contract and the small MCP, evaluation, CI, and release
components have independent source-of-truth, protocol, executable, or
publication lifecycles that justify their granularity.

## Recommended disposition

Apply the two supported package splits and retain the optional boundaries as
questions:

- **Recommended component count:** **70** (candidate 68, plus
  `fabro-spa` and `fabro-build-support`)
- **Relevant tracked files:** **3,104**
- **Assigned:** **2,256**
- **Excluded:** **833**
- **Unmapped:** **15** (`docs/internal/assets/**`, listed as exact paths)
- **Overlap or uncovered files:** **0**

The counts satisfy `2,256 + 833 + 15 = 3,104`. The optional installer split
would produce 71 components without changing coverage.
