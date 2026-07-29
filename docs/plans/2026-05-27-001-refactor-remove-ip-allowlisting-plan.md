---
title: "refactor: Remove IP allowlisting"
type: refactor
status: active
date: 2026-05-27
---

# refactor: Remove IP allowlisting

## Overview

Remove Fabro's inbound server IP allowlisting feature completely. This removes the
server-wide `[server.ip_allowlist]` setting, the GitHub webhook-specific
`[server.integrations.github.webhooks.ip_allowlist]` overlay, request middleware,
client-IP extraction, GitHub `/meta` hook-range expansion, settings API exposure,
generated API client types, and the Settings > Security UI display.

Existing configs that still contain removed IP allowlist keys should fail as
unknown fields. This is an intentional hard removal, not a compatibility
deprecation.

## Problem Frame

The feature is being removed from Fabro rather than maintained as an in-process
network access-control layer. Network source restrictions should be handled
outside Fabro by reverse proxies, firewalls, VPNs, Tailscale, platform ingress,
or other deployment-layer controls.

## Requirements Trace

- R1. Remove all runtime enforcement of inbound source-IP allowlisting from web,
  API, static asset, and GitHub webhook routes.
- R2. Remove the server settings schema for global and GitHub webhook IP
  allowlists.
- R3. Remove `IpAllowEntry` and related API schema/client types from public
  settings payloads.
- R4. Keep unrelated allowlists intact: GitHub username allowlists, sandbox
  egress CIDR allow lists, and internal policy/test allowlists.
- R5. Preserve existing authentication, GitHub webhook HMAC verification,
  routing, health checks, request logging, and settings hot-reload behavior.
- R6. Treat old IP allowlist config as invalid after removal.
- R7. Update operator-facing docs/changelog so users know to move source-IP
  restrictions upstream.

## Scope Boundaries

- Do not remove `[server.auth.github].allowed_usernames`.
- Do not remove Daytona/sandbox `cidr_allow_list` network policy support.
- Do not remove generic uses of "allowlist" in policy tests, markdown rendering,
  model controls, or other unrelated domains.
- Do not add a migration, warning-only parser, or fallback compatibility path for
  old IP allowlist settings.
- Do not change GitHub webhook signature verification or webhook route strategy.

## Context & Research

### Relevant Code and Patterns

- Settings use sparse config layers in `lib/crates/fabro-config/src/layers/`
  and dense resolved types in `lib/crates/fabro-types/src/settings/`.
- The OpenAPI spec at `docs/public/api-reference/fabro-api.yaml` is the source
  of truth for API wire shape; `cargo build -p fabro-api` regenerates Rust API
  code, and `cd lib/packages/fabro-api-client && bun run generate` regenerates
  the TypeScript client.
- `lib/crates/fabro-server/src/server.rs` builds the router and currently wires
  IP allowlist middleware before other route dispatch behavior.
- `lib/crates/fabro-server/src/serve.rs` currently resolves server and webhook
  allowlist configs at startup and creates a GitHub `/meta` resolver.
- `apps/fabro-web/app/routes/settings-security.tsx` displays the settings API's
  `server.ip_allowlist` state.

### Main Removal Surfaces

- Config/types: `fabro-types`, `fabro-config`, config tests.
- Server runtime: `fabro-server/src/ip_allowlist.rs`, router wiring, startup
  wiring, test helpers, routing/TCP tests.
- API/contracts: OpenAPI schema, `fabro-api` type replacements/exports, generated
  TypeScript API client models.
- Frontend/docs: Settings > Security copy, settings navigation copy, changelog.

## Key Technical Decisions

- **Hard removal:** Removed config keys should fail through existing
  `deny_unknown_fields` behavior. This keeps the change simple and makes stale
  deployment config visible immediately.
- **Delete, do not stub:** Remove `IpAllowlistConfig` and middleware parameters
  instead of passing default empty configs through the router. Empty stubs would
  keep the feature shape alive and make future cleanup harder.
- **Keep webhook auth unchanged:** GitHub webhook source-IP filtering goes away,
  but HMAC signature verification remains the security boundary for webhook
  payload authenticity.
- **Generated clients follow OpenAPI:** Update the OpenAPI schemas first, then
  regenerate Rust and TypeScript API outputs rather than hand-editing generated
  client files except as a short-term cleanup if generation leaves stale exports.
- **Docs mention upstream controls:** The changelog and security guidance should
  direct operators to network-layer controls, not a replacement Fabro setting.

## Implementation Units

- [ ] **Unit 1: Remove config schema and resolved types**

