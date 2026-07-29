# Agent Compaction API Usage Baseline Plan

Date: 2026-05-23

## Summary

Change Fabro's agent compaction trigger from a whole-history `chars / 4`
estimate to a Claude Code-style hot-path estimate: use the latest real
assistant response's stored `usage.total_tokens()` as the baseline, then add
local estimates for turns appended after that response. This avoids token-count
provider API calls while making compaction sensitive to actual
provider-reported context usage, including cache and reasoning tokens.

No provider token-count API calls should be added in this change.

## Key Changes

- Replace the current compaction estimate in `fabro-agent` with a new
  active-context estimator.
- Find the newest assistant turn whose `usage.total_tokens() > 0`.
- Use that `usage.total_tokens()` as the baseline.
- Add local estimates only for turns after that assistant turn.
- If no usable assistant usage exists, fall back to the existing local
  whole-history estimate.
- Reuse a shared per-turn local estimate helper so fallback and post-baseline
  delta counting stay consistent.
- Keep the estimator local and in-process. Do not call
  `llm_client.count_input_tokens()` from `compact_if_needed()`.

## Implementation Details

- Update `lib/crates/fabro-agent/src/compaction.rs`:
  - Add an estimator that returns both token count and method, for example
    `ApiUsagePlusLocalDelta` or `LocalEstimate`.
  - Make `check_context_usage()` use the new estimator and include the method
    in warning `details`.
  - Make `compact_context()` report the same improved estimate in
    `CompactionStarted`.
  - Move `CompactionStarted` emission after the
    `original_turn_count <= preserve_count` no-op check, so a no-op compact
    cannot emit started without completed.
- Update `lib/crates/fabro-agent/src/history.rs`:
  - In `History::compact()`, invalidate preserved assistant usage by replacing
    preserved assistant `usage` with `TokenCounts::default()`.
  - Keep provider parts, response IDs, text, and tool calls unchanged.
  - Rationale: preserved assistant usage reflects the pre-compaction context and
    must not become the next baseline. Billing remains available from emitted
    run events, so mutable runtime history should prefer compaction correctness.
- Leave public run event names and schemas unchanged:
  - `agent.compaction.started`
  - `agent.compaction.completed`
  - Existing warning event remains a warning with richer `details`.

## Test Plan

- Add unit coverage in `lib/crates/fabro-agent/src/compaction.rs`:
  - No assistant usage: estimator matches current local whole-history behavior.
  - Latest assistant usage present: estimator uses `usage.total_tokens()` plus
    only later tool/user/steering turns.
  - Usage fields include cache and reasoning through `TokenCounts::total_tokens()`.
  - Earlier assistant usage is ignored when a later assistant usage exists.
- Add unit coverage in `lib/crates/fabro-agent/src/history.rs`:
  - `History::compact()` preserves assistant content, tool calls, and provider
    parts, but resets preserved assistant usage to default.
  - Existing OpenAI opaque stripping and Anthropic thinking preservation tests
    still pass.
- Add session coverage in `lib/crates/fabro-agent/src/session.rs`:
  - A short assistant response with high `usage.total_tokens()` triggers
    compaction even when text length is small.
  - A compact no-op due to `turns.len() <= preserve_count` does not emit
    `CompactionStarted`.
  - Compaction disabled still prevents compaction even if the API usage
    baseline exceeds threshold.
- Run targeted verification:
  - `cargo nextest run -p fabro-agent compaction`
  - `cargo nextest run -p fabro-agent history`
  - If those pass, run `cargo nextest run -p fabro-agent`.

## Assumptions

- Runtime/session `Message::Assistant.usage` is safe to invalidate after
  compaction because authoritative billing comes from emitted workflow/run
  events, not preserved mutable agent history.
- A zero-token `TokenCounts::default()` should be treated as no usable API
  baseline.
- Provider token-count APIs remain available for future near-threshold
  confirmation, but are intentionally out of scope for this change.
