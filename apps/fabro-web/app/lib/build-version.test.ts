import { afterEach, describe, expect, test } from "bun:test";

import { fetchBuildId, isStaleBuild } from "./build-version";
import { parseBuildId } from "./build-version-contract";

describe("isStaleBuild", () => {
  test("reports stale only when both ids are known and differ", () => {
    expect(isStaleBuild("aaaaaaaa", "bbbbbbbb")).toBe(true);
    expect(isStaleBuild("aaaaaaaa", "aaaaaaaa")).toBe(false);
  });

  // A false "new version" claim is worse than a missed one: it trains people to
  // ignore the toast. Anything unknown must stay silent.
  test("stays silent when either side is unknown", () => {
    expect(isStaleBuild(null, "bbbbbbbb")).toBe(false);
    expect(isStaleBuild("aaaaaaaa", null)).toBe(false);
    expect(isStaleBuild(null, null)).toBe(false);
    expect(isStaleBuild("", "bbbbbbbb")).toBe(false);
  });
});

describe("fetchBuildId", () => {
  const realFetch = globalThis.fetch;
  afterEach(() => {
    globalThis.fetch = realFetch;
  });

  function stubFetch(response: Response) {
    globalThis.fetch = async () => response;
  }

  test("returns the published build id", async () => {
    stubFetch(Response.json({ buildId: "8f2yqj8q" }));
    expect(await fetchBuildId("/build-id.json")).toBe("8f2yqj8q");
  });

  test("returns null for a non-ok response", async () => {
    stubFetch(new Response(null, { status: 503 }));
    expect(await fetchBuildId("/build-id.json")).toBeNull();
  });

  // A server that returns something unexpected must not be read as "a new
  // build shipped" — that would fire the toast on every poll.
  test("returns null for a malformed body", async () => {
    stubFetch(Response.json({ buildId: 42 }));
    expect(await fetchBuildId("/build-id.json")).toBeNull();

    stubFetch(Response.json({}));
    expect(await fetchBuildId("/build-id.json")).toBeNull();

    stubFetch(Response.json(null));
    expect(await fetchBuildId("/build-id.json")).toBeNull();

    stubFetch(Response.json({ buildId: "" }));
    expect(await fetchBuildId("/build-id.json")).toBeNull();

    stubFetch(Response.json({ buildId: "not-a-build-id" }));
    expect(await fetchBuildId("/build-id.json")).toBeNull();

    stubFetch(new Response("<!doctype html>"));
    expect(await fetchBuildId("/build-id.json")).toBeNull();
  });
});

describe("parseBuildId", () => {
  test("normalizes valid ids and rejects values outside the wire format", () => {
    expect(parseBuildId(" 8f2yqj8q ")).toBe("8f2yqj8q");
    expect(parseBuildId("        ")).toBeNull();
    expect(parseBuildId("8F2YQJ8Q")).toBeNull();
    expect(parseBuildId("abc123")).toBeNull();
    expect(parseBuildId(null)).toBeNull();
  });
});
