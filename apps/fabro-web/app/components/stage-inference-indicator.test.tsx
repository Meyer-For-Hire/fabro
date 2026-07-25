import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import TestRenderer, { act } from "react-test-renderer";

import { LlmOutputKind } from "@qltysh/fabro-api-client";
import type { StageInferenceProjection } from "@qltysh/fabro-api-client";

import { StageInferenceIndicator } from "./stage-inference-indicator";

const OPENED_AT = new Date(Date.now() - 12_000).toISOString();

function makeInference(
  overrides: Partial<StageInferenceProjection> = {},
): StageInferenceProjection {
  return {
    session_id:      "ses_root",
    started_at:      OPENED_AT,
    requested_model: {
      provider: "anthropic",
      model_id: "claude-fable-5",
    },
    retries:         0,
    ...overrides,
  };
}

function render(
  inference: StageInferenceProjection | null | undefined,
  settled = false,
): string {
  let renderer!: TestRenderer.ReactTestRenderer;
  act(() => {
    renderer = TestRenderer.create(
      <StageInferenceIndicator inference={inference} settled={settled} />,
    );
  });
  const output = JSON.stringify(renderer.toJSON());
  act(() => renderer.unmount());
  return output;
}

describe("StageInferenceIndicator", () => {
  const actGlobal = globalThis as {
    IS_REACT_ACT_ENVIRONMENT?: boolean;
  };
  const previousActEnvironment = actGlobal.IS_REACT_ACT_ENVIRONMENT;

  beforeEach(() => {
    actGlobal.IS_REACT_ACT_ENVIRONMENT = true;
  });

  afterEach(() => {
    if (previousActEnvironment === undefined) {
      delete actGlobal.IS_REACT_ACT_ENVIRONMENT;
    } else {
      actGlobal.IS_REACT_ACT_ENVIRONMENT = previousActEnvironment;
    }
  });

  test("renders nothing without an open bracket", () => {
    expect(render(undefined)).toBe("null");
    expect(render(null)).toBe("null");
  });

  test("names the requested model while nothing has come back", () => {
    const output = render(makeInference());
    expect(output).toContain("Model request");
    expect(output).toContain("waiting on claude-fable-5");
    expect(output).toContain('"aria-live":"polite"');
    expect(output).toContain('"aria-hidden":"true"');
    // No completion estimate exists, so none may be shown.
    expect(output).not.toContain("%");
  });

  test("reports the observed first-output kind", () => {
    expect(
      render(makeInference({ first_output_kind: LlmOutputKind.REASONING })),
    ).toContain("reasoning");
    expect(
      render(makeInference({ first_output_kind: LlmOutputKind.TEXT })),
    ).toContain("writing");
    expect(
      render(makeInference({ first_output_kind: LlmOutputKind.TOOL_CALL })),
    ).toContain("calling tools");
  });

  test("never says thinking for non-reasoning output", () => {
    for (const kind of [LlmOutputKind.TEXT, LlmOutputKind.TOOL_CALL]) {
      expect(render(makeInference({ first_output_kind: kind }))).not.toContain(
        "thinking",
      );
    }
  });

  test("counts retries without presenting them as failure", () => {
    const output = render(makeInference({ retries: 2 }));
    expect(output).toContain("retry 2");
    expect(output).not.toContain("failed");
  });

  test("goes static once the run can no longer advance the bracket", () => {
    const output = render(makeInference(), true);
    // An open bracket on a settled run means we never learned how the request
    // ended — animating it would claim work that may not be happening.
    expect(output).toContain("never completed");
    expect(output).not.toContain("animate-pulse");
  });
});
