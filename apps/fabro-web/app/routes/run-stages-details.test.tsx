import { describe, expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import type {
  BilledTokenCounts,
  ReasoningOutput,
  StageModelUsage,
} from "@qltysh/fabro-api-client";

import { makeBilledTokenCounts } from "../lib/test-fixtures";
import { EventDetails, ModelUsagePopover } from "./run-stages";

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

const PROVIDER_USED: StageModelUsage = {
  mode: "agent",
  provider: "moonshot",
  model: "kimi-k3",
  reasoning_effort: "max",
};

function popoverMarkup(counts: BilledTokenCounts): string {
  return renderToStaticMarkup(
    <ModelUsagePopover providerUsed={PROVIDER_USED} billing={counts} />,
  );
}

describe("ModelUsagePopover billing", () => {
  test("shows the visit's token buckets and cost next to the model", () => {
    const html = popoverMarkup(
      makeBilledTokenCounts({
        input_tokens: 28_640,
        output_tokens: 7_550,
        reasoning_tokens: 1_200,
        cache_read_tokens: 4_800,
        cache_write_tokens: 1_500,
        total_tokens: 43_690,
        total_usd_micros: 720_000,
      }),
    );

    expect(html).toContain("kimi-k3");
    expect(html).toContain("Cache read");
    expect(html).toContain("4.8k");
    expect(html).toContain("Cache creation");
    expect(html).toContain("1.5k");
    expect(html).toContain("Uncached");
    expect(html).toContain("28.6k");
    // Output folds in reasoning tokens, matching the Billing tab.
    expect(html).toContain("Output");
    expect(html).toContain("8.8k");
    expect(html).toContain("Cost");
    expect(html).toContain("$0.72");
  });

  test("omits the token section for a stage that called no model", () => {
    const html = popoverMarkup(makeBilledTokenCounts());

    expect(html).toContain("kimi-k3");
    expect(html).not.toContain("Tokens");
    expect(html).not.toContain("Cost");
  });

  test("still shows tokens when nothing priced the stage", () => {
    const html = popoverMarkup(
      makeBilledTokenCounts({
        input_tokens: 1_000,
        output_tokens: 500,
        total_tokens: 1_500,
      }),
    );

    expect(html).toContain("Uncached");
    expect(html).toContain("1.0k");
    expect(html).not.toContain("Cost");
  });

  test("shows a provider-reported cost when token counts are unavailable", () => {
    const html = popoverMarkup(
      makeBilledTokenCounts({ total_usd_micros: 720_000 }),
    );

    expect(html).toContain("kimi-k3");
    expect(html).toContain("Cost");
    expect(html).toContain("$0.72");
  });
});