**Goal:** Delete the IP allowlist settings contract from config parsing and dense
server settings.

**Requirements:** R2, R3, R4, R6

**Dependencies:** None

**Files:**
- Modify: `lib/crates/fabro-types/src/settings/server.rs`
- Modify: `lib/crates/fabro-types/src/settings/mod.rs`
- Modify: `lib/crates/fabro-types/Cargo.toml`
- Modify: `lib/crates/fabro-config/src/layers/server.rs`
- Modify: `lib/crates/fabro-config/src/layers/mod.rs`
- Modify: `lib/crates/fabro-config/src/lib.rs`
- Modify: `lib/crates/fabro-config/src/resolve/server.rs`
- Modify: `lib/crates/fabro-config/src/tests/resolve_server.rs`

**Approach:**
- Remove `ServerNamespace.ip_allowlist` and its `test_default()` population.
- Delete `ServerIpAllowlistSettings`, `ServerIpAllowlistOverrideSettings`, and
  `IpAllowEntry`.
- Remove `ServerLayer.ip_allowlist`, `ServerIpAllowlistLayer`,
  `ServerIpAllowlistOverrideLayer`, and `IntegrationWebhooksLayer.ip_allowlist`.
- Remove resolver functions and validation for global and webhook IP allowlists,
  including `github_meta_hooks` parsing and Unix socket trusted-proxy checks.
- Remove `ipnet` from `fabro-types`. Keep `ipnet` in `fabro-config` because
  `resolve/environment.rs` still validates sandbox CIDR policy.
- Replace old positive/negative IP allowlist config tests with unknown-field
  tests for `[server.ip_allowlist]` and
  `[server.integrations.github.webhooks.ip_allowlist]`.

**Patterns to follow:**
- Existing unknown-field tests in `lib/crates/fabro-config/src/tests/resolve_server.rs`
  for retired server settings.
- Existing `ServerLayer`/`ServerNamespace` layer-to-resolved pattern for removing
  a server subdomain cleanly.

**Test scenarios:**
- Error path: TOML with `[server.ip_allowlist]` fails parsing/resolution with an
  unknown-field diagnostic mentioning `ip_allowlist`.
- Error path: TOML with `[server.integrations.github.webhooks.ip_allowlist]`
  fails with an unknown-field diagnostic mentioning `ip_allowlist`.
- Happy path: minimal valid server settings still resolve without any
  `ip_allowlist` field.
- Integration: serialized `ServerSettings` JSON no longer includes
  `server.ip_allowlist`.

**Verification:**
- `fabro-config` and `fabro-types` compile without removed IP allowlist symbols.
- Config tests prove stale allowlist settings are rejected.

- [ ] **Unit 2: Remove server middleware and startup resolution**

**Goal:** Remove all runtime source-IP filtering and GitHub `/meta` range
resolution from `fabro-server`.

**Requirements:** R1, R4, R5

**Dependencies:** Unit 1

**Files:**
- Delete: `lib/crates/fabro-server/src/ip_allowlist.rs`
- Modify: `lib/crates/fabro-server/src/lib.rs`
- Modify: `lib/crates/fabro-server/src/server.rs`
- Modify: `lib/crates/fabro-server/src/serve.rs`
- Modify: `lib/crates/fabro-server/src/test_support.rs`
- Modify: `lib/crates/fabro-server/Cargo.toml`
- Modify: `lib/crates/fabro-server/src/auth/translate.rs`
- Modify: `lib/crates/fabro-server/src/web_auth.rs`
- Modify: `lib/crates/fabro-cli/tests/it/support/auth_harness.rs`

**Approach:**
- Remove the public `ip_allowlist` module export.
- Delete `IpAllowlistConfig`, `IpAllowlist`, `GitHubMetaResolver`, client-IP
  extraction, middleware, and GitHub meta cache helpers.
- Simplify `build_router_with_options` by removing its `Arc<IpAllowlistConfig>`
  parameter and removing `RouterOptions.github_webhook_ip_allowlist`.
- Remove the global allowlist middleware layer from the main app router.
- Simplify `github_webhook_routes` so it only receives the webhook secret and no
  route-specific allowlist config.
- Remove startup creation of `GitHubMetaResolver`, `default_ip_allowlist`,
  `resolve_github_webhook_ip_allowlist`, and
  `resolve_startup_github_webhook_ip_allowlist`.
- Replace all test/helper call sites that pass `IpAllowlistConfig::default()`
  with the simplified router signature.
- Remove `ipnet` and any now-unused HTTP mocking/test-only dependencies from
  `fabro-server` if they are only used by the deleted module.
