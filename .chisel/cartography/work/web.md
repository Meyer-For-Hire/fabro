# JavaScript/TypeScript cartography proposal

Assessed revision: `2bcf94fed8a9b429f18d9196fa824711d6f4cb0a`

Owned scout scope: every tracked file under `apps/**` and
`lib/packages/**` at the assessed revision. `package.json`, `bun.lock`, and
`docs/public/api-reference/fabro-api.yaml` were consulted only as dependency
evidence and are not included in the scope counts. Applicable repository
instructions are `AGENTS.md` (the `CLAUDE.md` project instructions) and
`CONTRIBUTING.md`.

## Boundary decisions

- `apps/fabro-web` contains three coherent assessable responsibilities, not
  just one directory-shaped component:
  - the normal-mode React application, shared browser runtime, and bundle
    production;
  - the alternate first-run installation mode, with its own route graph,
    reducer/form lifecycle, session token, API facade, and focused tests;
  - the workflow playground subtree, which explicitly defines a standalone
    prop boundary and owns a browser-persisted workflow draft, graph
    simulation, chat adapter, and generated project files.
- The marketing site and Remotion project are separate applications. Each has
  its own package manifest, framework entry point, build command, assets, and
  output/deployment lifecycle.
- The hand-written Fabro API client generation package is an assessable
  component, but its checked-in `src/**` tree is generator output and should
  be excluded from assessment. The distinction and counts are documented
  below.

## Proposed components

### `fabro-web-app` — Fabro browser application

- **Purpose:** Build and run the normal-mode React SPA for run operations,
  chats, automations, insights, settings, profiles, and their shared browser
  infrastructure.
- **Tracked files:** 309.
- **Globs:** `apps/fabro-web/**`
- **Exclude globs (assigned to sibling components):**
  `apps/fabro-web/app/components/playground/**`,
  `apps/fabro-web/app/install-*`,
  `apps/fabro-web/app/mode.ts`,
  `apps/fabro-web/app/mode.test.ts`,
  `apps/fabro-web/app/hooks/use-install-effects.ts`
- **Entry points:** `apps/fabro-web/scripts/build.ts:main`,
  `apps/fabro-web/app/entry.tsx`,
  `apps/fabro-web/app/router.tsx:routes`,
  `apps/fabro-web/index.template.html`
- **Owns:** Browser bundle assembly and content-hashed publication under
  `dist/`; the normal-mode route graph; run, chat, automation, insight,
  settings, and profile UX; shared API/query/mutation/event-stream adapters;
  app-wide layouts, components, hooks, browser view preferences, and public
  UI assets.
- **Depends-on candidates:** `fabro-web-install` (alternate route graph
  composed by the browser entry), `fabro-workflow-playground` (route-level
  feature composition), `fabro-api-client-generation` (through its generated
  package output), and the parent map's server HTTP/API-contract component
  (likely `fabro-server` and/or `fabro-api`).
- **Evidence:**
  - `apps/fabro-web/package.json` — declares a private React application,
    custom build/dev commands, browser dependencies, tests, and a workspace
    dependency on `@qltysh/fabro-api-client`.
  - `apps/fabro-web/scripts/build.ts:main` — bundles
    `app/entry.tsx`, compiles Tailwind CSS, copies public/worker assets, writes
    the HTML shell, publishes a content-addressed build, and provides the
    watch lifecycle.
  - `apps/fabro-web/app/entry.tsx` — creates the React root, browser router,
    SWR runtime, build-version guard, and toaster, then selects the normal or
    install route graph.
  - `apps/fabro-web/app/router.tsx:routes` — explicitly composes the
    normal-mode route tree for chats, playground, automations, runs, insights,
    settings, and profile pages beneath the app shell.
  - `apps/fabro-web/app/lib/api-client.ts` and
    `apps/fabro-web/app/lib/queries.ts` — form the browser-side API and query
    integration boundary used across normal-mode routes.

### `fabro-web-install` — First-run browser installer

