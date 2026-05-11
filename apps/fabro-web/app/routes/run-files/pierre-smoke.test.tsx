import { describe, expect, test } from "bun:test";
import * as PierreDiffs from "@pierre/diffs/react";

const {
  MultiFileDiff,
  PatchDiff,
  Virtualizer,
  WorkerPoolContextProvider,
} = PierreDiffs;

describe("@pierre/diffs public API", () => {
  test("MultiFileDiff is a callable component export", () => {
    expect(typeof MultiFileDiff).toBe("function");
  });

  test("PatchDiff is a callable component export", () => {
    expect(typeof PatchDiff).toBe("function");
  });

  test("Virtualizer is a callable component export", () => {
    expect(typeof Virtualizer).toBe("function");
  });

  test("WorkerPoolContextProvider is a callable component export", () => {
    expect(typeof WorkerPoolContextProvider).toBe("function");
  });
});
