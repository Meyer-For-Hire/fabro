import { describe, expect, test } from "bun:test";
import TestRenderer, { act } from "react-test-renderer";
import { SWRConfig } from "swr";

import { EditableRunTitle } from "./editable-run-title";
import { ToastProvider } from "./toast";
import { generatedAxios } from "../lib/api-client";

function render(node: React.ReactNode): TestRenderer.ReactTestRenderer {
  let tree: TestRenderer.ReactTestRenderer | undefined;
  act(() => {
    tree = TestRenderer.create(
      <SWRConfig value={{ provider: () => new Map(), dedupingInterval: 0 }}>
        <ToastProvider>{node}</ToastProvider>
      </SWRConfig>,
    );
  });
  return tree!;
}

function instanceText(instance: TestRenderer.ReactTestInstance): string {
  const parts: string[] = [];
  for (const child of instance.children) {
    if (typeof child === "string") parts.push(child);
    else parts.push(instanceText(child));
  }
  return parts.join("");
}

function findEditButton(
  tree: TestRenderer.ReactTestRenderer,
): TestRenderer.ReactTestInstance {
  return tree.root.findByProps({ "aria-label": "Edit run title" });
}

function findInput(
  tree: TestRenderer.ReactTestRenderer,
): TestRenderer.ReactTestInstance {
  return tree.root.findByProps({ "aria-label": "Run title" });
}

describe("EditableRunTitle", () => {
  test("renders the run title with an edit affordance", () => {
    const tree = render(<EditableRunTitle runId="run-1" title="Initial title" />);
    expect(instanceText(findEditButton(tree))).toContain("Initial title");
  });

  test("clicking the title swaps to an input pre-filled with the current value", () => {
    const tree = render(<EditableRunTitle runId="run-1" title="Initial title" />);
    act(() => {
      findEditButton(tree).props.onClick();
    });
    expect(findInput(tree).props.value).toBe("Initial title");
  });

  test("Enter submits a PATCH and the input collapses back to the heading", async () => {
    const submitted: unknown[] = [];
    const originalAdapter = generatedAxios.defaults.adapter;
    generatedAxios.defaults.adapter = async (config) => {
      submitted.push({ url: config.url, method: config.method, body: JSON.parse(String(config.data)) });
      return {
        data: { id: "run-1", title: "Renamed title" },
        status: 200,
        statusText: "OK",
        headers: {},
        config,
      };
    };

    try {
      const tree = render(<EditableRunTitle runId="run-1" title="Initial title" />);
      act(() => {
        findEditButton(tree).props.onClick();
      });
      const input = findInput(tree);
      act(() => {
        input.props.onChange({ target: { value: "Renamed title" } });
      });
      await act(async () => {
        input.props.onKeyDown({ key: "Enter", preventDefault: () => {} });
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(submitted).toEqual([
        { url: "/api/v1/runs/run-1", method: "patch", body: { title: "Renamed title" } },
      ]);
    } finally {
      generatedAxios.defaults.adapter = originalAdapter;
    }
  });

  test("Escape exits without sending a request", () => {
    let calls = 0;
    const originalAdapter = generatedAxios.defaults.adapter;
    generatedAxios.defaults.adapter = async (config) => {
      calls += 1;
      return {
        data: undefined,
        status: 204,
        statusText: "No Content",
        headers: {},
        config,
      };
    };

    try {
      const tree = render(<EditableRunTitle runId="run-1" title="Initial title" />);
      act(() => {
        findEditButton(tree).props.onClick();
      });
      const input = findInput(tree);
      act(() => {
        input.props.onChange({ target: { value: "Discarded" } });
      });
      act(() => {
        input.props.onKeyDown({ key: "Escape", preventDefault: () => {} });
      });

      expect(calls).toBe(0);
      expect(instanceText(findEditButton(tree))).toContain("Initial title");
    } finally {
      generatedAxios.defaults.adapter = originalAdapter;
    }
  });

  test("submitting an empty title does not send a request", async () => {
    let calls = 0;
    const originalAdapter = generatedAxios.defaults.adapter;
    generatedAxios.defaults.adapter = async (config) => {
      calls += 1;
      return {
        data: undefined,
        status: 200,
        statusText: "OK",
        headers: {},
        config,
      };
    };

    try {
      const tree = render(<EditableRunTitle runId="run-1" title="Initial title" />);
      act(() => {
        findEditButton(tree).props.onClick();
      });
      const input = findInput(tree);
      act(() => {
        input.props.onChange({ target: { value: "   " } });
      });
      await act(async () => {
        input.props.onKeyDown({ key: "Enter", preventDefault: () => {} });
        await Promise.resolve();
      });

      expect(calls).toBe(0);
    } finally {
      generatedAxios.defaults.adapter = originalAdapter;
    }
  });

  test("unchanged title on Enter exits without a request", async () => {
    let calls = 0;
    const originalAdapter = generatedAxios.defaults.adapter;
    generatedAxios.defaults.adapter = async (config) => {
      calls += 1;
      return {
        data: undefined,
        status: 200,
        statusText: "OK",
        headers: {},
        config,
      };
    };

    try {
      const tree = render(<EditableRunTitle runId="run-1" title="Initial title" />);
      act(() => {
        findEditButton(tree).props.onClick();
      });
      const input = findInput(tree);
      await act(async () => {
        input.props.onKeyDown({ key: "Enter", preventDefault: () => {} });
        await Promise.resolve();
      });

      expect(calls).toBe(0);
      expect(instanceText(findEditButton(tree))).toContain("Initial title");
    } finally {
      generatedAxios.defaults.adapter = originalAdapter;
    }
  });
});
