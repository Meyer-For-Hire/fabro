# Explicit Event-Sourced Run Titles

## Summary

Add an explicit run `title` that is always a non-blank string in API responses. On creation, clients may provide `title`; otherwise the server infers it from the run goal exactly once for new runs. Later title changes are persisted through a new event, not by mutating stored summaries or recomputing from the goal.

## Key Changes

- Add optional `title` to `RunManifest`; validate provided titles by trimming, rejecting blank/whitespace-only values, rejecting titles containing newline/control characters, and rejecting values over 100 chars.
- Add shared title helpers:
  - explicit title normalization: trim, require non-blank, require a single logical line, max 100 chars, reject invalid input.
  - inferred title: current first-line goal cleanup, truncate to 100 chars using the existing `97 chars + "..."` behavior, fallback to `"Untitled run"` if inference is blank.
- Store resolved title on new `run.created` events. Keep legacy event replay compatible by inferring title from goal only when replaying old `run.created` events that lack a title.
- Add `RunProjection.title`; populate it from `run.created`, update it from `run.title.updated`, and make `RunSummary.title` read from projection title instead of deriving from `goal`.
- Change summary construction so `build_summary` passes the projected title into `RunSummary` instead of letting `RunSummary::new` derive it from `goal`.
- Add `run.title.updated` with `{ "title": "..." }`; event creation is trusted and events are expected to already contain normalized, valid titles. PATCH is the public validation boundary.
- Add `PATCH /api/v1/runs/{id}` with body `{ "title": "..." }`; return updated `RunSummary`.
- Allow title PATCH for all run states, including archived runs, as a metadata-only exception to archived read-only behavior. This endpoint deliberately bypasses the existing archived-run mutation guard; all other archived-run mutation guards remain unchanged.
- If PATCH normalizes to the existing title, return the current summary without appending a no-op event.
- Add `title` to `RunStatusResponse` so create/start/lifecycle acknowledgements include the current resolved title.
- No web edit UI in this slice. Do update frontend/SSE invalidation so `run.title.updated` refreshes run detail and both board data keys (`include_archived=false` and `include_archived=true`) when another client changes a title.

## Public Interfaces

- OpenAPI:
  - `RunManifest.title?: string | null` with `maxLength: 100` and description documenting trim, non-blank, and single-line validation.
  - new `UpdateRunRequest` with required `title: string`, `maxLength: 100`, and the same validation description.
  - `PATCH /api/v1/runs/{id}` returns `RunSummary`
  - add required `RunStatusResponse.title: string`; create/start/pause/unpause/archive/unarchive responses include the current projected title.
  - document `RunSummary.title` and `RunListItem.title` as non-blank strings
- Generated clients:
  - rebuild Rust API types and TypeScript API client after OpenAPI changes.
- CLI:
  - no new `fabro run --title` flag in this plan; CLI-created runs continue relying on server-side inference unless a caller builds a manifest with `title`.

## Test Plan

- Unit tests for title normalization: trims, rejects blank, rejects over 100, preserves valid title.
- Unit tests for inference: strips markdown heading and `Plan:`, truncates to 100, returns `"Untitled run"` for blank/empty goals.
- Store/projection tests:
  - new `run.created` with title populates projection and summary title.
  - old `run.created` without title still replays and yields inferred/fallback title.
  - `run.title.updated` changes projection and summary.
  - `RunSummary` construction uses the projected title rather than deriving from `goal`.
- Server API tests:
  - create run with explicit title returns that title in `RunStatusResponse`.
  - create run without title returns inferred title in `RunStatusResponse`.
  - blank/whitespace create title returns 400.
  - create and PATCH reject titles containing newline/control characters with 400.
  - start/pause/unpause/archive/unarchive responses include the current projected title.
  - PATCH updates active, terminal, and archived runs.
  - archived title PATCH succeeds while the existing archived-run mutation guards continue rejecting other write operations.
  - PATCH rejects blank and over-100 titles with 400.
  - same-title PATCH is idempotent and does not append an update event.
- Frontend tests:
  - board and run-detail invalidation include `run.title.updated`.
  - board invalidation refreshes both archived and non-archived board keys for `run.title.updated`.
  - existing rendering continues to display server `title`.

## Assumptions

- Maximum explicit title length is 100 characters.
- Explicit title length and inferred title truncation are counted in Rust chars, matching the current title derivation behavior.
- Explicit titles are rejected when invalid; only inferred titles are truncated.
- Explicit titles must be single-line strings without control characters.
- `"Untitled run"` is the canonical server fallback when inference cannot produce a non-blank title.
- Existing legacy runs may still have their title inferred during replay because old event streams did not record a resolved title.