- Consider whether `serve.rs` still needs `SocketAddr` for TCP
  `ConnectInfo`; if it was only present for allowlisting tests, remove the
  connect-info service wrapper and imports.

**Patterns to follow:**
- Keep router layers ordered as they are after the allowlist layer is removed:
  auth translation, demo routing, auth extension, canonical host, security
  headers, HTTP logging, request ID.
- Existing webhook route HMAC tests and auth tests should remain the source of
  truth for webhook behavior.

**Test scenarios:**
- Happy path: API requests from any TCP remote address route normally when auth
  requirements are satisfied.
- Happy path: `/health` remains accessible.
- Integration: GitHub webhook route still rejects missing/invalid signatures and
  accepts valid signatures exactly as before.
- Cleanup: there are no references to `IpAllowlistConfig`, `ip_allowlist_middleware`,
  `GitHubMetaResolver`, `github_meta_hooks`, or `github-meta-hooks.json`.

**Verification:**
- `fabro-server` and CLI auth harness tests compile against the simplified router
  API.
- Runtime startup no longer performs any GitHub `/meta` fetch for webhook IP
  ranges.

- [ ] **Unit 3: Update OpenAPI and generated API clients**

**Goal:** Remove IP allowlist fields and schemas from public settings API
contracts and regenerated clients.

**Requirements:** R3, R4

**Dependencies:** Units 1 and 2

**Files:**
- Modify: `docs/public/api-reference/fabro-api.yaml`
- Modify: `lib/crates/fabro-api/build.rs`
- Modify: `lib/crates/fabro-api/src/lib.rs`
- Modify: `lib/crates/fabro-api/tests/server_settings_round_trip.rs`
- Modify/generated: `lib/packages/fabro-api-client/src/models/*`

**Approach:**
- Remove `ip_allowlist` from `ServerNamespace.required` and
  `ServerNamespace.properties`.
- Remove `ServerIpAllowlistSettings`,
  `ServerIpAllowlistOverrideSettings`, `IpAllowEntry`,
  `LiteralIpAllowEntry`, and `GitHubMetaHooksEntry` schemas.
- Remove `IntegrationWebhooksSettings.ip_allowlist` from required/properties.
- Remove `with_replacement` entries and public re-exports for removed types in
  `fabro-api`.
- Regenerate Rust API code by building `fabro-api`.
- Regenerate TypeScript Axios client and ensure stale allowlist model files and
  index exports are gone.
- Strengthen round-trip tests to assert settings JSON omits
  `server.ip_allowlist` and webhook `ip_allowlist`.

**Patterns to follow:**
- Existing OpenAPI-first workflow in `AGENTS.md`.
- Existing `server_settings_family_reuses_domain_types` assertions for shared
  domain type identity.

**Test scenarios:**
- Contract: `ServerSettings` round-trips through API types without any IP
  allowlist field.
- Contract: OpenAPI-generated TypeScript `ServerNamespace` has no
  `ip_allowlist` property.
- Contract: `IntegrationWebhooksSettings` has only webhook strategy fields after
  removal.

**Verification:**
- Generated Rust and TypeScript clients match the updated OpenAPI spec.
- `rg "ServerIpAllowlist|IpAllowEntry|server-ip-allowlist|literal-ip-allow-entry"`
  finds no remaining generated/public API references.

- [ ] **Unit 4: Update web settings UI**

**Goal:** Remove IP allowlist display from Settings > Security and align copy
with the new settings shape.

**Requirements:** R3, R7

**Dependencies:** Unit 3

**Files:**
- Modify: `apps/fabro-web/app/routes/settings-security.tsx`
- Modify: `apps/fabro-web/app/routes/settings.tsx`

**Approach:**
- Change the Security page description from "Authentication methods and network
  allowlist" to authentication-only wording.
- Remove destructuring and rendering of `settings.server.ip_allowlist`.
- Remove the unused `Count` and `plural` imports if no longer used.
- Update the Settings nav description for Security from "Authentication and
  network allowlist" to authentication-focused copy.

**Patterns to follow:**
- Existing `settings-panel` row layout for Auth methods and Allowed usernames.

**Test scenarios:**
- Typecheck: the route compiles against the regenerated API client with no
  `server.ip_allowlist` property.
- UI behavior: Security page still renders auth methods and allowed usernames.
- Cleanup: no frontend references to `ip_allowlist`, `IP allowlist`, or
  `trusted_proxy_count` remain.

**Verification:**
- `apps/fabro-web` typecheck passes against the new generated client.

- [ ] **Unit 5: Update tests, docs, changelog, and dependency lockfile**

