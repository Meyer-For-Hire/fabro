import { describe, expect, test } from "bun:test";
import {
  modelOfferingKey,
  modelOfferingTestArgs,
} from "./model-offerings";

describe("model offering identity", () => {
  const openai = { id: "portable-model", provider: "openai" };
  const openrouter = { id: "portable-model", provider: "openrouter" };

  test("keeps duplicate model IDs in independent row state", () => {
    expect(modelOfferingKey(openai)).not.toBe(modelOfferingKey(openrouter));

    const state = new Map([
      [modelOfferingKey(openai), "ok"],
      [modelOfferingKey(openrouter), "error"],
    ]);
    expect(state.get(modelOfferingKey(openai))).toBe("ok");
    expect(state.get(modelOfferingKey(openrouter))).toBe("error");
  });

  test("includes the row provider in model-test request arguments", () => {
    expect(modelOfferingTestArgs(openai)).toEqual([
      "portable-model",
      "openai",
    ]);
    expect(modelOfferingTestArgs(openrouter)).toEqual([
      "portable-model",
      "openrouter",
    ]);
  });
});
