import { describe, expect, test } from "bun:test";

import {
  isInterruptDisabled,
  isSteerDockCollapsed,
  steerStatusLabel,
} from "./steer-bar";

describe("SteerBar", () => {
  test("prevents a second interrupt while one is in flight or already settled", () => {
    expect(isInterruptDisabled(true, false)).toBe(true);
    expect(isInterruptDisabled(false, true)).toBe(true);
    expect(isInterruptDisabled(false, false)).toBe(false);
  });

  test("names the durable waiting state in the dock header", () => {
    expect(steerStatusLabel(true)).toBe("Interrupted — waiting for steering");
    expect(steerStatusLabel(false)).toBe("Steering");
  });

  test("reopens the dock while the run waits for steering", () => {
    expect(isSteerDockCollapsed(true, false)).toBe(true);
    expect(isSteerDockCollapsed(false, false)).toBe(false);
    // Collapsing cannot hide a run that is blocked on the operator.
    expect(isSteerDockCollapsed(true, true)).toBe(false);
    expect(isSteerDockCollapsed(false, true)).toBe(false);
  });
});
