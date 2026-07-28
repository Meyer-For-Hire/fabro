import {
  afterEach,
  beforeEach,
  describe,
  expect,
  mock,
  test,
} from "bun:test";
import TestRenderer, { act } from "react-test-renderer";

import { setupReactTestEnv } from "../lib/test-utils";
import { DockComposer } from "./run-dock";

const mountedRenderers: TestRenderer.ReactTestRenderer[] = [];
let teardownReactEnv: (() => void) | undefined;

beforeEach(() => {
  teardownReactEnv = setupReactTestEnv();
});

afterEach(() => {
  for (const renderer of mountedRenderers.splice(0)) {
    act(() => renderer.unmount());
  }
  teardownReactEnv?.();
  teardownReactEnv = undefined;
});

describe("DockComposer", () => {
  test("describes its keyboard behavior to assistive technology", () => {
    let renderer!: TestRenderer.ReactTestRenderer;
    act(() => {
      renderer = TestRenderer.create(
        <DockComposer
          onSubmit={() => Promise.resolve(true)}
          placeholder="Write a message"
          submitLabel="Send"
          submitting={false}
          ariaLabel="Message"
        />,
      );
    });
    mountedRenderers.push(renderer);

    const textarea = renderer.root.findByType("textarea");
    const instruction = renderer.root.findByProps({
      id: textarea.props["aria-describedby"],
    });
    expect(instruction.children.join("")).toBe(
      "Press Enter to send. Press Shift+Enter for a new line.",
    );
    expect(textarea.props.name).toBeUndefined();
  });

  test("does not submit Enter while an IME composition is active", async () => {
    const onSubmit = mock(() => Promise.resolve(true));
    const preventDefault = mock(() => undefined);
    let renderer!: TestRenderer.ReactTestRenderer;
    act(() => {
      renderer = TestRenderer.create(
        <DockComposer
          onSubmit={onSubmit}
          placeholder="Write a message"
          submitLabel="Send"
          submitting={false}
          ariaLabel="Message"
        />,
      );
    });
    mountedRenderers.push(renderer);
    const textarea = renderer.root.findByType("textarea");

    act(() => textarea.props.onChange({ target: { value: "draft" } }));
    await act(async () => {
      textarea.props.onKeyDown({
        key: "Enter",
        shiftKey: false,
        nativeEvent: { isComposing: true },
        preventDefault,
      });
    });

    expect(preventDefault).not.toHaveBeenCalled();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  test("submits Enter after composition ends", async () => {
    const onSubmit = mock(() => Promise.resolve(true));
    const preventDefault = mock(() => undefined);
    let renderer!: TestRenderer.ReactTestRenderer;
    act(() => {
      renderer = TestRenderer.create(
        <DockComposer
          onSubmit={onSubmit}
          placeholder="Write a message"
          submitLabel="Send"
          submitting={false}
          ariaLabel="Message"
        />,
      );
    });
    mountedRenderers.push(renderer);
    const textarea = renderer.root.findByType("textarea");

    act(() => textarea.props.onChange({ target: { value: "  ready  " } }));
    await act(async () => {
      textarea.props.onKeyDown({
        key: "Enter",
        shiftKey: false,
        nativeEvent: { isComposing: false },
        preventDefault,
      });
    });

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith("ready");
  });
});
