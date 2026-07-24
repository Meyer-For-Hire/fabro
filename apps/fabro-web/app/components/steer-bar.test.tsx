import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

import {
  isInterruptDisabled,
  SteerWaitingStatus,
} from "./steer-bar";

describe("SteerBar", () => {
  test("shows durable waiting state and prevents a second interrupt", () => {
    expect(isInterruptDisabled(true, false)).toBe(true);
    expect(isInterruptDisabled(false, true)).toBe(true);
    expect(isInterruptDisabled(false, false)).toBe(false);

    const html = renderToStaticMarkup(
      createElement(SteerWaitingStatus, { waitingForSteer: true }),
    );
    expect(html).toContain('role="status"');
    expect(html).toContain("Interrupted — waiting for steering");
  });
});