- **Purpose:** Drive the browser-only first-run installation workflow that
  configures server URL, object storage, sandbox, LLM providers, and GitHub
  before finishing installation.
- **Tracked files:** 14.
- **Globs:** `apps/fabro-web/app/install-*`,
  `apps/fabro-web/app/mode.ts`,
  `apps/fabro-web/app/mode.test.ts`,
  `apps/fabro-web/app/hooks/use-install-effects.ts`
- **Exclude globs:** none.
- **Entry points:** `apps/fabro-web/app/install-router.tsx:installRoutes`,
  `apps/fabro-web/app/install-app.tsx:InstallApp`,
  `apps/fabro-web/app/mode.ts:resolveFabroMode`
- **Owns:** The `install` browser mode; installation step navigation and form
  reducer state; install-session query lifecycle; the
  `fabro-install-token` session-storage value; URL token/GitHub callback
  consumption; install-specific validation, persistence, finish, and restart
  health-poll behavior.
- **Depends-on candidates:** `fabro-web-app` for the shared root, common UI,
  hooks, and browser API transport; `fabro-api-client-generation` through
  generated Install DTOs/API methods; and the parent map's server
  installation/API-contract component.
- **Evidence:**
  - `apps/fabro-web/app/entry.tsx` — selects `installRoutes` instead of the
    normal `routes` when `window.__FABRO_MODE__` resolves to `install`.
  - `apps/fabro-web/app/install-router.tsx:installRoutes` — defines a separate
    catch-all route graph centered on `InstallApp`.
  - `apps/fabro-web/app/install-app.tsx` — owns the seven-step install flow
    and its installation-specific reducer/form state.
  - `apps/fabro-web/app/install-api.ts` — wraps generated Install API methods
    and owns the session-storage token contract.
  - `docs/public/api-reference/fabro-api.yaml` — dependency evidence outside
    owned scope: declares the `Install` tag as the first-run browser install
    workflow.

### `fabro-workflow-playground` — Browser workflow playground

- **Purpose:** Provide a self-contained workflow drafting, simulation, chat,
  visualization, file-generation, download, and run-launch surface.
- **Tracked files:** 44.
- **Globs:** `apps/fabro-web/app/components/playground/**`
- **Exclude globs:** none.
- **Entry points:**
  `apps/fabro-web/app/components/playground/playground.tsx:Playground`
- **Owns:** The `WorkflowDraft` graph schema and reducer; the versioned
  `fabro:playground:draft:v1` local-storage document; draft validation and
  animation; workflow simulation state; canvas rendering; playground chat/SSE
  adaptation; `workflow.fabro`, TOML, and README rendering; download and
  real-run launch controls.
- **Depends-on candidates:** `fabro-web-app` for a small set of shared chat,
  graph-theme, dynamic-import, event-hook, and test utilities; and the parent
  map's server component for `/api/v1/playground/chat` and `/api/v1/runs`.
- **Evidence:**
  - `apps/fabro-web/app/components/playground/playground.tsx:Playground` —
    exposes `chatEndpoint`, `authMode`, and optional redirect props and states
    that the subtree is framed for re-embedding without the app shell or
    app-wide stores.
  - `apps/fabro-web/app/components/playground/state/draft.ts:WorkflowDraft` —
    defines the complete workflow document and describes it as a
    self-contained, re-embeddable island.
  - `apps/fabro-web/app/components/playground/state/persist.ts:usePlaygroundDraft`
    — owns reducer-driven browser persistence and the versioned storage key.
  - `apps/fabro-web/app/components/playground/chat/runtime.ts:createPlaygroundAdapter`
    — adapts chat turns and streamed tool calls into draft changes.
  - `apps/fabro-web/app/routes/playground.tsx:PlaygroundRoute` — integration
    evidence in the sibling app component: mounts the feature at
    `/playground` and supplies its endpoint/auth contract.

### `fabro-marketing-site` — Fabro marketing site

