import { createElement, type ReactNode } from "react";
import type { EventEnvelope } from "@qltysh/fabro-api-client";
import TestRenderer, { act } from "react-test-renderer";

import type { Stage } from "./stage-sidebar";
import { makeBilledTokenCounts } from "./test-fixtures";

const IS_REACT_ACT_ENV = "IS_REACT_ACT_ENVIRONMENT" as const;

/**
 * Per-test setup for code that uses react-test-renderer:
 * - Sets IS_REACT_ACT_ENVIRONMENT (required by act()).
 * - Silences react-test-renderer's deprecation warning.
 *
 * Returns a teardown function; pair with beforeEach/afterEach so the global
 * state is scoped to the test rather than leaking process-wide.
 */
export function setupReactTestEnv(): () => void {
  type Globals = { [IS_REACT_ACT_ENV]?: boolean };
  const globals = globalThis as Globals;
  const hadEnv = IS_REACT_ACT_ENV in globals;
  const previousEnv = globals[IS_REACT_ACT_ENV];
  globals[IS_REACT_ACT_ENV] = true;

  const originalConsoleError = console.error;
  console.error = ((...args: unknown[]) => {
    if (
      typeof args[0] === "string" &&
      args[0].startsWith("react-test-renderer is deprecated")
    ) {
      return;
    }
    originalConsoleError(...args);
  }) as typeof console.error;

  return () => {
    console.error = originalConsoleError;
    if (hadEnv) {
      globals[IS_REACT_ACT_ENV] = previousEnv;
    } else {
      delete globals[IS_REACT_ACT_ENV];
    }
  };
}

/** Build an event envelope fixture; override any field via `partial`. */
export function makeEventEnvelope(
  seq: number,
  partial: Partial<EventEnvelope>,
): EventEnvelope {
  return {
    seq,
    id: `evt-${seq}`,
    ts: `2026-04-09T12:00:0${seq}Z`,
    run_id: "run-1",
    event: "stage.prompt",
    ...partial,
  } as EventEnvelope;
}

/** Flatten a rendered subtree to its visible text. */
export function textContent(node: TestRenderer.ReactTestInstance): string {
  return node.children
    .map((child) => (typeof child === "string" ? child : textContent(child)))
    .join("");
}

/**
 * Build a sidebar `Stage` fixture; override any field via `overrides`. Kept
 * here so widening `Stage` updates every fixture at once — test files are
 * excluded from typecheck, so a per-file copy silently goes stale instead.
 */
export function makeStage(overrides: Partial<Stage> = {}): Stage {
  return {
    id: "implement@1",
    name: "implement",
    handler: "agent",
    nodeId: "implement",
    visit: 1,
    graphVisit: null,
    resumedFromStageId: null,
    parallelGroupId: null,
    parallelBranchIndex: null,
    status: "running",
    duration: "--",
    startedAt: null,
    providerUsed: null,
    billing: makeBilledTokenCounts(),
    ...overrides,
  };
}

export function renderHook<T>(
  hook: () => T,
  options: { wrapper: React.ComponentType<{ children: ReactNode }> },
): { result: { current: T } } {
  const result = { current: undefined as unknown as T };
  function HookHost() {
    result.current = hook();
    return null;
  }
  act(() => {
    TestRenderer.create(
      createElement(options.wrapper, null, createElement(HookHost)),
    );
  });
  return { result };
}
