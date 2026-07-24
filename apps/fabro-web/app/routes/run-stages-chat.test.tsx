import { describe, expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { StageState } from "@qltysh/fabro-api-client";

import type { Stage } from "../lib/stage-sidebar";
import { StageChatView } from "./run-stages";

function stage(overrides: Partial<Stage> = {}): Stage {
  return {
    id: "plan@1",
    name: "Plan",
    handler: "agent",
    status: StageState.SUCCEEDED,
    duration: "1m 12s",
    visit: 1,
    nodeId: "plan",
    graphVisit: 1,
    resumedFromStageId: null,
    startedAt: "2026-04-09T12:00:00Z",
    providerUsed: null,
    ...overrides,
  };
}

describe("StageChatView", () => {
  test("shows a completed stage duration when the final assistant turn has no tokens", () => {
    const html = renderToStaticMarkup(
      <StageChatView
        turns={[
          {
            kind: "assistant",
            ts: "2026-04-09T12:00:01Z",
            content: "Finished",
            inputTokens: 0,
            outputTokens: 0,
          },
          {
            kind: "tool",
            ts: "2026-04-09T12:00:02Z",
            toolName: "shell",
            input: "{}",
            result: "ok",
            isError: false,
            durationMs: 5,
          },
        ]}
        pendingTools={[]}
        stage={stage()}
      />,
    );

    expect(html).toContain("1m 12s");
  });

  test("connects a long prompt's expand button to its controlled content", () => {
    const html = renderToStaticMarkup(
      <StageChatView
        turns={[
          {
            kind: "system",
            ts: "2026-04-09T12:00:00Z",
            content: "x".repeat(281),
          },
        ]}
        pendingTools={[]}
        stage={stage()}
      />,
    );

    const controls = html.match(/aria-controls="([^"]+)"/)?.[1];
    expect(controls).toBeDefined();
    expect(html).toContain(`id="${controls}"`);
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("Show all (281 characters)");
  });

  test("announces in-progress tool calls as a polite live status", () => {
    const html = renderToStaticMarkup(
      <StageChatView
        turns={[]}
        pendingTools={[
          {
            toolCallId: "call-1",
            toolName: "shell",
            input: JSON.stringify({ command: "cargo build" }),
          },
        ]}
        stage={stage({ status: StageState.RUNNING, duration: "--" })}
      />,
    );

    expect(html).toContain('role="status"');
    expect(html).toContain('aria-live="polite"');
    expect(html).toContain("1 tool call in progress");
  });
});
