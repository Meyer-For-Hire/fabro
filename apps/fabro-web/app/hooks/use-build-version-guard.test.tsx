import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import TestRenderer, { act } from "react-test-renderer";
import { toast as sonnerToast } from "sonner";

import { setupReactTestEnv } from "../lib/test-utils";

const loadedBuildId = "aaaaaaaa";
let latestBuildId: string | null = loadedBuildId;

mock.module("../lib/build-version", () => ({
  documentBuildId: () => loadedBuildId,
  isStaleBuild: (loaded: string | null, latest: string | null) =>
    loaded != null && latest != null && loaded !== latest,
  useLatestBuildId: () => latestBuildId,
}));

const { useBuildVersionGuard } = await import("./use-build-version-guard");

let renderer: TestRenderer.ReactTestRenderer | null = null;
let restoreReactTestEnv = () => {};
let reloads = 0;
let previousWindow: unknown;
let hadWindow = false;
let previousRequestAnimationFrame: typeof requestAnimationFrame | undefined;

function GuardHost({ revision }: { revision: number }) {
  void revision;
  useBuildVersionGuard();
  return null;
}

function activeToasts() {
  return sonnerToast.getToasts().filter((toast) => !toast.delete);
}

function renderRevision(revision: number) {
  act(() => {
    if (renderer) {
      renderer.update(<GuardHost revision={revision} />);
    } else {
      renderer = TestRenderer.create(<GuardHost revision={revision} />);
    }
  });
}

beforeEach(() => {
  restoreReactTestEnv = setupReactTestEnv();
  latestBuildId = loadedBuildId;
  reloads = 0;
  hadWindow = "window" in globalThis;
  previousWindow = (globalThis as { window?: unknown }).window;
  previousRequestAnimationFrame = globalThis.requestAnimationFrame;
  globalThis.requestAnimationFrame = (callback) =>
    setTimeout(callback, 0) as unknown as number;
  Object.defineProperty(globalThis, "window", {
    value: {
      location: {
        reload: () => {
          reloads += 1;
        },
      },
    },
    writable:     true,
    configurable: true,
  });
});

afterEach(() => {
  act(() => {
    renderer?.unmount();
  });
  renderer = null;
  sonnerToast.dismiss();
  if (hadWindow) {
    Object.defineProperty(globalThis, "window", {
      value:        previousWindow,
      writable:     true,
      configurable: true,
    });
  } else {
    delete (globalThis as { window?: unknown }).window;
  }
  if (previousRequestAnimationFrame) {
    globalThis.requestAnimationFrame = previousRequestAnimationFrame;
  } else {
    delete (globalThis as { requestAnimationFrame?: typeof requestAnimationFrame })
      .requestAnimationFrame;
  }
  restoreReactTestEnv();
});

test("keeps one actionable prompt synchronized with the latest stale build", () => {
  renderRevision(0);
  expect(activeToasts()).toHaveLength(0);

  latestBuildId = "bbbbbbbb";
  renderRevision(1);
  const firstPrompt = activeToasts();
  expect(firstPrompt).toHaveLength(1);
  expect(firstPrompt[0]).toMatchObject({
    duration: Infinity,
    title:    "A new version of Fabro is available.",
  });

  const action = firstPrompt[0]?.action;
  expect(action).toMatchObject({ label: "Reload" });
  if (action && typeof action === "object" && "onClick" in action) {
    action.onClick({} as never);
  }
  expect(reloads).toBe(1);

  // Re-rendering the same poll result neither re-nags nor replaces the toast.
  renderRevision(2);
  expect(activeToasts()).toHaveLength(1);
  expect(activeToasts()[0]?.id).toBe(firstPrompt[0]?.id);

  // Later deploys replace the active prompt instead of accumulating forever.
  latestBuildId = "cccccccc";
  renderRevision(3);
  expect(activeToasts()).toHaveLength(1);
  expect(activeToasts()[0]?.id).not.toBe(firstPrompt[0]?.id);

  latestBuildId = "bbbbbbbb";
  renderRevision(4);
  expect(activeToasts()).toHaveLength(1);

  // If the server rolls back to the document's build, the claim is no longer
  // true and the prompt disappears.
  latestBuildId = loadedBuildId;
  renderRevision(5);
  expect(activeToasts()).toHaveLength(0);
});
