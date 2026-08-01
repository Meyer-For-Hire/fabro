import { describe, expect, test } from "bun:test";
import type { EventEnvelope } from "@qltysh/fabro-api-client";

import { makeEventEnvelope } from "../../lib/test-utils";
import {
  extractStageContext,
  parseHumanInterviewPairs,
  parseParallelOverview,
  parseReducerTranscript,
} from "./helpers";

describe("parseHumanInterviewPairs", () => {
  test("pairs interview.started with interview.completed by question_id", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "interview.started",
        properties: {
          question_id: "q-1",
          question: "Approve PR?",
          question_type: "yes_no",
          options: [
            { key: "y", label: "Yes" },
            { key: "n", label: "No" },
          ],
          allow_freeform: false,
        },
      }),
      makeEventEnvelope(2, {
        event: "interview.completed",
        properties: {
          question_id: "q-1",
          question: "Approve PR?",
          answer: "y",
          duration_ms: 4200,
          actor: { kind: "user", email: "alice@example.com" },
        },
      }),
    ];

    const pairs = parseHumanInterviewPairs(events);
    expect(pairs).toHaveLength(1);
    expect(pairs[0].question.questionType).toBe("yes_no");
    expect(pairs[0].question.options).toEqual([
      { key: "y", label: "Yes" },
      { key: "n", label: "No" },
    ]);
    const resolution = pairs[0].resolution;
    expect(resolution).not.toBeNull();
    if (resolution?.kind === "answered") {
      expect(resolution.answer).toBe("y");
      expect(resolution.actor).toBe("alice@example.com");
      expect(resolution.durationMs).toBe(4200);
    }
  });

  test("leaves resolution null for unanswered (still pending) questions", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "interview.started",
        properties: {
          question_id: "q-1",
          question: "Pick a branch",
          question_type: "multiple_choice",
        },
      }),
    ];
    const pairs = parseHumanInterviewPairs(events);
    expect(pairs[0].resolution).toBeNull();
  });

  test("preserves option description and preview metadata from started events", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "interview.started",
        properties: {
          question_id: "q-1",
          question: "Pick a path",
          question_type: "multiple_choice",
          options: [
            {
              key: "ship",
              label: "Ship",
              description: "Deploy the current patch",
              preview: "diff preview",
            },
          ],
        },
      }),
    ];

    const pairs = parseHumanInterviewPairs(events);

    expect(pairs[0].question.options[0]).toEqual({
      key: "ship",
      label: "Ship",
      description: "Deploy the current patch",
      preview: "diff preview",
    });
  });

  test("preserves a typed review target from started events", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "interview.started",
        properties: {
          question_id: "q-1",
          question:
            "Review the Quarry review exercise document, then choose the next action.",
          question_type: "multiple_choice",
          review_target: {
            label: "Quarry review exercise",
            url: "https://quarry.lithos.computer/tmp/0123456789abcdef0123456789abcdef",
            kind: "document",
          },
        },
      }),
    ];

    const pairs = parseHumanInterviewPairs(events);

    expect(pairs[0].question.reviewTarget).toEqual({
      label: "Quarry review exercise",
      url: "https://quarry.lithos.computer/tmp/0123456789abcdef0123456789abcdef",
      kind: "document",
    });
  });

  test("captures timeout and interrupted resolutions", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "interview.started",
        properties: { question_id: "q-1", question: "?", question_type: "freeform" },
      }),
      makeEventEnvelope(2, {
        event: "interview.timeout",
        properties: { question_id: "q-1", duration_ms: 30000 },
      }),
      makeEventEnvelope(3, {
        event: "interview.started",
        properties: { question_id: "q-2", question: "?", question_type: "freeform" },
      }),
      makeEventEnvelope(4, {
        event: "interview.interrupted",
        properties: {
          question_id: "q-2",
          reason: "user cancelled",
          duration_ms: 1200,
          actor: { kind: "user", email: "bob@example.com" },
        },
      }),
    ];
    const pairs = parseHumanInterviewPairs(events);
    expect(pairs[0].resolution?.kind).toBe("timeout");
    expect(pairs[1].resolution?.kind).toBe("interrupted");
    if (pairs[1].resolution?.kind === "interrupted") {
      expect(pairs[1].resolution.reason).toBe("user cancelled");
      expect(pairs[1].resolution.actor).toBe("bob@example.com");
    }
  });
});

