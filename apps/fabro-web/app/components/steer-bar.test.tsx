import {
  afterEach,
  beforeEach,
  describe,
  expect,
  mock,
  test,
} from "bun:test";
import { createRef } from "react";
import TestRenderer, { act } from "react-test-renderer";

import { setupReactTestEnv } from "../lib/test-utils";

let steerPending = false;
let interruptPending = false;
const steerTrigger = mock(() => Promise.resolve(undefined));
const interruptTrigger = mock(() => Promise.resolve(undefined));

mock.module("../lib/mutations", () => ({
  useSteerRun: () => ({
    isMutating: steerPending,
    trigger: steerTrigger,
  }),
  useInterruptRun: () => ({
    isMutating: interruptPending,
    trigger: interruptTrigger,
  }),
}));

const {
  isInterruptDisabled,
  isSteerDockCollapsed,
  SteerBar,
  steerStatusLabel,
} = await import("./steer-bar");
type SteerBarHandle = import("./steer-bar").SteerBarHandle;
mock.restore();

const mountedRenderers: TestRenderer.ReactTestRenderer[] = [];
let teardownReactEnv: (() => void) | undefined;

function textFromNode(node: TestRenderer.ReactTestInstance): string {
  return node.children
    .map((child) =>
      typeof child === "string" ? child : textFromNode(child),
    )
    .join("");
}

beforeEach(() => {
  teardownReactEnv = setupReactTestEnv();
  steerPending = false;
  interruptPending = false;
  steerTrigger.mockClear();
  interruptTrigger.mockClear();
});

afterEach(() => {
  for (const renderer of mountedRenderers.splice(0)) {
    act(() => renderer.unmount());
  }
  teardownReactEnv?.();
  teardownReactEnv = undefined;
});

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

  test("the focus handle expands a collapsed dock before focusing", async () => {
    const focus = mock(() => undefined);
    const ref = createRef<SteerBarHandle>();
    let renderer!: TestRenderer.ReactTestRenderer;
    await act(async () => {
      renderer = TestRenderer.create(
        <SteerBar ref={ref} runId="run-1" />,
        {
          createNodeMock: (element) =>
            element.type === "textarea" ? { focus } : null,
        },
      );
    });
    mountedRenderers.push(renderer);

    // The dock starts collapsed by default.
    expect(
      renderer.root.findByProps({
        "aria-label": "Expand Steer running agent",
      }),
    ).toBeDefined();

    act(() => ref.current?.focus());
    expect(
      renderer.root.findByProps({
        "aria-label": "Collapse Steer running agent",
      }),
    ).toBeDefined();
    expect(focus).not.toHaveBeenCalled();

    await act(
      async () =>
        await new Promise((resolve) => {
          setTimeout(resolve, 0);
        }),
    );
    expect(focus).toHaveBeenCalledTimes(1);
  });

  test("interrupt progress disables the composer without calling it sending", async () => {
    interruptPending = true;
    let renderer!: TestRenderer.ReactTestRenderer;
    await act(async () => {
      renderer = TestRenderer.create(<SteerBar runId="run-1" />);
    });
    mountedRenderers.push(renderer);

    const submit = renderer.root.findByProps({ type: "submit" });
    expect(submit.props.disabled).toBe(true);
    expect(textFromNode(submit)).toBe("Send");
    expect(
      renderer.root
        .findAllByType("button")
        .map(textFromNode),
    ).toContain("Interrupting…");
  });
});
