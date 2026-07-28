import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import type { EventEnvelope } from "@qltysh/fabro-api-client";
import TestRenderer, { act } from "react-test-renderer";
import { MemoryRouter } from "react-router";

import { makeEventEnvelope, setupReactTestEnv } from "../../lib/test-utils";
import { makeBilledTokenCounts } from "../../lib/test-fixtures";
import type { Stage } from "../stage-sidebar";
import { ParallelChildren } from "./parallel-children";

let teardown: () => void;
beforeEach(() => {
  teardown = setupReactTestEnv();
});
afterEach(() => teardown());

const parallelStage: Stage = {
  id: "fork@1",
  name: "fork",
  handler: "parallel",
  status: "succeeded",
  duration: "12s",
  nodeId: "fork",
  visit: 1,
  startedAt: "2026-04-09T12:00:00Z",
  providerUsed: null,
  billing: makeBilledTokenCounts(),
};

function event(partial: Partial<EventEnvelope>): EventEnvelope {
  return makeEventEnvelope(partial.seq ?? 1, { event: "parallel.completed", ...partial });
}

function renderParallel(events: EventEnvelope[]): TestRenderer.ReactTestRenderer {
  let renderer!: TestRenderer.ReactTestRenderer;
  act(() => {
    renderer = TestRenderer.create(
      <MemoryRouter>
        <ParallelChildren
          stage={parallelStage}
          events={events}
          runId="run-1"
          allStages={[
            { ...parallelStage, id: "branch-a@1", name: "branch-a", nodeId: "branch-a", handler: "agent" },
            { ...parallelStage, id: "branch-b@1", name: "branch-b", nodeId: "branch-b", handler: "agent", status: "failed" },
          ]}
        />
      </MemoryRouter>,
    );
  });
  return renderer;
}

describe("ParallelChildren", () => {
  test("renders branch status and stage links without checkout metadata", () => {
    const renderer = renderParallel([
      event({
        event: "parallel.started",
        properties: { branch_count: 2 },
      }),
      event({
        seq: 2,
        event: "parallel.completed",
        properties: {
          duration_ms: 12000,
          success_count: 1,
          failure_count: 1,
          results: [
            { id: "branch-a", status: "succeeded", context_updates: {} },
            { id: "branch-b", status: "failed", context_updates: {} },
          ],
        },
      }),
    ]);

    const rendered = JSON.stringify(renderer.toJSON());
    expect(rendered).toContain("branch-a");
    expect(rendered).toContain("Succeeded");
    expect(rendered).toContain("branch-b");
    expect(rendered).toContain("Failed");
    const hrefs = renderer.root.findAllByType("a").map((link) => link.props.href);
    expect(hrefs).toEqual([
      "/runs/run-1/stages/branch-a@1",
      "/runs/run-1/stages/branch-b@1",
    ]);
  });
});
