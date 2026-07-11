CREATE TABLE runs (
    id TEXT PRIMARY KEY NOT NULL,
    source_last_seq INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL,
    started_at_ms INTEGER,
    last_event_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    status TEXT NOT NULL,
    archived_at_ms INTEGER,
    parent_id TEXT,
    title TEXT NOT NULL,
    workflow_slug TEXT,
    workflow_name TEXT,
    repository_name TEXT,
    automation_id TEXT,
    diff_files_changed INTEGER NOT NULL DEFAULT 0,
    diff_additions INTEGER NOT NULL DEFAULT 0,
    diff_deletions INTEGER NOT NULL DEFAULT 0,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    total_usd_micros INTEGER,
    summary_json TEXT NOT NULL,
    CHECK (source_last_seq >= 1),
    CHECK (status IN (
        'submitted',
        'pending',
        'runnable',
        'starting',
        'running',
        'blocked',
        'paused',
        'removing',
        'succeeded',
        'failed',
        'dead'
    )),
    CHECK (diff_files_changed >= 0),
    CHECK (diff_additions >= 0),
    CHECK (diff_deletions >= 0),
    CHECK (input_tokens >= 0),
    CHECK (output_tokens >= 0),
    CHECK (reasoning_tokens >= 0),
    CHECK (cache_read_tokens >= 0),
    CHECK (cache_write_tokens >= 0),
    CHECK (total_usd_micros IS NULL OR total_usd_micros >= 0),
    CHECK (json_valid(summary_json))
);

CREATE INDEX runs_by_created_at ON runs(created_at_ms DESC, id DESC);
CREATE INDEX runs_by_updated_at ON runs(last_event_at_ms DESC, id DESC);
CREATE INDEX runs_by_status ON runs(archived_at_ms, status, last_event_at_ms DESC, id DESC);
CREATE INDEX runs_by_parent ON runs(parent_id, created_at_ms DESC, id DESC);
CREATE INDEX runs_by_workflow ON runs(workflow_slug, created_at_ms DESC, id DESC);
CREATE INDEX runs_by_repository ON runs(repository_name, created_at_ms DESC, id DESC);
CREATE INDEX runs_by_automation ON runs(automation_id, created_at_ms DESC, id DESC);
