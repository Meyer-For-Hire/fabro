# Automations SQLite schema design

## Outcome

Use two tables: `automations` owns the aggregate, public revision, and API enablement; `automation_triggers` stores schedule triggers keyed by automation and trigger ID. Keep scheduler cursors and executions out of this migration.

## Proposed migration

`lib/crates/fabro-db/migrations/2026071102_automations.sql` after the secrets migration.

```sql
CREATE TABLE automations (
    id TEXT PRIMARY KEY NOT NULL,
    revision TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    api_enabled INTEGER NOT NULL,
    target_repository TEXT NOT NULL,
    target_ref TEXT NOT NULL,
    target_workflow TEXT NOT NULL,
    CHECK (length(id) BETWEEN 1 AND 63),
    CHECK (substr(id, 1, 1) GLOB '[a-z0-9]'),
    CHECK (id NOT GLOB '*[^a-z0-9-]*'),
    CHECK (length(revision) = 64),
    CHECK (revision NOT GLOB '*[^0-9a-f]*'),
    CHECK (length(trim(name)) > 0),
    CHECK (api_enabled IN (0, 1)),
    CHECK (length(target_repository) BETWEEN 3 AND 140),
    CHECK (length(target_ref) BETWEEN 1 AND 255),
    CHECK (length(target_workflow) BETWEEN 1 AND 255)
);

CREATE TABLE automation_triggers (
    automation_id TEXT NOT NULL,
    id TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    expression TEXT NOT NULL,
    PRIMARY KEY (automation_id, id),
    FOREIGN KEY (automation_id) REFERENCES automations(id) ON DELETE CASCADE,
    CHECK (length(id) BETWEEN 1 AND 63),
    CHECK (substr(id, 1, 1) GLOB '[a-z0-9]'),
    CHECK (id NOT GLOB '*[^a-z0-9_-]*'),
    CHECK (enabled IN (0, 1)),
    CHECK (length(trim(expression)) > 0)
);
```

## Decisions

- Trigger order is non-semantic. Load schedule triggers by ID and hash that canonical order so equivalent definitions share a revision.
- Trigger IDs are unique only within an automation, matching current validation.
- `api_enabled` on `automations` models the manual/API capability, so the trigger table needs no API-row special case or partial unique index.
- `automation_triggers` stores schedule configuration only. Rust validates the five-field UTC cron grammar; SQLite enforces required schedule fields.
- `revision` remains the 64-character SHA-256 ETag for the complete aggregate, including API enablement and schedules in canonical trigger-ID order. Trigger rows do not have independent revisions.
- No timestamps: the current model and API expose none, and revisions already own concurrency.
- No JSON blob: typed columns give useful constraints and queries. A future trigger kind should get an explicit schema migration and the most natural relational shape for its cardinality.
- No schedule index yet. The scheduler reads all definitions and evaluates cron in Rust; the primary key `(automation_id, id)` supports deterministic child loading.

## Store behavior

- Make `AutomationStore` pool-backed with no process-wide cache. `list` and `get` become async and fallible so database failures never look like empty state.
- Load an automation and its schedules with one join ordered by trigger ID, then re-run the existing Rust validation when constructing domain values.
- Create the parent and schedule rows in one transaction.
- Replace validates and computes the aggregate revision first, conditionally updates the parent with `WHERE id = ? AND revision = ?`, then replaces all children in the same transaction.
- Delete uses `WHERE id = ? AND revision = ?`; child deletion cascades.
- On a zero-row replace/delete, query the current revision inside the transaction to distinguish `NotFound` from `StaleRevision`.
- Canonicalize API enablement and schedules sorted by trigger ID before computing the SHA-256 revision. During import, persist the raw-file revision so existing ETags do not change merely because storage moved.

## Legacy import

- Read every `automations/*.toml` beside the active `settings.toml`, ignoring non-TOML files as today.
- Parse and validate the full directory before opening the write transaction.
- Insert each aggregate transactionally. `ON CONFLICT(id) DO NOTHING`; an existing SQL automation wins as a whole, including its triggers.
- After commit, rename the directory to `automations.imported-<timestamp>.bak`.
- Missing directory is a no-op. Invalid input leaves the directory untouched. A rename failure is retry-safe because the next run skips already imported IDs and retries the rename.

## Scheduler boundary

This schema stores definitions only. `next_due_at`, `last_fired_at`, leases, and execution claims remain out of scope because the current scheduler intentionally keeps cursors in memory and skips missed occurrences across restarts. Multi-node exactly-once scheduling would require a separately designed durable claim/execution table, not extra mutable fields on definitions.

## Tests

- Schema constraints: IDs, revisions, booleans, API enablement, schedule fields, foreign key cascade, and duplicate schedule-trigger IDs.
- Store: sorted list, deterministic schedule order, empty schedule list, API enablement, create conflict, conditional replace/delete, two-pool visibility, and row revalidation.
- Atomicity: failed child insert leaves the old aggregate unchanged.
- Import: absent, success, SQL wins, invalid directory unchanged, backup rename, retry after destination commit.
- API/scheduler regression: ETag behavior unchanged; scheduler sees SQL changes and does not treat read failures as an empty list.

## Unresolved questions

- None for definition storage. Durable/multi-node scheduling remains a separate design.
