# Live Events Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a live-only `/events` page that streams server-wide Fabro run events received after the page is opened.

**Architecture:** Reuse the existing web SSE coordination layer. One browser tab owns the global `/api/v1/attach` EventSource and rebroadcasts events through `BroadcastChannel`; the new page subscribes to that stream and stores only an in-memory ring buffer for the current page lifetime.

**Tech Stack:** React, React Router, SWR mutate plumbing, EventSource, BroadcastChannel, Bun tests.

---

## Summary

Build a live-only `/events` page that shows server-wide Fabro run events received after the page is opened. Do not add backend replay, persistence, or a server-side recent-events buffer. Reuse the existing web SSE coordination: one tab owns `/api/v1/attach`, other tabs receive events via `BroadcastChannel`.

## Key Changes

- Add a top-level route `events` in `apps/fabro-web/app/router.tsx`, rendered inside the existing app shell with `wide` and `fullHeight` handles.
- Add a top-level nav item named `Events` in `apps/fabro-web/app/layouts/app-shell.tsx` using an existing outline icon such as `BoltIcon`.
- Add `apps/fabro-web/app/lib/live-events.ts`:
  - Export `subscribeToLiveEvents(onEvent, options?)`.
  - Use `subscribeToCrossTabSse` with `subscriptionKey: "live-events"`.
  - In coordinated mode, rely on the existing leader-owned `/api/v1/attach` stream.
  - In fallback mode, use `subscribeToSharedEventSource` with `queryKeys.system.attachUrl()`.
  - Return no SWR invalidation keys; call `onEvent(payload)` from the resolver.
- Add `apps/fabro-web/app/routes/live-events.tsx`:
  - Keep a local in-memory ring buffer, newest first, max 1,000 events.
  - Deduplicate by `id` when present, otherwise by `run_id:seq`.
  - Start empty on mount and reset on refresh/navigation remount.
  - Reuse existing event debug controls: category filter, search input, details panel.
  - Render a global row shape that includes category, event name, `run_id`, optional `node_id`/`stage_id`, and absolute timestamp.
  - Link `run_id` to `/runs/:id` when present.
  - Empty state text should make live-only behavior clear: events appear only after this page is opened.

## Testing

- Add `apps/fabro-web/app/lib/live-events.test.tsx`:
  - Coordinated mode opens `/api/v1/attach`.
  - Fallback mode opens `/api/v1/attach`, not `/api/v1/runs/:id/attach`.
  - `onEvent` receives all run IDs.
  - No SWR mutate keys are emitted.
- Add `apps/fabro-web/app/routes/live-events.test.tsx`:
  - Renders empty live-only state initially.
  - Appends incoming events newest first.
  - Deduplicates repeated event IDs.
  - Caps retained events at 1,000.
  - Filters by category/search and opens details panel.
- Run:
  - `cd apps/fabro-web && bun test`
  - `cd apps/fabro-web && bun run typecheck`

## Assumptions

- No Rust/server changes are needed because `GET /api/v1/attach` already streams global live events.
- “Live-only” means no replay on connect and no persistence in the browser beyond the current page lifetime.
- The page should be discoverable from main navigation as `Events`; the direct URL `/events` remains the canonical route.
