import { describe, expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import type { ReasoningOutput } from "@qltysh/fabro-api-client";

import { EventDetails } from "./run-stages";

const RUN_START = "2026-04-09T12:00:00Z";

function assistantMarkup(reasoning: ReasoningOutput | null): string {
  return renderToStaticMarkup(
    <EventDetails
      turn={{
        kind: "assistant",
        ts: "2026-04-09T12:00:05Z",
        content: "Refactored the auth module.",
        inputTokens: 120,
        outputTokens: 30,
        toolCallCount: null,
        reasoning,
      }}
      runStart={RUN_START}
    />,
  );
}

describe("EventDetails reasoning", () => {
  test("shows nothing when the response disclosed no reasoning", () => {
    const html = assistantMarkup(null);

    expect(html).toContain("Refactored the auth module.");
    expect(html).not.toContain("Reasoning");
  });

  test("labels a trace-only response Reasoning, not Reasoning trace", () => {
    const html = assistantMarkup({ trace: "Considered A." });

    expect(html).toContain("Reasoning");
    expect(html).not.toContain("Reasoning trace");
    expect(html).toContain("Considered A.");
  });

  test("distinguishes the summary from the verbatim trace when both arrive", () => {
    const html = assistantMarkup({
      summary: "Checked the config.",
      trace: "Considered A.",
    });

    expect(html).toContain("Reasoning trace");
    expect(html).toContain("Checked the config.");
    expect(html).toContain("Considered A.");
  });

  test("renders short reasoning in full, with no disclosure control", () => {
    const html = assistantMarkup({ trace: "Considered A." });

    expect(html).not.toContain("Show all");
    expect(html).not.toContain("aria-expanded");
  });

  test("connects a long trace's expand button to its controlled content", () => {
    const trace = "x".repeat(281);
    const html = assistantMarkup({ trace });

    const controls = html.match(/aria-controls="([^"]+)"/)?.[1];
    expect(controls).toBeDefined();
    expect(html).toContain(`id="${controls}"`);
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("Show all (281 characters)");
    // Collapsed, so the preview is truncated rather than the whole trace.
    expect(html).not.toContain(trace);
    expect(html).toContain(`${"x".repeat(280)}…`);
  });
});
