import { describe, expect, test } from "bun:test";
import { StageHandler, StageState } from "@qltysh/fabro-api-client";
import type { RunArtifactEntry } from "@qltysh/fabro-api-client";

import type { Stage } from "../../lib/stage-sidebar";
import { groupArtifactsByFile, splitArtifactPath } from "./group";

function stage(nodeId: string, startedAt: string | null, visit = 1): Stage {
  return {
    id: `${nodeId}@${visit}`,
    name: nodeId,
    handler: StageHandler.AGENT,
    nodeId,
    visit,
    graphVisit: null,
    resumedFromStageId: null,
    status: StageState.SUCCEEDED,
    duration: "1s",
    startedAt,
    providerUsed: null,
  };
}

function artifact(
  nodeSlug: string,
  path: string,
  size: number,
  retry = 1,
  visit = 1,
): RunArtifactEntry {
  return {
    stage_id: `${nodeSlug}@${visit}`,
    node_slug: nodeSlug,
    retry,
    relative_path: path,
    size,
  };
}

/** Mirrors run 01KYJ8ZR0N: one report rewritten by four stages. */
const REPORT = ".ai/reports/2026-07-27-wrk-002-instance-lifecycle.md";

const STAGES: Stage[] = [
  stage("start", "2026-07-27T17:12:08Z"),
  stage("plan", "2026-07-27T17:21:44Z"),
  stage("implement_plan", "2026-07-27T17:44:18Z"),
  stage("simplify", "2026-07-27T18:43:11Z"),
  stage("consolidate_reviews", "2026-07-27T19:44:26Z"),
  stage("fix_review_findings", "2026-07-27T19:49:22Z"),
];

describe("splitArtifactPath", () => {
  test("splits a nested path into directory prefix and filename", () => {
    expect(splitArtifactPath(".ai/reports/run.md")).toEqual({
      dir: ".ai/reports/",
      name: "run.md",
    });
  });

  test("leaves a root-level path without a directory", () => {
    expect(splitArtifactPath("README.md")).toEqual({ dir: "", name: "README.md" });
  });
});

describe("groupArtifactsByFile", () => {
  test("collapses repeated captures of one path into a single file", () => {
    const files = groupArtifactsByFile(
      [
        artifact("consolidate_reviews", REPORT, 14323),
        artifact("fix_review_findings", REPORT, 17483),
        artifact("implement_plan", REPORT, 8422),
        artifact("simplify", REPORT, 13162),
      ],
      STAGES,
    );

    expect(files).toHaveLength(1);
    expect(files[0].path).toBe(REPORT);
    expect(files[0].dir).toBe(".ai/reports/");
    expect(files[0].name).toBe("2026-07-27-wrk-002-instance-lifecycle.md");
    expect(files[0].versions).toHaveLength(4);
  });

  test("orders versions newest first using the API stage order", () => {
    const stages = [
      stage("implement_plan", "2026-07-27T20:00:00Z"),
      stage("simplify", "2026-07-27T18:43:11Z"),
    ];
    const files = groupArtifactsByFile(
      [
        artifact("implement_plan", REPORT, 8422),
        artifact("simplify", REPORT, 13162),
      ],
      stages,
    );

    expect(files[0].versions.map((v) => v.stageLabel)).toEqual([
      "simplify",
      "implement_plan",
    ]);
    expect(files[0].versions[0].size).toBe(13162);
  });

  test.each([
    ["equal", "2026-07-27T18:43:11Z", "2026-07-27T18:43:11Z"],
    ["missing", null, null],
  ])("preserves API order when stage timestamps are %s", (_case, firstAt, secondAt) => {
    const stages = [
      stage("implement_plan", firstAt),
      stage("simplify", secondAt),
    ];
    const files = groupArtifactsByFile(
      [
        artifact("simplify", REPORT, 13162),
        artifact("implement_plan", REPORT, 8422),
      ],
      stages,
    );

    expect(files[0].versions.map((v) => v.stageLabel)).toEqual([
      "simplify",
      "implement_plan",
    ]);
  });

  test("reports the byte change each capture introduced, oldest capture first", () => {
    const files = groupArtifactsByFile(
      [
        artifact("implement_plan", REPORT, 8422),
        artifact("simplify", REPORT, 13162),
        artifact("consolidate_reviews", REPORT, 14323),
        artifact("fix_review_findings", REPORT, 17483),
      ],
      STAGES,
    );

    // versions are newest-first, so deltas read 17483-14323, 14323-13162, ...
    expect(files[0].versions.map((v) => v.delta)).toEqual([3160, 1161, 4740, null]);
  });

  test("drops captures from graph control nodes", () => {
    const files = groupArtifactsByFile(
      [
        artifact("start", ".ai/reports/pre-existing.md", 12402),
        artifact("plan", ".ai/plans/plan.md", 21749),
      ],
      STAGES,
    );

    expect(files.map((file) => file.path)).toEqual([".ai/plans/plan.md"]);
  });

  test("sorts files by their most recent capture", () => {
    const files = groupArtifactsByFile(
      [
        artifact("plan", ".ai/plans/plan.md", 21749),
        artifact("fix_review_findings", REPORT, 17483),
        artifact("simplify", ".ai/reviews/bugs.xml", 5231),
      ],
      STAGES,
    );

    expect(files.map((file) => file.path)).toEqual([
      REPORT,
      ".ai/reviews/bugs.xml",
      ".ai/plans/plan.md",
    ]);
  });

  test("keeps retries of one stage as separate ordered versions", () => {
    const files = groupArtifactsByFile(
      [
        artifact("simplify", REPORT, 13162, 2),
        artifact("simplify", REPORT, 9000, 1),
      ],
      STAGES,
    );

    expect(files[0].versions.map((v) => v.retry)).toEqual([2, 1]);
    expect(files[0].versions[0].size).toBe(13162);
    expect(files[0].versions.map((v) => v.delta)).toEqual([4162, null]);
  });

  test("returns no files when every capture came from a control node", () => {
    const files = groupArtifactsByFile(
      [artifact("start", ".ai/reports/pre-existing.md", 12402)],
      STAGES,
    );

    expect(files).toEqual([]);
  });
});
