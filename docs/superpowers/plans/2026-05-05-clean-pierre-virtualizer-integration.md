# Clean Pierre Virtualizer Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Files Changed tab use `@pierre/diffs` `Virtualizer` correctly and consistently as the scroll root for every non-empty file list.

**Architecture:** Treat the Files Changed tab as a full-height application surface instead of a document-scrolling page. The route renders one virtualized diff pane, backed by Pierre's worker pool, stable per-file props, and explicit Pierre cache keys. The app shell and `RunDetail` parent route both pass full-height layout through to child routes that opt in.

**Tech Stack:** React 19, React Router, Tailwind CSS, Bun build scripts, `@pierre/diffs` 1.1.15, `react-test-renderer`, Bun test.

---

## Summary

This is a greenfield cleanup, not a conservative compatibility patch. Remove the arbitrary file-count threshold and the optional `Virtualizer` fallback. Every non-empty Files Changed view should use Pierre's `Virtualizer` in the shape Pierre documents: a bounded scroll container with an inner content wrapper.

## Implementation

- [ ] Update `apps/fabro-web/app/layouts/app-shell.tsx` to support full-height route surfaces.
  - Add support for a route handle flag named `fullHeight`.
  - Detect it with `matches.some((m) => (m.handle as { fullHeight?: boolean } | undefined)?.fullHeight)`.
  - When enabled, make the outer app container `flex min-h-dvh flex-col`.
  - When enabled, make `<main>` `min-h-0 flex-1`.
  - When enabled, make the outlet wrapper `flex h-full min-h-0 flex-col`.
  - Preserve existing non-full-height behavior for other routes.

- [ ] Update `apps/fabro-web/app/routes/run-detail.tsx` to pass full-height layout through to full-height child routes.
  - Import `useMatches` from `react-router`.
  - Detect whether the active child route has `handle.fullHeight` with the same handle check used by `AppShell`.
  - When enabled, make the `RunDetail` root wrapper `flex h-full min-h-0 flex-col`.
  - When enabled, keep the breadcrumb, header, actions, and tab bar as fixed-height content with `shrink-0`.
  - When enabled, change the outlet wrapper around `<Outlet />` from document-flow layout to `mt-6 min-h-0 flex-1`.
  - Handle blocked runs explicitly. The current `isBlocked && pendingQuestions.length > 0` branch renders an `h-72` spacer after `<Outlet />`; that spacer must not be a flex sibling competing with the virtualized pane for height. For full-height child routes, omit the sibling spacer and expose a CSS custom property such as `--fabro-interview-dock-clearance: 18rem` on the outlet/root wrapper. For non-full-height child routes, preserve the existing spacer behavior.
  - Preserve existing document-flow classes when no active child route opts into `fullHeight`.

- [ ] Update `apps/fabro-web/app/routes/run-files.tsx`.
  - Change the route handle to:
    ```ts
    export const handle = { wide: true, fullHeight: true };
    ```
  - Remove the ad hoc `maybeVirtualizer` / fallback component.
  - Remove the `files.length > 20` branch.
  - Always render non-empty file lists through `VirtualizedDiffList`.
  - Extract a memoized `RunFileRow` component that owns placeholder, `MultiFileDiff`, and `PatchDiff` rendering.
  - Pass `runId` and `meta.to_sha` into each `RunFileRow` so Pierre file cache keys can include run/version identity.
  - Use `useMemo` inside `RunFileRow` for Pierre `oldFile`, `newFile`, and `options` props so freshness ticks and revalidation do not force unnecessary reparsing.
  - Add stable `cacheKey` values to Pierre `oldFile` and `newFile` props. Build each key from run id, `meta.to_sha` when available, file side, file name, and a deterministic content hash. Pierre's worker-pool cache uses `cacheKey`; object identity alone only helps React memoization and `MultiFileDiff`'s local `useMemo`.
  - Keep per-row Pierre `options` limited to row-specific render behavior such as `diffStyle` and `expandUnchanged`. Do not use per-component `theme` as the source of truth under `WorkerPoolContextProvider`; the theme belongs in the provider's `highlighterOptions`.
  - Keep `PatchDiff` rows on the public React `PatchDiff` component for now. `PatchDiffProps` accepts `patch` and `options` but does not expose a cache-key parameter, so memoize its `patch` and `options` inputs and accept that patch rows cannot get explicit worker-pool cache keys without switching to parsed metadata APIs.
  - Make every non-empty route wrapper participate in the full-height chain: root `h-full min-h-0 flex flex-col`; toolbar/banner area `shrink-0`; sidebar/diff row container `min-h-0 flex-1`; diff pane `min-h-0 flex-1`.
  - Ensure any interview-dock clearance is applied inside the scrollable diff content, not as a sibling below the virtualized pane. Use the CSS variable from `RunDetail` in the virtualizer content padding so the last file can scroll above the dock.

