CREATE TABLE secrets (
    name TEXT PRIMARY KEY NOT NULL,
    secret_type TEXT NOT NULL,
    value TEXT NOT NULL,
    description TEXT,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (length(name) > 0),
    CHECK (secret_type IN ('token', 'oauth', 'file')),
    CHECK (revision > 0),
    CHECK (length(created_at) > 0),
    CHECK (length(updated_at) > 0),
    CHECK (
        (secret_type IN ('token', 'oauth')
            AND substr(name, 1, 1) GLOB '[A-Za-z_]'
            AND name NOT GLOB '*[^A-Za-z0-9_]*')
        OR
        (secret_type = 'file'
            AND (
                name = 'GITHUB_APP_PRIVATE_KEY'
                OR (
                    substr(name, 1, 1) = '/'
                    AND substr(name, -1, 1) <> '/'
                )
            ))
    )
);
