import { describe, expect, test } from "bun:test";

import { queryKeys } from "./query-keys";

describe("queryKeys", () => {
  test("uses semantic tuples as stable SWR keys and keeps SSE URLs explicit", () => {
    expect(queryKeys.auth.me()).toEqual(["auth", "me"]);
    expect(queryKeys.runs.files("run 1")).toEqual([
      "runs",
      "files",
      "run 1",
      "scope",
      "committed",
    ]);
    expect(queryKeys.runs.files("run 1", { kind: "scope", scope: "all" })).toEqual([
      "runs",
      "files",
      "run 1",
      "scope",
      "all",
    ]);
    expect(
      queryKeys.runs.files("run 1", {
        kind:    "commit",
        fromSha: "abc1234",
        toSha:   "def5678",
      }),
    ).toEqual(["runs", "files", "run 1", "commit", "abc1234", "def5678"]);
    expect(queryKeys.runs.commits("run 1")).toEqual(["runs", "commits", "run 1"]);
    expect(queryKeys.runs.graph("run-1", "TB")).toEqual(["runs", "graph", "run-1", "TB"]);
    expect(queryKeys.runs.stageLog("run 1", "build step@2", 12, 34)).toEqual([
      "runs",
      "stage-log",
      "run 1",
      "build step@2",
      12,
      34,
    ]);
    expect(queryKeys.runs.stageEvents("run 1", "build step")).toEqual([
      "runs",
      "stage-events",
      "run 1",
      "build step",
    ]);
    expect(queryKeys.runs.stageContextWindow("run 1", "build step@2")).toEqual([
      "runs",
      "stage-context-window",
      "run 1",
      "build step@2",
    ]);
    expect(queryKeys.runs.sandbox("run 1")).toEqual(["runs", "sandbox", "run 1"]);
    expect(queryKeys.system.integrations()).toEqual(["system", "integrations"]);
    expect(queryKeys.mcpServers.list()).toEqual(["mcp-servers", "list"]);
    expect(queryKeys.mcpServers.detail("github")).toEqual([
      "mcp-servers",
      "detail",
      "github",
    ]);
    expect(queryKeys.system.attachUrl()).toBe("/api/v1/attach");
    expect(queryKeys.runs.attachUrl("run 1")).toBe("/api/v1/runs/run%201/attach");
  });

});
