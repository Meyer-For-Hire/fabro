# Unified Auth Sessions Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fill `/profile/sessions` with a unified sessions view backed by the new auth sessions API.

**Architecture:** The frontend will consume the normalized `GET /api/v1/auth/sessions` response and render all session kinds in one list. The page follows the existing Profile UI patterns and does not encode backend storage details beyond the `kind`, `current`, and `revocable` fields.

**Tech Stack:** React 19, React Router, SWR, generated TypeScript Axios client, Tailwind CSS, Bun tests.

**Backend dependency:** This plan depends on [2026-05-10-auth-sessions-backend.md](2026-05-10-auth-sessions-backend.md). Implement the backend plan and regenerate the TypeScript API client before starting this plan.

---

## Contract Consumed

Use the generated client for:

- `GET /api/v1/auth/sessions`
- `DELETE /api/v1/auth/sessions/{id}`

Expected response shape:

```ts
type AuthSessionsResponse = {
  sessions: AuthSession[];
};

type AuthSession = {
  id: string;
  kind: "browser" | "cli";
  current: boolean;
  provider: string;
  login: string;
  label: string;
  userAgent?: string | null;
  createdAt: string;
  lastSeenAt: string;
  expiresAt: string;
  revocable: boolean;
};
```

## Tasks

### Task 1: Regenerate The TypeScript API Client

**Files:**
- Generated: `lib/packages/fabro-api-client/src/api/auth-api.ts`
- Generated: `lib/packages/fabro-api-client/src/models/*`

- [ ] Confirm the backend OpenAPI changes from the backend plan are present.
- [ ] Run:

```bash
cd lib/packages/fabro-api-client && bun run generate
```

Expected: generated client includes auth sessions list and delete methods plus `AuthSession` / `AuthSessionsResponse` models.

### Task 2: Add Query Keys And Data Hook

**Files:**
- Modify: `apps/fabro-web/app/lib/query-keys.ts`
- Modify: `apps/fabro-web/app/lib/queries.ts`

- [ ] Add `queryKeys.auth.sessions()`.
- [ ] Add `useAuthSessions()` that calls the generated auth API method and returns `AuthSessionsResponse`.
- [ ] Keep the hook behavior consistent with `useAuthMe()`: authenticated, cookie-backed, SWR-managed.

### Task 3: Build The Sessions Page

**Files:**
- Modify: `apps/fabro-web/app/routes/profile-sessions.tsx`

- [ ] Replace the current empty component.
- [ ] Use existing `Panel`, `PanelSkeleton`, `Badge`, `Muted`, and `Mono` patterns from `apps/fabro-web/app/components/settings-panel.tsx`.
- [ ] Render skeletons while `useAuthSessions()` is loading.
- [ ] Sort sessions with `current === true` first, then descending `lastSeenAt`.
- [ ] Render one row per session with:
  - session label
  - kind badge (`browser` or `cli`)
  - provider
  - login
  - last active timestamp
  - expiry timestamp
  - user agent when present
- [ ] Show no action for `revocable === false`.
- [ ] Show a compact revoke button for `revocable === true`.

### Task 4: Wire Revocation

**Files:**
- Modify: `apps/fabro-web/app/routes/profile-sessions.tsx`

- [ ] On revoke click, call the generated delete sessions API method with the session ID.
- [ ] Disable the button while that session is being revoked.
- [ ] After successful revocation, revalidate `queryKeys.auth.sessions()`.
- [ ] If revocation fails, keep the session visible and render a small inline error near the list.

### Task 5: Add Frontend Tests

**Files:**
- Add: `apps/fabro-web/app/routes/profile-sessions.test.tsx`

- [ ] Test loading state renders a profile-style skeleton.
- [ ] Test browser and CLI sessions render from a mocked unified response.
- [ ] Test non-revocable browser sessions do not show a revoke button.
- [ ] Test revocable CLI sessions show a revoke button.
- [ ] Test clicking revoke calls the delete endpoint and refreshes the sessions query.
- [ ] Run:

```bash
cd apps/fabro-web && bun test
```

Expected: `PASS`.

### Task 6: Typecheck And Visual Smoke

**Files:**
- Modify only if tests/typecheck reveal a real issue in the files above.

- [ ] Run:

```bash
cd apps/fabro-web && bun run typecheck
```

Expected: `PASS`.

- [ ] Start the app stack using the repository's normal dev server flow and visit `/profile/sessions`.
- [ ] Confirm the page shows the current browser session and any authenticated CLI sessions returned by the backend.
- [ ] Confirm the layout matches the existing Profile section and remains usable at narrow widths.

## Final Validation

Run:

```bash
cd lib/packages/fabro-api-client && bun run generate
cd apps/fabro-web && bun test
cd apps/fabro-web && bun run typecheck
```

Expected: all commands pass.

## Assumptions

- The generated API client exposes methods for listing and deleting auth sessions after the backend OpenAPI update.
- The frontend does not invent local session kinds; it renders the normalized backend response.
- Revocation is action-gated entirely by the backend-provided `revocable` field.
