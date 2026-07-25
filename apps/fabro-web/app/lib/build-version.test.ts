import { afterEach, describe, expect, test } from "bun:test";

import { fetchBuildId, isStaleBuild } from "./build-version";

describe("isStaleBuild", () => {
  test("reports stale only when both ids are known and differ", () => {
    expect(isStaleBuild("abc", "def")).toBe(true);
    expect(isStaleBuild("abc", "abc")).toBe(false);
  });

  // A false "new version" claim is worse than a missed one: it trains people to
  // ignore the toast. Anything unknown must stay silent.
  test("stays silent when either side is unknown", () => {
    expect(isStaleBuild(null, "def")).toBe(false);
    expect(isStaleBuild("abc", null)).toBe(false);
    expect(isStaleBuild(null, null)).toBe(false);
    expect(isStaleBuild("", "def")).toBe(false);
  });
});

describe("fetchBuildId", () => {
  const realFetch = globalThis.fetch;
  afterEach(() => {
    globalThis.fetch = realFetch;
  });

  function stubFetch(response: { ok: boolean; body?: unknown }) {
    globalThis.fetch = (async () => ({
      ok:   response.ok,
      json: async () => response.body,
    })) as unknown as typeof fetch;
  }

  test("returns the published build id", async () => {
    stubFetch({ ok: true, body: { buildId: "8f2yqj8q" } });
    expect(await fetchBuildId("/build-id.json")).toBe("8f2yqj8q");
  });

  test("returns null for a non-ok response", async () => {
    stubFetch({ ok: false });
    expect(await fetchBuildId("/build-id.json")).toBeNull();
  });

  // A server that returns something unexpected must not be read as "a new
  // build shipped" — that would fire the toast on every poll.
  test("returns null for a malformed body", async () => {
    stubFetch({ ok: true, body: { buildId: 42 } });
    expect(await fetchBuildId("/build-id.json")).toBeNull();

    stubFetch({ ok: true, body: {} });
    expect(await fetchBuildId("/build-id.json")).toBeNull();

    stubFetch({ ok: true, body: null });
    expect(await fetchBuildId("/build-id.json")).toBeNull();

    stubFetch({ ok: true, body: { buildId: "" } });
    expect(await fetchBuildId("/build-id.json")).toBeNull();
  });
});