- **Purpose:** Build and deploy the public Fabro site, including product
  landing content, blog, roadmap, showcase, install resources, and social
  metadata/assets.
- **Tracked files:** 51 assigned; two generated Vercel link files excluded
  below.
- **Globs:** `apps/marketing/**`
- **Exclude globs:** `apps/marketing/.vercel/**`
- **Entry points:** `apps/marketing/astro.config.mjs`,
  `apps/marketing/src/pages/index.astro`,
  `apps/marketing/src/content.config.ts`
- **Owns:** Astro page routing and layout; global marketing presentation;
  blog, roadmap, and showcase content collections; workflow showcase
  rendering; public install script/instructions and brand/social assets;
  public redirects and Vercel deployment configuration.
- **Depends-on candidates:** none within this scout's assessable components.
  It has framework dependencies and renders workflow graphs via Viz.js but
  does not import another repository workspace.
- **Evidence:**
  - `apps/marketing/package.json` — declares an independent private Astro
    application with dev/build/preview lifecycle.
  - `apps/marketing/astro.config.mjs` — integrates React/Tailwind and defines
    public redirects.
  - `apps/marketing/src/content.config.ts` — defines separately typed roadmap,
    blog, and showcase content collections whose source documents are owned
    under `src/content/**`.
  - `apps/marketing/src/pages/**` — Astro's file-based entries own the landing,
    roadmap, blog, and showcase URL surfaces.
  - `apps/marketing/vercel.json` — owns production redirect behavior for the
    deployed site.

### `fabro-remotion-video` — Fabro Remotion composition

- **Purpose:** Render the branded `FabroIntro` motion-graphics video.
- **Tracked files:** 9.
- **Globs:** `apps/remotion/**`
- **Exclude globs:** none.
- **Entry points:** `apps/remotion/src/index.ts`,
  `apps/remotion/src/Root.tsx:RemotionRoot`,
  `apps/remotion/src/FabroIntro.tsx:FabroIntro`
- **Owns:** The `FabroIntro` composition registration, 1920x1080/30fps/150
  frame timeline, image-format configuration, logo animation, brand assets,
  and `out/intro.mp4` render lifecycle.
- **Depends-on candidates:** none within this scout's assessable components.
- **Evidence:**
  - `apps/remotion/package.json` — declares an independent Remotion project
    whose studio and render/build scripts target composition `FabroIntro`.
  - `apps/remotion/src/index.ts` — registers the Remotion root.
  - `apps/remotion/src/Root.tsx:RemotionRoot` — declares the composition ID,
    component, dimensions, frame rate, and duration.
  - `apps/remotion/src/FabroIntro.tsx:FabroIntro` — owns the composition's
    animation timeline and use of the two local public assets.

### `fabro-api-client-generation` — TypeScript API client generation contract

- **Purpose:** Configure, normalize, and type-check the generated
  TypeScript/Axios client for the Fabro OpenAPI contract.
- **Tracked files:** 6 assigned; 554 generated/output files excluded below.
- **Globs:** `lib/packages/fabro-api-client/package.json`,
  `lib/packages/fabro-api-client/openapitools.json`,
  `lib/packages/fabro-api-client/scripts/**`,
  `lib/packages/fabro-api-client/tests/**`,
  `lib/packages/fabro-api-client/tsconfig.json`
- **Exclude globs:** `lib/packages/fabro-api-client/src/**`
- **Entry points:**
  `lib/packages/fabro-api-client/package.json:scripts.generate`,
  `lib/packages/fabro-api-client/scripts/normalize-generated.ts`
- **Owns:** OpenAPI Generator CLI/template options and version selection;
  output location; deterministic whitespace normalization; strict TypeScript
  compilation of output; hand-written exhaustiveness/invariant checks for
  generated discriminated unions and API shapes.
- **Depends-on candidates:** the parent map's `fabro-api`/OpenAPI-contract
  component, whose source is
  `docs/public/api-reference/fabro-api.yaml`.
