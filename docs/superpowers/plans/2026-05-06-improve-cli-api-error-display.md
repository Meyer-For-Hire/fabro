# Improve CLI API Error Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve useful server error details in Fabro CLI output without adding a new error taxonomy or sprawling command-specific handling.

**Architecture:** Keep error flow centered on existing `anyhow`, `source()`, `miette`, `TaggedFailure`, `ApiFailure`, `api_failure_for`, `classify_api_error`, and `raw_response_failure_error` abstractions. Preserve response details centrally in `fabro-client`, then add sparse user-action context at command boundaries.

**Tech Stack:** Rust, `anyhow`, `miette`, `progenitor_client`, `httpmock`, `cargo nextest`.

---

## Summary

Improve failed `fabro run` and related API command output by preserving server response details through the existing error chain. The target display for run creation failures is:

```text
x could not create run
  caused by: missing field `dirty` at line 1 column 2834
```

For non-JSON plain-text bodies, the CLI should retain the body instead of collapsing to status only:

```text
x could not create run
  caused by: request failed with status 422 Unprocessable Entity: Failed to deserialize ...
```

## Key Changes

- [x] In `lib/crates/fabro-client/src/error.rs`, keep `map_api_error` synchronous. Do not make it read `UnexpectedResponse` bodies, because that requires async body consumption.
- [x] In async client paths that receive `progenitor_client::Error` after `.await`, use `classify_api_error(err).await` so `UnexpectedResponse` bodies are consumed and preserved.
- [x] Update the token-refresh retry path in `Client::send_api` so retry failures also go through the async classifier instead of `.map_err(map_api_error)`.
- [x] Update optional fetch paths that currently call `map_api_error` after `.await` (`get_run_logs`, `read_run_blob`) to use `classify_api_error(err).await.error`, preserving existing `is_not_found_error` behavior through `ApiFailure`.
- [x] In `lib/crates/fabro-cli/src/commands/run/create.rs`, wrap `client.create_run_from_manifest(built.manifest).await` with `context("could not create run")`.
- [x] Do not add new public error types, new CLI diagnostic enums, run-specific API error branches, or a new version-compatibility framework.
- [x] Do not change server wire behavior in this patch.

## Interface Impact

- No public Rust API additions.
- No OpenAPI/schema changes.
- CLI stderr output changes for failed API calls by showing action context plus the existing source chain.
- Exit codes remain unchanged because `ApiFailure` and existing `ExitClass` tagging stay in place.
- `--json` behavior remains unchanged; do not add new JSON error payloads.

## Test Plan

- [x] Add or adjust `fabro-client` unit tests in `lib/crates/fabro-client/src/error.rs` for structured JSON API errors:
  - `errors[0].detail` remains the displayed message.
  - `errors[0].code` remains discoverable through `api_failure_for`.
  - Existing 401 `ExitClass::AuthRequired` behavior still passes.
- [x] Add a `fabro-client` async test for a plain-text `422` `UnexpectedResponse` through `classify_api_error`:
  - The displayed error includes both the status and response body.
  - `api_failure_for` reports status `422`.
- [x] Add a CLI integration test in `lib/crates/fabro-cli/tests/it/cmd/run.rs` with a mock server returning `422` from `POST /api/v1/runs`:
  - `stderr` includes `could not create run`.
  - `stderr` includes the response detail/body, such as `missing field \`dirty\``.
  - `stderr` does not collapse to status-only output.
- [x] Run targeted tests:

```bash
cargo nextest run -p fabro-client
cargo nextest run -p fabro-cli --test it run
```

- [x] Run workspace checks:

```bash
cargo +nightly-2026-04-14 fmt --check --all
cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings
```

## Assumptions

- The first fix should improve error presentation, not add backwards compatibility for the `git.dirty` manifest change.
- Version-skew-specific hints are out of scope unless they can reuse already-available metadata without extra probing or new error plumbing.
- Existing `miette` cause-chain rendering is the display mechanism; implementation should make the error chain better, not bypass it.