**Goal:** Remove stale references and document the operator-facing behavior
change.

**Requirements:** R4, R6, R7

**Dependencies:** Units 1 through 4

**Files:**
- Modify: `lib/crates/fabro-server/tests/it/api/routing.rs`
- Modify: `lib/crates/fabro-server/tests/it/api/tcp.rs`
- Modify: `lib/crates/fabro-server/tests/it/api/settings.rs`
- Modify: `docs/public/administration/security.mdx`
- Create or modify: `docs/public/changelog/2026-05-27.mdx`
- Modify: `Cargo.lock` if dependency graph changes

**Approach:**
- Delete route/TCP tests that only prove IP allowlist enforcement or
  `ConnectInfo` behavior.
- Adjust any router setup helpers after the signature simplification.
- Add a settings API assertion that `server.ip_allowlist` is absent from
  `/api/v1/settings`.
- Update security docs to explicitly state that Fabro does not provide inbound
  source-IP allowlisting and operators should use upstream network controls.
- Add a changelog entry dated 2026-05-27 describing the hard removal and the
  expected replacement at the deployment layer.
- Run dependency resolution after removing direct `ipnet`/test dependencies;
  keep transitive or unrelated `ipnet` entries needed by sandbox CIDR validation.

**Patterns to follow:**
- Existing changelog style in `docs/public/changelog/2026-05-26.mdx`.
- Existing settings API integration test style in
  `lib/crates/fabro-server/tests/it/api/settings.rs`.

**Test scenarios:**
- Settings API: response contains server auth, listen, storage, scheduler, and
  integrations fields, but not `server.ip_allowlist`.
- Config compatibility: old IP allowlist TOML fails before startup rather than
  being silently ignored.
- Docs validation: security docs no longer imply Fabro can restrict inbound
  source IPs internally.

**Verification:**
- `rg -n "ip_allowlist|trusted_proxy_count|github_meta_hooks|IP allowlist|ip allowlist"`
  returns only historical archived plan/brainstorm/spec references or unrelated
  non-server allowlist text.

## System-Wide Impact

- **Public API:** `GET /api/v1/settings` response shape changes by removing
  `server.ip_allowlist` and webhook `ip_allowlist`.
- **Config compatibility:** Existing `settings.toml` files containing the removed
  keys become invalid. This is intentional.
- **Runtime security posture:** Fabro no longer blocks requests based on source
  IP. Operators must enforce network source restrictions upstream.
- **Webhook handling:** GitHub webhook HMAC verification remains unchanged; only
  optional source-IP filtering is removed.
- **Generated clients:** Downstream TypeScript/Rust consumers that read
  `server.ip_allowlist` must update.

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Operators accidentally expose a server that previously relied on Fabro IP allowlisting | Changelog and security docs explicitly call out the removal and direct operators to upstream controls. |
| Generated API clients retain stale types | Update OpenAPI first, regenerate both Rust and TypeScript clients, and run search checks for stale symbols. |
| Unrelated allowlist functionality is removed by broad search/replace | Scope searches to `IpAllow`, `ip_allowlist`, `trusted_proxy_count`, and `github_meta_hooks`; preserve sandbox CIDR and GitHub username allowlists. |
| Router signature cleanup breaks many tests | Update shared test helpers first, then compile-driven cleanup of remaining call sites. |
| `ipnet` is removed where still needed | Keep `fabro-config` dependency if environment CIDR validation still imports `ipnet::IpNet`. |

## Test Plan

- `cargo build -p fabro-api`
- `cd lib/packages/fabro-api-client && bun run generate`
- `cargo nextest run -p fabro-config -p fabro-types -p fabro-api -p fabro-server`
- `cd apps/fabro-web && bun run typecheck`
- `cd apps/fabro-web && bun test`
- `cargo +nightly-2026-04-14 fmt --check --all`
- `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`

## Assumptions

- The accepted compatibility policy is hard removal: no warning-only parser, no
  migration, and no custom compatibility error path.
- Public API consumers can tolerate a breaking settings shape change for this
  removed feature.
- Historical documents in `docs/plans/`, `docs/brainstorms/`, and
  `docs/superpowers/` are archival and do not need rewriting.

## Sources & References

- Previous feature requirements:
  `docs/brainstorms/2026-04-15-ip-whitelist-requirements.md`
- Previous implementation plan:
  `docs/plans/2026-04-15-002-feat-ip-allowlist-plan.md`
- API workflow guidance: `AGENTS.md`
- OpenAPI source of truth: `docs/public/api-reference/fabro-api.yaml`
