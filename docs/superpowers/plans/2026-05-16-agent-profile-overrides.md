# Agent Profile Overrides Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add provider-level and model-level `agent_profile` overrides for Fabro LLM catalog routing.

**Architecture:** Keep adapter defaults as the baseline, but resolve agent profile as catalog data with model override taking precedence over provider override, which takes precedence over adapter metadata. Route all existing `profile_kind` decisions through the resolved catalog profile so API sessions, project memory discovery, CLI backend command selection, and ACP launch behavior stay consistent.

**Tech Stack:** Rust, serde, strum, Fabro settings/catalog layers, Fabro workflow routing, cargo nextest.

---

## Summary

Add `agent_profile` as trusted LLM catalog settings at both provider and model level. Effective precedence will be:

```text
model agent_profile > provider agent_profile > adapter default_profile
```

Allowed values are exactly `anthropic`, `openai`, and `gemini`. This override affects all current profile-based behavior, including native API sessions, project memory discovery, CLI backend command selection, and ACP launch behavior.

## Key Changes

- Add typed `agent_profile` fields to `[llm.providers.<id>]` and `[llm.models.<id>]` settings, flowing through `fabro-config` builders into `fabro-model`.
- Update `AgentProfileKind` to be a settings-compatible string enum using existing project style: serde + strum, with `openai` as the canonical spelling.
- Resolve catalog data so `CatalogProvider` stores an effective provider `agent_profile`, and `CatalogModelSettings` stores an effective model `agent_profile`.
- Add a catalog helper such as `effective_agent_profile(provider_id, model_id_or_alias)` that:
  - canonicalizes provider aliases;
  - applies the model profile only when the resolved model belongs to the effective provider;
  - otherwise falls back to the provider profile.
- Replace direct `provider.adapter.metadata().default_profile` lookups in workflow routing, run startup, prompt memory discovery, and standalone agent profile selection with the resolved catalog profile.
- Update user configuration docs to show both provider-level and model-level `agent_profile` examples.

## Test Plan

- `fabro-model`:
  - enum string round-trips for `anthropic`, `openai`, `gemini`;
  - provider override wins over adapter default;
  - model override wins over provider override;
  - omitted settings preserve current adapter defaults;
  - explicit provider plus unrelated known model does not leak that model's profile.
- `fabro-config`:
  - TOML parses `agent_profile` at provider and model level;
  - settings layering preserves higher-precedence scalar overrides;
  - runtime settings builder passes both fields into catalog settings.
- `fabro-workflow` / `fabro-agent`:
  - routing returns model-level profile when applicable;
  - run startup stores the effective profile;
  - prompt project-memory discovery uses the effective profile;
  - standalone agent construction uses model override when `--model` points at a catalog model.
- Run:
  - `cargo nextest run -p fabro-model -p fabro-config -p fabro-workflow -p fabro-agent`
  - `cargo +nightly-2026-04-14 fmt --check --all`

## Assumptions

- The field name is only `agent_profile`; no `profile` alias in v1.
- This is catalog/config behavior only; no OpenAPI or model-list response shape changes.
- Built-in provider TOML does not need explicit `agent_profile` entries unless a built-in model intentionally deviates from its adapter default later.
