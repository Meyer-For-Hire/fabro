# Run Files Diff Scope Picker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a run-files diff scope picker for committed, uncommitted, and all sandbox changes.

**Architecture:** Keep the existing files endpoint default behavior as committed changes. Add a `scope` query parameter for sandbox working-tree scopes, and use patch-backed response construction for scopes that include uncommitted state. The backend always prefers the run-owned sandbox, starts/resumes stopped sandboxes synchronously when needed, and falls back to stored `final_patch` with `source: "final_patch"` if the sandbox cannot be read. The frontend stores the selected scope in the URL, gives each scope an independent SWR cache key, and shows the scope picker only for sandbox-backed responses.

**Tech Stack:** Rust, Axum, OpenAPI/progenitor, Bun, React, SWR, generated TypeScript Axios client.

---

## Summary

Add a picker to the run files page with three modes:

- `Committed changes`: current behavior, comparing `base_sha..HEAD`.
- `Uncommitted changes`: staged, unstaged, and safe untracked changes in the run-owned sandbox.
- `All changes`: committed plus uncommitted sandbox changes from `base_sha` to the current working tree.

`Committed changes` remains the default. The endpoint prefers the run-owned sandbox for every scope. If the sandbox is unavailable after any needed start/resume attempt, the endpoint falls back to stored `final_patch` and returns `source: "final_patch"` so the frontend can omit the picker.

## Sandbox And Fallback Contract

Each workflow run owns one sandbox workspace, and that sandbox is expected to survive until the workflow run is deleted. The files endpoint should use that run-owned sandbox as the source of truth whenever it can be reached.

For any requested scope:

- If the run sandbox is running, read from it.
- If the run sandbox exists but is stopped, synchronously start/resume it, then read from it.
- Starting a stopped sandbox is allowed to block the request; providers are expected to resume in under 1 second.
- If the sandbox cannot be found, cannot be started, or cannot be read, fall back to stored `RunProjection.final_patch`.
- Sandbox-backed responses return `source: "sandbox"`.
- Fallback responses return `source: "final_patch"`.

When `source` is `final_patch`, the response represents the stored final/committed run diff. It does not semantically honor a requested `uncommitted` or `all` scope. The frontend should treat `source: "final_patch"` as fallback committed/final diff mode and omit the scope picker.

Invalid request state still fails as a request error rather than falling back. Examples: invalid `scope`, reserved `from_sha`/`to_sha`, invalid pagination, or an unknown run id.

## API And Backend Changes

- [ ] Add `scope` to `GET /api/v1/runs/{id}/files` in `docs/public/api-reference/fabro-api.yaml`.
  - Accepted values: `committed`, `uncommitted`, `all`.
  - Default: `committed`.
  - Keep `from_sha` and `to_sha` reserved and rejected when present.
- [ ] Add `source` to the `PaginatedRunFileList` response in `docs/public/api-reference/fabro-api.yaml`.
  - Accepted values: `sandbox`, `final_patch`.
  - `sandbox` means the response was computed from the run-owned sandbox.
  - `final_patch` means the response was computed from stored `RunProjection.final_patch` because the sandbox was unavailable.
- [ ] Run `cargo build -p fabro-api` so progenitor regenerates Rust API types.
- [ ] Add a server-side scope enum in `lib/crates/fabro-server/src/run_files.rs`.
  - `Committed` maps to the existing `base_sha..HEAD` materialization path.
  - `Uncommitted` maps to a new sandbox working-tree materialization path.
  - `All` maps to the same sandbox working-tree path with `base_sha` as the comparison base.
- [ ] Keep the existing committed path unchanged except for dispatching through the new scope enum and adding response `source`.
  - It may still reconnect/start/resume the sandbox.
  - It may still fall back to `RunProjection.final_patch` when sandbox diffing cannot use the stored base or the sandbox cannot be read.
- [ ] Implement sandbox-first response materialization for every scope.
  - Try to resolve the run-owned sandbox.
  - If the sandbox is stopped, start/resume it synchronously.
  - If sandbox materialization succeeds, return the requested scope with `source: "sandbox"`.
  - If sandbox materialization fails because the sandbox is unavailable, deleted, failed to start, or cannot execute/read git state, return the stored `final_patch` response with `source: "final_patch"`.
  - Do not use fallback for invalid request parameters or unknown runs.
- [ ] Implement a patch-backed working-tree path for `uncommitted` and `all`.
  - For `uncommitted`, use sandbox git diff output equivalent to `git diff HEAD`.
  - For `all`, use sandbox git diff output equivalent to `git diff <base_sha>`.
  - Include staged and unstaged changes.
  - Include safe untracked files discovered with sandbox git output equivalent to `git ls-files --others --exclude-standard`.
  - Exclude ignored files.
  - Validate repo-relative paths before reading any untracked file content.
  - Apply sensitive filtering before rendering path/content details.
  - Enforce per-file and aggregate size caps.
  - Render eligible text files as added-file patch sections.
  - Render sensitive, binary, symlink, unreadable, or oversized untracked files as placeholder entries using the same semantics as existing patch-section response code.
