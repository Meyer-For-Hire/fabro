import { describe, expect, test } from "bun:test";

import { fileCacheKey } from "./cache-keys";

describe("fileCacheKey", () => {
  test("identical inputs produce the same key", () => {
    const first = fileCacheKey({
      runId:    "run_1",
      toSha:    "abc1234",
      side:     "old",
      path:     "src/app.ts",
      contents: "export const value = 1;\n",
    });
    const second = fileCacheKey({
      runId:    "run_1",
      toSha:    "abc1234",
      side:     "old",
      path:     "src/app.ts",
      contents: "export const value = 1;\n",
    });

    expect(second).toBe(first);
  });

  test("changing file contents changes the key", () => {
    const base = {
      runId: "run_1",
      toSha: "abc1234",
      side:  "new" as const,
      path:  "src/app.ts",
    };

    expect(fileCacheKey({ ...base, contents: "one" })).not.toBe(
      fileCacheKey({ ...base, contents: "two" }),
    );
  });

  test("changing file name or side changes the key", () => {
    const base = {
      runId:    "run_1",
      toSha:    "abc1234",
      contents: "same",
    };

    expect(fileCacheKey({ ...base, side: "old", path: "src/a.ts" })).not.toBe(
      fileCacheKey({ ...base, side: "old", path: "src/b.ts" }),
    );
    expect(fileCacheKey({ ...base, side: "old", path: "src/a.ts" })).not.toBe(
      fileCacheKey({ ...base, side: "new", path: "src/a.ts" }),
    );
  });

  test("missing toSha still produces a deterministic key", () => {
    expect(
      fileCacheKey({
        runId:    "run_1",
        toSha:    null,
        side:     "new",
        path:     "src/app.ts",
        contents: "same",
      }),
    ).toMatch(/^fabro-run-file:run_1:no-sha:new:src\/app\.ts:[0-9a-f]{8}$/);
  });
});
