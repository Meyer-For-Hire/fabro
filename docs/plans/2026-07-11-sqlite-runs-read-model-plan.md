# SQLite Runs Read Model Plan

## Goal

Add SQLite `runs` projection. Power run summary reads, board, table. SlateDB events remain authoritative.

## Decisions

- `runs` rebuildable from events; never directly mutated.
- Store query columns + canonical base `Run` JSON.
- Store five token buckets + `total_usd_micros`; derive `total_tokens`, `RunSize`.
- Dynamic fields at read: children count, live wall time, Ask Fabro readiness, queue position.
- Preserve API wire contract. No event schema changes.

## 1. Schema

- Add `fabro-db/migrations/2026071101_runs.sql`.
- Columns: `id`, `source_last_seq`, timestamps, status, archive, parent, title, workflow, repo, automation, diff totals, token buckets, cost, `summary_json`.
- Index: created, updated, status/archive, parent, workflow, repo, automation.
- Constraints: JSON valid; booleans/counts valid; known status strings.
- Migration tests: table, indexes, constraints.

## 2. Concrete store

- Add `fabro-store::RunSummaryStore` over `SqlitePool`; no trait/backend enum.
- Methods: upsert projected row, get, list/count, children count, delete.
- `source_last_seq` monotonic; duplicate/older projection no-op.
- One transaction writes columns + `summary_json`.
- Row decode validates `Run`; parity-check indexed columns against JSON in tests.

## 3. Projection + reconciliation

- Convert existing `CachedRunProjection` to SQL row via existing `build_summary`.
- After durable SlateDB append and in-memory projection update, synchronously upsert SQLite.
- Backfill/reconcile at startup from warmed SlateDB projections.
- Idempotent restart; newer SQLite watermark never overwritten.
- Projection failure observable with run id/seq; no payload logging.
- Delete SQLite row after authoritative SlateDB run deletion succeeds.

## 4. Shadow verification

- Keep current reads.
- Compare SQLite vs current cache for list/get in integration tests.
- Cover create, lifecycle, title, parent, archive, retry, billing, diff, delete, restart/backfill.
- Cover failed/interrupted reconciliation and resume.

## 5. Read cutover

- `GET /runs`: SQLite filtering, sorting, count, pagination.
- `GET /runs/{id}` and resolve: SQLite summary.
- Automation/parent-child summary lists: SQLite.
- Apply dynamic decorations after row decode.
- Keep `/state`, stages, detailed billing, settings, questions, events on full projection/event store.
- Remove list-path dependency on global projection-cache scan; retain detailed projection cache.

## 6. Verification

- `cargo nextest run -p fabro-db`
- `cargo nextest run -p fabro-store`
- `cargo nextest run -p fabro-server`
- `cargo build --workspace`
- Pinned fmt + Clippy.
- Representative multi-run append/list benchmark; record p50/p95, DB size.

## Unresolved questions

- Keep `RunSize` cost-derived (recommended), or redefine from total tokens?
- Cut over reads in same release after parity tests (recommended), or shadow for one release?
