# MCP Servers to SQLite

## Goal

Move server-managed MCP definitions from sibling `mcps/*.toml` files into the
shared SQLite database. Preserve the REST contract, value-omitting read views,
content-hash ETags, synchronous manifest catalog reads, and all three transport
types.

## Schema

Use one `mcp_servers` row per definition:

- Scalar columns: id, revision, display name, description, transport type,
  protocol, URL, port, and timeouts.
- Typed JSON columns: command array, env map, and header map.
- A transport-shape constraint requires exactly the columns belonging to
  `stdio`, `http`, or `sandbox`.
- ID, revision, protocol, port, timeout, and top-level JSON shape constraints
  provide database-level defense in depth.
- No secondary indexes: supported queries are primary-key lookup and full
  sorted listing.

Keep `McpServerRevision` as lowercase SHA-256 hex. Create and replace derive it
from the existing canonical representation. Legacy import preserves the hash
of the original TOML bytes so an ETag remains valid across upgrade.

## Store

- `fabro-mcp-store` owns SQL, row mapping, typed JSON encoding, validation,
  revisions, caching, and legacy import.
- Retain the synchronous in-memory catalog required by manifest resolution.
- Serialize in-process mutations, then enforce replace/delete revisions in SQL
  with `WHERE id = ? AND revision = ?`.
- Update the cache only after a successful transaction.
- Revalidate every decoded row through the existing domain model.
- Encode env/header maps through sorted maps for deterministic JSON.
- Never log or debug-print transport env/header values.

## Legacy import

At startup, inspect `mcps/` next to the active `settings.toml`:

1. Missing directory: no-op.
2. Parse and validate every TOML definition before mutating SQLite.
3. Insert all definitions in one transaction with
   `ON CONFLICT(id) DO NOTHING`; SQLite wins.
4. Commit, then rename the directory to
   `mcps.imported-<timestamp>.bak`.
5. If backup rename fails, leave the source directory for a retry; the next
   import skips existing SQLite rows and retries the rename.

Logs contain only paths, counts, and MCP ids. They never contain commands,
URLs, env values, or headers.

## Tests

- Schema accepts every transport and rejects invalid variant shapes.
- CRUD, sorted listing, reload persistence, and all transport round trips.
- Independent store instances enforce SQL revision conflicts.
- Deterministic map JSON and typed corrupted-row errors.
- Import success, SQLite precedence, stable imported revision, retry no-op,
  malformed input unchanged, and directory backup.
- API persistence/restart behavior, value-omitting reads, legacy startup import,
  malformed legacy startup failure, and existing auth requirements.
- Workspace build, formatter, Clippy, and relevant Nextest suites.

## Unresolved questions

None.