describe("parseParallelOverview", () => {
  test("rolls up branch_count and status-only results", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "parallel.started",
        properties: { branch_count: 3 },
      }),
      makeEventEnvelope(2, {
        event: "parallel.completed",
        properties: {
          duration_ms: 12000,
          success_count: 2,
          failure_count: 1,
          results: [
            {
              id: "branch-a",
              status: "succeeded",
              context_updates: { "response.branch-a": "A" },
            },
            {
              id: "branch-b",
              status: "succeeded",
              context_updates: { "command.output": { stdout: "B" } },
            },
            {
              id: "branch-c",
              status: "failed",
              context_updates: { "response.branch-c": "C" },
            },
          ],
        },
      }),
    ];
    const overview = parseParallelOverview(events);
    expect(overview).toEqual({
      branchCount: 3,
      results: [
        { id: "branch-a", index: null, itemLabel: null, status: "succeeded" },
        { id: "branch-b", index: null, itemLabel: null, status: "succeeded" },
        { id: "branch-c", index: null, itemLabel: null, status: "failed" },
      ],
    });
  });

  test("parses dynamic item identity from results", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "parallel.completed",
        properties: {
          duration_ms: 20,
          success_count: 2,
          failure_count: 0,
          results: [
            {
              id: "reviewer",
              index: 0,
              item_label: "auth",
              status: "succeeded",
              context_updates: {},
            },
            {
              id: "reviewer",
              index: 1,
              item_label: "api",
              status: "succeeded",
              context_updates: {},
            },
          ],
        },
      }),
    ];

    expect(parseParallelOverview(events).results).toEqual([
      { id: "reviewer", index: 0, itemLabel: "auth", status: "succeeded" },
      { id: "reviewer", index: 1, itemLabel: "api", status: "succeeded" },
    ]);
  });

  test("reports in-flight when only the started event is present", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "parallel.started",
        properties: { branch_count: 4 },
      }),
    ];
    const overview = parseParallelOverview(events);
    expect(overview.branchCount).toBe(4);
    expect(overview.results).toEqual([]);
  });
});

describe("parseReducerTranscript", () => {
  test("returns null when fan-in joins without a reducer", () => {
    expect(parseReducerTranscript([])).toBeNull();
  });

  test("parses the standard prompt transcript when a reducer ran", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "stage.prompt",
        properties: {
          mode: "prompt",
          text: "Combine the branch results.",
          model: "claude-sonnet-4-6",
        },
      }),
      makeEventEnvelope(2, {
        event: "prompt.completed",
        properties: {
          response: "The branch results are joined.",
          billing: { input_tokens: 1200, output_tokens: 340 },
        },
      }),
    ];

    expect(parseReducerTranscript(events)).toEqual({
      prompt: "Combine the branch results.",
      response: "The branch results are joined.",
      model: "claude-sonnet-4-6",
      inputTokens: 1200,
      outputTokens: 340,
    });
  });

  test("uses normal prompt mode for the reducer transcript", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "stage.prompt",
        properties: { mode: "prompt", text: "Standard reducer" },
      }),
      makeEventEnvelope(2, {
        event: "prompt.completed",
        properties: { response: "Standard response" },
      }),
    ];

    expect(parseReducerTranscript(events)?.response).toBe("Standard response");
  });
});

describe("extractStageContext", () => {
  test("keeps author-set keys and drops engine bookkeeping keys", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "stage.completed",
        properties: {
          context_updates: {
            "plan.summary": "ship the thing",
            review_score: 8,
            last_stage: "implement",
            last_response: "done",
            "response.implement": "full text",
            "internal.run_id": "run-1",
            "current.preamble": "...",
            "command.output": "blob:abc",
            "human.gate.selected": "A",
            "parallel.results": [],
          },
        },
      }),
    ];
    const ctx = extractStageContext(events);
    expect(ctx).not.toBeNull();
    expect(ctx?.updates).toEqual({
      "plan.summary": "ship the thing",
      review_score: 8,
    });
  });

  test("extracts routing hints from preferred_label and suggested_next_ids", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "stage.completed",
        properties: {
          preferred_label: "approve",
          suggested_next_ids: ["review", "merge", 7],
        },
      }),
    ];
    const ctx = extractStageContext(events);
    expect(ctx?.routing.preferredLabel).toBe("approve");
    expect(ctx?.routing.suggestedNextIds).toEqual(["review", "merge"]);
    expect(ctx?.updates).toEqual({});
  });

  test("returns null when the stage only wrote engine keys", () => {
    const events: EventEnvelope[] = [
      makeEventEnvelope(1, {
        event: "stage.completed",
        properties: {
          context_updates: { last_stage: "implement", "command.output": "blob:x" },
        },
      }),
    ];
    expect(extractStageContext(events)).toBeNull();
  });

  test("returns null when the stage has not completed", () => {
    expect(extractStageContext([])).toBeNull();
  });
});
