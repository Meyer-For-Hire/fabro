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