- [ ] Add `apps/fabro-web/app/routes/run-files/cache-keys.ts`.
  - Export a small deterministic synchronous string hash helper, such as FNV-1a, so cache keys can be computed during render without async browser crypto.
  - Export a `fileCacheKey` helper that produces keys shaped like:
    ```ts
    fabro-run-file:${runId}:${toSha ?? "no-sha"}:${side}:${path}:${contentHash}
    ```
  - Keep the helper independent of React so it is easy to unit test.

- [ ] Add `apps/fabro-web/app/routes/run-files/virtualized-diff-list.tsx`.
  - Import `Virtualizer` and `WorkerPoolContextProvider` from `@pierre/diffs/react`.
  - Import `workerFactory` from `../../lib/pierre-diffs-worker`.
  - Define stable module-level options:
    ```ts
    const poolOptions = { workerFactory };
    const highlighterOptions = { theme: "pierre-dark" };
    ```
  - Render:
    ```tsx
    export function VirtualizedDiffList({ children }: { children: React.ReactNode }) {
      return (
        <WorkerPoolContextProvider
          poolOptions={poolOptions}
          highlighterOptions={highlighterOptions}
        >
          <Virtualizer
            className="min-h-0 flex-1 overflow-auto pr-2"
            contentClassName="flex flex-col gap-2 pb-[calc(1rem+var(--fabro-interview-dock-clearance,0px))]"
          >
            {children}
          </Virtualizer>
        </WorkerPoolContextProvider>
      );
    }
    ```

- [ ] Update `apps/fabro-web/app/routes/run-files/file-tree-sidebar.tsx`.
  - Remove document-scroll-oriented sticky positioning.
  - Use a layout that belongs inside the route's full-height flex row:
    ```tsx
    className="flex min-h-0 w-72 shrink-0 flex-col gap-2 self-stretch"
    ```
  - Keep the tree itself `min-h-0 flex-1 overflow-hidden`.
  - Preserve filtering, selection, status icons, and current public props.

- [ ] Update `apps/fabro-web/app/routes/run-files/states.tsx`.
  - Change `FileTreeSidebarSkeleton` from document-scroll-oriented sticky viewport sizing to the same full-height flex shape as the real sidebar:
    ```tsx
    className="flex min-h-0 w-72 shrink-0 flex-col self-stretch rounded-md border border-line bg-panel/40 motion-safe:animate-pulse"
    ```
  - Change `LoadingSkeleton`'s `reserveSidebar` row to `min-h-0 flex-1 gap-4` and make the diff skeleton pane `min-h-0 flex-1`.
  - Keep the top toolbar skeleton as `shrink-0` so initial loading uses the same height chain as the loaded route.

- [ ] Add `apps/fabro-web/app/lib/pierre-diffs-worker.ts`.
  - Implement:
    ```ts
    export function workerFactory(): Worker {
      return new Worker(
        "/assets/pierre-diffs-worker/worker-portable.js",
        { type: "module" },
      );
    }
    ```

- [ ] Update `apps/fabro-web/scripts/build.ts`.
  - Copy Pierre worker assets after creating `dist/assets`.
  - Source files:
    - `node_modules/@pierre/diffs/dist/worker/worker-portable.js`
    - every sibling file matching `node_modules/@pierre/diffs/dist/worker/wasm-*.js`
  - Destination directory:
    - `dist/assets/pierre-diffs-worker/`
  - Do not hard-code Pierre's hashed WASM filename. `wasm-BaDzIkIn.js` is the current `@pierre/diffs` 1.1.15 artifact, but that hash can change when the package updates.
  - Use `worker-portable.js`, not `worker.js`, because `worker.js` contains bare imports that the browser cannot resolve from a static URL in this Bun build.

## Tests

- [ ] Add `apps/fabro-web/app/routes/run-files/virtualized-diff-list.test.tsx`.
  - Mock `@pierre/diffs/react`.
  - Assert `WorkerPoolContextProvider` receives `poolOptions.workerFactory`.
  - Assert `WorkerPoolContextProvider` receives `highlighterOptions: { theme: "pierre-dark" }`.
  - Assert `Virtualizer` receives `className` containing `min-h-0`, `flex-1`, and `overflow-auto`.
  - Assert `Virtualizer` receives `contentClassName` containing `flex`, `flex-col`, `gap-2`, and the interview-dock clearance CSS variable.
  - Use `gap-2` because Pierre's installed default virtual `fileGap` is 8. If implementation changes back to a 16px visual gap, add a shared metrics object with `fileGap: 16` and pass it consistently to `MultiFileDiff` and `PatchDiff`.

