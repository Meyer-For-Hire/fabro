import {
  afterEach,
  beforeEach,
  describe,
  expect,
  mock,
  test,
} from "bun:test";
import { createElement, createRef } from "react";
import TestRenderer, { act } from "react-test-renderer";

import { setupReactTestEnv } from "../../lib/test-utils";

mock.module("../../components/interview-dock", () => ({
  InterviewDock: () => createElement("div", null, "Interview"),
}));
mock.module("../../components/steer-bar", () => ({
  SteerBar: () => createElement("div", null, "Steer"),
}));

const { RunDetailDockedControls } = await import("./docked-controls");
mock.restore();

const mountedRenderers: TestRenderer.ReactTestRenderer[] = [];
const observerCallbacks: ResizeObserverCallback[] = [];
const dockNode = { offsetHeight: 999 };
let originalResizeObserver: typeof ResizeObserver | undefined;
let teardownReactEnv: (() => void) | undefined;

function resizeEntry(blockSize: number): ResizeObserverEntry {
  return {
    borderBoxSize: [{ blockSize, inlineSize: 600 }],
  } as unknown as ResizeObserverEntry;
}

function renderDock({
  identity,
  hasPendingQuestions = false,
  onHeightChange,
}: {
  identity: string | null;
  hasPendingQuestions?: boolean;
  onHeightChange: (identity: string, height: number | null) => void;
}) {
  return (
    <RunDetailDockedControls
      key={identity ?? "hidden"}
      runId="run-1"
      dockIdentity={identity}
      hasPendingQuestions={hasPendingQuestions}
      pendingQuestions={[]}
      sidebarWidth={0}
      isResizing={false}
      steerBarRef={createRef()}
      waitingForSteer={false}
      onHeightChange={onHeightChange}
    />
  );
}

beforeEach(() => {
  teardownReactEnv = setupReactTestEnv();
  observerCallbacks.length = 0;
  originalResizeObserver = globalThis.ResizeObserver;
  globalThis.ResizeObserver = class ResizeObserver {
    constructor(callback: ResizeObserverCallback) {
      observerCallbacks.push(callback);
    }
    observe() {}
    unobserve() {}
    disconnect() {}
  } as typeof ResizeObserver;
});

afterEach(() => {
  for (const renderer of mountedRenderers.splice(0)) {
    act(() => renderer.unmount());
  }
  if (originalResizeObserver) {
    globalThis.ResizeObserver = originalResizeObserver;
  } else {
    delete (globalThis as { ResizeObserver?: typeof ResizeObserver })
      .ResizeObserver;
  }
  teardownReactEnv?.();
  teardownReactEnv = undefined;
});

describe("RunDetailDockedControls", () => {
  test("reports border-box height only when the height changes", () => {
    const onHeightChange = mock(
      (_identity: string, _height: number | null) => undefined,
    );
    let renderer!: TestRenderer.ReactTestRenderer;
    act(() => {
      renderer = TestRenderer.create(
        renderDock({ identity: "run-1:steer", onHeightChange }),
        {
          createNodeMock: () => dockNode,
        },
      );
    });
    mountedRenderers.push(renderer);
    const callback = observerCallbacks[0]!;

    act(() => callback([resizeEntry(108)], {} as ResizeObserver));
    act(() => callback([resizeEntry(108)], {} as ResizeObserver));
    act(() => callback([resizeEntry(120)], {} as ResizeObserver));

    expect(onHeightChange).toHaveBeenCalledTimes(2);
    expect(onHeightChange.mock.calls).toEqual([
      ["run-1:steer", 108],
      ["run-1:steer", 120],
    ]);
  });

  test("clears the old identity when the dock changes or hides", () => {
    const onHeightChange = mock(
      (_identity: string, _height: number | null) => undefined,
    );
    let renderer!: TestRenderer.ReactTestRenderer;
    act(() => {
      renderer = TestRenderer.create(
        renderDock({ identity: "run-1:steer", onHeightChange }),
        {
          createNodeMock: () => dockNode,
        },
      );
    });
    mountedRenderers.push(renderer);
    act(() =>
      observerCallbacks[0]!([resizeEntry(108)], {} as ResizeObserver),
    );

    act(() => {
      renderer.update(
        renderDock({
          identity: "run-1:interview",
          hasPendingQuestions: true,
          onHeightChange,
        }),
      );
    });
    act(() =>
      observerCallbacks[1]!([resizeEntry(220)], {} as ResizeObserver),
    );
    act(() => {
      renderer.update(renderDock({ identity: null, onHeightChange }));
    });

    expect(onHeightChange.mock.calls).toEqual([
      ["run-1:steer", 108],
      ["run-1:steer", null],
      ["run-1:interview", 220],
      ["run-1:interview", null],
    ]);
  });
});
