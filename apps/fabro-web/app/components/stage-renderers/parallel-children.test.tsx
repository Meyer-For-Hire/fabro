import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { StageState } from "@qltysh/fabro-api-client";
import type { EventEnvelope } from "@qltysh/fabro-api-client";
import TestRenderer, { act } from "react-test-renderer";
import { MemoryRouter } from "react-router";

import { makeEventEnvelope, setupReactTestEnv } from "../../lib/test-utils";
import type { Stage } from "../stage-sidebar";
import { ParallelChildren } from "./parallel-children";

let teardown: () => void;
beforeEach(() => {
  teardown = setupReactTestEnv();
});
afterEach(() => teardown());

function makeStage(overrides: Partial<Stage> = {}): Stage {
  return {
    id: "stage@1",
    name: "stage",
    handler: "agent",
    status: StageState.RUNNING,
    duration: "--",
    nodeId: "stage",
    visit: 1,
    graphVisit: 1,
    resumedFromStageId: null,
    parallelGroupId: null,
    parallelBranchIndex: null,
    startedAt: "2026-04-09T12:00:00Z",
    providerUsed: null,
    ...overrides,
  };
}

const parallelStage = makeStage({
  id: "fork@1",
  name: "fork",
  handler: "parallel",
  status: StageState.RUNNING,
  duration: "12s",
  nodeId: "fork",
});

function branchStage(
  name: string,
  index: number,
  status: StageState,
  groupId = "fork@1",
  visit = 1,
): Stage {
  return makeStage({
    id: `${name}@${visit}`,
    name,
    nodeId: name,
    visit,
    status,
    parallelGroupId: groupId,
    parallelBranchIndex: index,
  });
}

function event(partial: Partial<EventEnvelope>): EventEnvelope {
  return makeEventEnvelope(partial.seq ?? 1, {
    event: "parallel.completed",
    stage_id: "fork@1",
    ...partial,
  });
}

function startedEvent(branchCount: number): EventEnvelope {
  return event({
    event: "parallel.started",
    properties: { branch_count: branchCount },
  });
}

function completedEvent(
  results: Array<{ id: string; status: string }>,
  successCount: number,
  failureCount: number,
): EventEnvelope {
  return event({
    seq: 2,
    event: "parallel.completed",
    properties: {
      duration_ms: 12000,
      success_count: successCount,
      failure_count: failureCount,
      results: results.map((result) => ({ ...result, context_updates: {} })),
    },
  });
}

function renderParallel(
  events: EventEnvelope[],
  allStages: Stage[],
  stage = parallelStage,
): TestRenderer.ReactTestRenderer {
  let renderer!: TestRenderer.ReactTestRenderer;
  act(() => {
    renderer = TestRenderer.create(
      <MemoryRouter>
        <ParallelChildren
          stage={stage}
          events={events}
          runId="run-1"
          allStages={allStages}
        />
      </MemoryRouter>,
    );
  });
  return renderer;
}

function textContent(node: TestRenderer.ReactTestInstance): string {
  return node.children
    .map((child) => typeof child === "string" ? child : textContent(child))
    .join("");
}

function branchRowText(renderer: TestRenderer.ReactTestRenderer): string[] {
  return renderer.root.findAllByType("li").map(textContent);
}

function hrefs(renderer: TestRenderer.ReactTestRenderer): string[] {
  return renderer.root.findAllByType("a").map((link) => link.props.href);
}

function statValue(renderer: TestRenderer.ReactTestRenderer, label: string): string {
  const stat = renderer.root
    .findAllByProps({ className: "flex flex-col gap-0.5" })
    .find((item) => textContent(item).startsWith(label));
  if (!stat) throw new Error(`stat ${label} not found`);
  return textContent(stat.findAllByType("span")[1]);
}

describe("ParallelChildren", () => {
  test("renders live branch names, statuses, counts, and stage links", () => {
    const renderer = renderParallel(
      [startedEvent(2)],
      [
        branchStage("review_glm", 0, StageState.SUCCEEDED),
        branchStage("review_opus", 1, StageState.RUNNING),
      ],
    );

    const rows = branchRowText(renderer);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toContain("Succeeded");
    expect(rows[0]).toContain("review_glm");
    expect(rows[1]).toContain("Running");
    expect(rows[1]).toContain("review_opus");
    expect(hrefs(renderer)).toEqual([
      "/runs/run-1/stages/review_glm@1",
      "/runs/run-1/stages/review_opus@1",
    ]);
    expect(statValue(renderer, "Succeeded")).toBe("1");
    expect(statValue(renderer, "Failed")).toBe("0");
  });

  test("keeps looped fork links scoped to the selected fork visit", () => {
    const renderer = renderParallel(
      [startedEvent(1)],
      [
        branchStage("review_glm", 0, StageState.SUCCEEDED, "fork@1", 1),
        branchStage("review_glm", 0, StageState.RUNNING, "fork@2", 2),
      ],
    );

    expect(hrefs(renderer)).toEqual(["/runs/run-1/stages/review_glm@1"]);
  });

  test("keeps duplicate branch targets in index order and only links recorded stages", () => {
    const renderer = renderParallel(
      [
        startedEvent(2),
        completedEvent(
          [
            { id: "review", status: "failed" },
            { id: "review", status: "failed" },
          ],
          1,
          1,
        ),
      ],
      [branchStage("review", 0, StageState.SUCCEEDED)],
    );

    const rows = branchRowText(renderer);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toContain("Succeeded");
    expect(rows[1]).toContain("Failed");
    expect(hrefs(renderer)).toEqual(["/runs/run-1/stages/review@1"]);
  });

  test("renders a completed result without a matching stage as an unlinked row", () => {
    const renderer = renderParallel(
      [
        startedEvent(1),
        completedEvent(
          [{ id: "legacy_branch", status: "succeeded" }],
          1,
          0,
        ),
      ],
      [],
    );

    expect(branchRowText(renderer)).toEqual(["Succeededlegacy_branch"]);
    expect(hrefs(renderer)).toEqual([]);
  });

  test("counts partial and skipped branches as neither succeeded nor failed", () => {
    const allStages = [
      branchStage("partial", 0, StageState.PARTIALLY_SUCCEEDED),
      branchStage("skipped", 1, StageState.SKIPPED),
    ];
    const running = renderParallel([startedEvent(2)], allStages);
    const completed = renderParallel(
      [
        startedEvent(2),
        completedEvent(
          [
            { id: "partial", status: "partially_succeeded" },
            { id: "skipped", status: "skipped" },
          ],
          0,
          0,
        ),
      ],
      allStages,
    );

    expect([
      statValue(running, "Succeeded"),
      statValue(running, "Failed"),
    ]).toEqual(["0", "0"]);
    expect([
      statValue(completed, "Succeeded"),
      statValue(completed, "Failed"),
    ]).toEqual(["0", "0"]);
  });
});