- [ ] Add or extend tests for `apps/fabro-web/app/routes/run-detail.tsx`.
  - Keep the existing pure unit tests.
  - Add render tests using a data-router harness, because `useMatches` only works under a data router. Use `createMemoryRouter` and `RouterProvider`, not `MemoryRouter`.
  - Build route objects with a parent `/runs/:id` route rendering `RunDetail` and a child `files` route whose route object has `handle: { fullHeight: true }`.
  - Inject route params the same way `apps/fabro-web/app/router.tsx` does. Use a local test wrapper that calls `useParams()` and renders:
    ```tsx
    <RunDetail params={params as { id: string }} />
    ```
    or reuse the app route-wrapper pattern.
  - Wrap the rendered router with the real `ToastProvider` and `DemoModeProvider`.
  - Mock run queries, run-question queries, run-events, and mutations so the component can render deterministically.
  - Assert the `RunDetail` root wrapper receives `h-full`, `min-h-0`, `flex`, and `flex-col`.
  - Assert the outlet wrapper receives `min-h-0` and `flex-1`.
  - Cover the blocked-run full-height branch and assert the old `h-72` spacer is not rendered as an outlet sibling while the dock still renders and the clearance CSS variable is set.
  - Cover the default branch and assert the document-flow wrapper remains unchanged for non-full-height child routes.

- [ ] Add `apps/fabro-web/app/routes/run-files/cache-keys.test.ts`.
  - Assert identical run id, SHA, side, path, and contents produce the same key.
  - Assert changing file contents changes the key.
  - Assert changing file name or side changes the key.
  - Assert `null`/missing `toSha` still produces a deterministic key.

- [ ] Add `apps/fabro-web/app/routes/run-files.render.test.tsx`.
  - Mock `../lib/queries` to return run metadata and files.
  - Mock Pierre components with lightweight test doubles.
  - Render with a real React Router harness, not bare component rendering:
    ```tsx
    <ToastProvider>
      <MemoryRouter initialEntries={["/runs/run_1/files"]}>
        <Routes>
          <Route path="/runs/:id/files" element={<RunFiles />} />
        </Routes>
      </MemoryRouter>
    </ToastProvider>
    ```
  - Do not mock `useParams` or `useToast`; the harness should exercise the real route and toast boundaries.
  - Render a 1-file payload and assert `Virtualizer` is called once.
  - Render a 27-file payload and assert `Virtualizer` is still called once.
  - Assert every file row exposes `data-run-file-row="true"`.
  - Assert `MultiFileDiff` receives the expected `diffStyle`.
  - Do not assert `theme` on `MultiFileDiff` or `PatchDiff`; under `WorkerPoolContextProvider`, theme is controlled by `WorkerPoolContextProvider.highlighterOptions`.
  - Assert `MultiFileDiff` receives `oldFile.cacheKey` and `newFile.cacheKey`, and those keys are stable across an unrelated re-render.

- [ ] Update `apps/fabro-web/app/routes/run-files/pierre-smoke.test.tsx`.
  - Remove optional-export fallback logic.
  - Check callable exports for `MultiFileDiff`, `PatchDiff`, `Virtualizer`, and `WorkerPoolContextProvider`.

- [ ] Extend `apps/fabro-web/scripts/build.test.ts`.
  - Run the production build.
  - Assert `dist/assets/pierre-diffs-worker/worker-portable.js` exists.
  - Inspect `node_modules/@pierre/diffs/dist/worker/` for upstream `wasm-*.js` files.
  - If upstream has one or more `wasm-*.js` files, assert `dist/assets/pierre-diffs-worker/` contains the same `wasm-*.js` basenames. For the current installed `@pierre/diffs` 1.1.15 package, this means at least one `wasm-*.js` file should be copied.

## Verification

- [ ] Run targeted frontend tests:
  ```bash
  cd apps/fabro-web
  bun test app/routes/run-detail.test.ts app/routes/run-files.test.ts app/routes/run-files.render.test.tsx app/routes/run-files/cache-keys.test.ts app/routes/run-files/virtualized-diff-list.test.tsx app/routes/run-files/pierre-smoke.test.tsx scripts/build.test.ts
  ```

- [ ] Run typecheck:
  ```bash
  cd apps/fabro-web
  bun run typecheck
  ```

- [ ] Run production build:
  ```bash
  cd apps/fabro-web
  bun run build
  ```

- [ ] Browser-check the reported run:
  - Open `/runs/01KQW839X61WJ8MYWJB9P9WV66/files`.
  - Confirm Chrome stays responsive.
  - Confirm the diff list scrolls inside the Files Changed pane.
  - Confirm sidebar selection updates the hash and focuses the selected row.
  - Confirm keyboard `j` / `k` navigation still moves between file rows.
  - Confirm mobile unified mode remains usable.

## Assumptions

- Greenfield simplicity wins over preserving the old document-scroll behavior.
- Always using Pierre's virtualized path is preferred over threshold-based branching.
- Worker startup cost is acceptable for this tab; tune pool options later only if measured.
- No server, OpenAPI, Rust, or CSP changes are expected.