- [ ] Reuse existing response semantics where possible.
  - Return `PaginatedRunFileList`.
  - Keep `FileDiff[]` shape.
  - For patch-backed sandbox working-tree scopes, use `unified_patch` with null file contents, matching the degraded renderer branch.
  - Reuse existing sensitive, binary, symlink, submodule, truncation, file-count, and aggregate-byte behavior from the patch-section response code.
- [ ] Remove sandbox-unavailable `409 Conflict` behavior.
  - Any requested scope may fall back to `final_patch`.
  - The response `source` field tells clients whether the requested scope was actually served from the sandbox.
  - Do not imply that `final_patch` contains uncommitted or all sandbox changes.

## Frontend Changes

- [ ] Regenerate the TypeScript client.
  - Run: `cd lib/packages/fabro-api-client && bun run generate`.
- [ ] Update `apps/fabro-web/app/lib/queries.ts`.
  - Change `useRunFiles(id)` to `useRunFiles(id, scope)`.
  - Include `scope` in the SWR key so each mode caches independently.
  - Pass `scope` to `runOutputsApi.listRunFiles`.
- [ ] Update `apps/fabro-web/app/routes/run-files.tsx`.
  - Parse `scope` from the page URL.
  - Missing or invalid scope resolves to `committed`.
  - Persist picker changes with `?scope=committed`, `?scope=uncommitted`, or `?scope=all`.
  - Keep hash file deep links working with the existing `#file=...` format.
- [ ] Update `apps/fabro-web/app/routes/run-files/toolbar.tsx`.
  - Add a compact segmented picker with labels:
    - `All changes`
    - `Uncommitted`
    - `Committed`
  - Keep the existing diff layout toggle and refresh button.
  - Show the picker only when the files response has `source === "sandbox"`.
  - Omit the picker when the files response has `source === "final_patch"`.
- [ ] Handle fallback source in the run files page.
  - If `source` is `sandbox`, respect the selected URL scope and keep the selected scope visible.
  - If `source` is `final_patch`, treat the response as fallback committed/final diff mode.
  - If the URL requested `scope=uncommitted` or `scope=all` but the response has `source: "final_patch"`, do not show `Uncommitted` or `All changes` as selected.
  - Do not show the old sandbox-unavailable `409` inline error; fallback data is the intended response.

## Test Plan

- [ ] Add server/API tests for committed default behavior.
  - Request without `scope` still returns the same committed response as today.
  - Request with `scope=committed` uses the same path.
  - `from_sha` and `to_sha` are still rejected.
- [ ] Add server/API tests for fallback behavior.
  - `scope=committed` falls back to `final_patch` when sandbox access is unavailable.
  - `scope=uncommitted` falls back to `final_patch` when sandbox access is unavailable and returns `source: "final_patch"`.
  - `scope=all` falls back to `final_patch` when sandbox access is unavailable and returns `source: "final_patch"`.
  - `source` is `sandbox` when sandbox materialization succeeds.
  - `source` is `final_patch` when fallback materialization is used.
  - Invalid `scope` still returns a request validation error rather than falling back.
- [ ] Add server/API tests for stopped sandbox behavior.
  - A stopped sandbox is started/resumed synchronously and returns `source: "sandbox"`.
  - A stopped sandbox whose start/resume fails falls back to `final_patch` and returns `source: "final_patch"`.
- [ ] Add server/API tests for working-tree scopes.
  - `scope=uncommitted` includes staged changes.
  - `scope=uncommitted` includes unstaged changes.
  - `scope=uncommitted` includes safe untracked files.
  - `scope=all` includes committed plus sandbox working-tree changes.
  - Sensitive untracked paths render as sensitive placeholders.
  - Large untracked files render as truncated placeholders.
  - Ignored untracked files are excluded.
  - Binary, symlink, and unreadable untracked files render as placeholders.
- [ ] Add frontend tests.
  - Default picker state is `Committed`.
  - Picker changes update the URL query string.
  - `useRunFiles` calls the API with the selected scope.
  - Each scope has a distinct cache key.
  - The picker is visible when `source === "sandbox"`.
  - The picker is omitted when `source === "final_patch"`.
  - A response with `source: "final_patch"` does not show `All changes` or `Uncommitted` as selected even if the URL requested one of those scopes.
- [ ] Run verification commands.
  - `cargo build -p fabro-api`
  - `cargo nextest run -p fabro-server run_files`
  - `cd lib/packages/fabro-api-client && bun run generate`
  - `cd apps/fabro-web && bun test`
  - `cd apps/fabro-web && bun run typecheck`

## Assumptions

- Existing behavior should remain the default, so missing `scope` means `committed`.
- `Uncommitted changes` means staged, unstaged, and safe untracked files inside the run sandbox.
- Remote clone-based sandboxes cannot include submitter-side local dirty changes unless those changes were already present in the sandbox.
- Stored `final_patch` is used only as fallback committed/final run diff data when the run-owned sandbox cannot be read.
- A stopped sandbox is not considered unavailable until a synchronous start/resume attempt has failed.