- **Evidence:**
  - `lib/packages/fabro-api-client/package.json` — `generate` invokes pinned
    OpenAPI Generator CLI `2.20.2`, reads the repository OpenAPI YAML, selects
    `typescript-axios` with separate model/API packages and tag-based APIs,
    writes to `src`, then runs the normalizer.
  - `lib/packages/fabro-api-client/openapitools.json` — selects generator
    version `7.20.0`.
  - `lib/packages/fabro-api-client/scripts/normalize-generated.ts` — is
    explicitly hand-written normalization logic and scans exactly
    `src/**/*.ts`.
  - `lib/packages/fabro-api-client/tests/principal-exhaustive.ts` and
    `tests/reasoning-output-invariant.ts` — hand-written compile-time
    assertions over generated types.
  - `lib/packages/fabro-api-client/tsconfig.json` — type-checks both
    `src/**/*` and `tests/**/*`.

## Evidence-backed exclusions

### Generated TypeScript/Axios client output

- **Glob:** `lib/packages/fabro-api-client/src/**`
- **Count:** 554 tracked files: 551 TypeScript files and three generator
  bookkeeping/ignore files
  (`.openapi-generator/FILES`, `.openapi-generator/VERSION`, and
  `.openapi-generator-ignore`).
- **Reason/evidence:**
  - The hand-written package script directs OpenAPI Generator to `-o src`.
  - 550 of the 551 TypeScript files carry the literal header
    `NOTE: This class is auto generated by OpenAPI Generator` and
    `Do not edit the class manually`.
  - The only TypeScript file without that header is
    `src/models/index.ts`; it is explicitly named in
    `src/.openapi-generator/FILES`.
  - `src/.openapi-generator/FILES` contains 545 generated path entries and
    `src/.openapi-generator/VERSION` records `7.20.0`.
  - Six additional TypeScript files are not in that `FILES` snapshot, but
    each has the same auto-generation marker:
    `models/daytona-network-layer-one-of-allow-list.ts`,
    `models/daytona-network-layer-one-of.ts`,
    `models/daytona-network-layer.ts`, `models/docker-settings.ts`,
    `models/run-projection-checkpoints-inner-inner.ts`, and
    `models/sandbox-provider.ts`.
  - Therefore the stable exclusion is the output-root glob `src/**`, not only
    the metadata's current list or only marker-bearing files.

### Vercel CLI link metadata

- **Glob:** `apps/marketing/.vercel/**`
- **Count:** 2 tracked files.
- **Reason/evidence:** `apps/marketing/.vercel/README.txt` states that the
  folder is automatically created when linking a directory to a Vercel
  project, describes `project.json` as the linked project/team IDs, and says
  the directory should not be committed/shared. These are generated local
  deployment-link records rather than marketing-site source.

## Computed coverage

| Category | Count |
| --- | ---: |
| Tracked files in owned scope | 989 |
| Assigned to proposed components | 433 |
| Evidence-backed excluded | 556 |
| Unmapped | 0 |

Assigned counts are `309 + 14 + 44 + 51 + 9 + 6 = 433`. Excluded counts are
`554 + 2 = 556`. The total is `433 + 556 + 0 = 989`. No file is claimed by
two proposed components.

## Open questions

1. Should the 14-file first-run installer remain a separate component in the
   final map? Its alternate route graph, lifecycle, state, and API boundary
   support the split, but it imports shared web UI/runtime code while the
   shared browser entry imports its route graph, so source dependencies are
   reciprocal at composition time.
2. Should `apps/fabro-web/app/routes/playground.tsx` remain assigned to
   `fabro-web-app` as the app-level integration adapter (the proposal here),
   or move into `fabro-workflow-playground`? Keeping the 44-file subtree as
   the playground boundary matches its own standalone/re-embedding contract.
3. Which final Rust component ID owns
   `docs/public/api-reference/fabro-api.yaml` and the server endpoints:
   `fabro-api`, `fabro-server`, or a separately reconciled API-contract
   component? The JavaScript dependencies above should be renamed to that
   final ID.
