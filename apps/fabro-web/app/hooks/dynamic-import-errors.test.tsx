import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import { useState } from "react";
import TestRenderer, { act } from "react-test-renderer";

import { setupReactTestEnv } from "../lib/test-utils";
import type {
  ConnectionStatus,
  TerminalConnectionError,
} from "./use-terminal-session";

const importFailure = new Error("chunk unavailable");

mock.module("../lib/import-chunk", () => ({
  importChunk: async () => {
    throw importFailure;
  },
}));

const [{ useRenderedVizDiagram }, { useTerminalSession }] = await Promise.all([
  import("./use-rendered-viz-diagram"),
  import("./use-terminal-session"),
]);

let renderer: TestRenderer.ReactTestRenderer | null = null;
let restoreReactTestEnv = () => {};

beforeEach(() => {
  restoreReactTestEnv = setupReactTestEnv();
});

afterEach(() => {
  act(() => {
    renderer?.unmount();
  });
  renderer = null;
  restoreReactTestEnv();
});

async function renderAndFlush(element: React.ReactElement) {
  await act(async () => {
    renderer = TestRenderer.create(element);
    await Promise.resolve();
    await Promise.resolve();
  });
}

const diagramContainerRef = { current: null };
const diagramSvgRef = { current: null };
const buildDot = () => "digraph {}";
let diagramError: string | null = null;

function DiagramHost() {
  diagramError = useRenderedVizDiagram({
    buildDot,
    innerRef: diagramContainerRef,
    identity: "diagram",
    svgRef: diagramSvgRef,
  });
  return null;
}

test("diagram import failures populate the hook error instead of rejecting", async () => {
  await renderAndFlush(<DiagramHost />);
  expect(diagramError).toBe("chunk unavailable");
});

const terminalElementRef = {
  current: {} as HTMLDivElement,
};
let terminalError: TerminalConnectionError | null = null;
let terminalStatus: ConnectionStatus = "closed";

function TerminalHost() {
  const [error, setError] = useState<TerminalConnectionError | null>(null);
  const [status, setStatus] = useState<ConnectionStatus>("closed");
  terminalError = error;
  terminalStatus = status;
  useTerminalSession({
    connectionKey: 0,
    runId: "run_1",
    setError,
    setStatus,
    terminalEl: terminalElementRef,
  });
  return null;
}

test("terminal import failures leave a recoverable error state", async () => {
  await renderAndFlush(<TerminalHost />);
  expect(terminalStatus).toBe("error");
  expect(terminalError).toEqual({
    message:     "Terminal initialization failed: chunk unavailable",
    recoverable: true,
  });
});
