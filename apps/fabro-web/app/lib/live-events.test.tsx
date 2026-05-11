import { describe, expect, test } from "bun:test";

import {
  createCrossTabSseCoordinator,
  type BroadcastChannelLike,
} from "./cross-tab-sse";
import { subscribeToLiveEvents, type LiveEventPayload } from "./live-events";
import type { EventSourceLike } from "./sse";

type MessageHandler = ((event: { data: string }) => void) | null;

class FakeEventSource {
  onmessage: MessageHandler = null;
  closed = false;

  emit(payload: unknown) {
    this.onmessage?.({ data: JSON.stringify(payload) });
  }

  close() {
    this.closed = true;
  }
}

class FakeBroadcastChannel implements BroadcastChannelLike {
  onmessage: ((event: { data: unknown }) => void) | null = null;

  postMessage() {}

  close() {}
}

describe("subscribeToLiveEvents", () => {
  test("coordinated mode opens /api/v1/attach and forwards every payload", async () => {
    const source = new FakeEventSource();
    const created: string[] = [];
    const seen: LiveEventPayload[] = [];
    const coordinator = createCoordinator((url) => {
      created.push(url);
      return source;
    });

    const cleanup = subscribeToLiveEvents(
      (payload) => seen.push(payload),
      () => {
        throw new Error("source should be created by coordinator");
      },
      { coordinator },
    );

    await waitFor(() => created.length === 1);

    source.emit({ id: "evt-1", event: "stage.started", run_id: "run-a" });
    source.emit({ id: "evt-2", event: "agent.message", run_id: "run-b" });

    expect(created).toEqual(["/api/v1/attach"]);
    expect(seen.map((p) => p.run_id)).toEqual(["run-a", "run-b"]);

    cleanup();
    coordinator.close();
  });

  test("fallback mode opens /api/v1/attach (not a per-run URL) and forwards payloads", () => {
    const source = new FakeEventSource();
    const created: string[] = [];
    const seen: LiveEventPayload[] = [];
    const coordinator = createFallbackCoordinator();

    const cleanup = subscribeToLiveEvents(
      (payload) => seen.push(payload),
      (url) => {
        created.push(url);
        return source;
      },
      { coordinator },
    );

    source.emit({ id: "evt-1", event: "run.completed", run_id: "run-a" });
    source.emit({ id: "evt-2", event: "run.failed", run_id: "run-b" });

    expect(created).toEqual(["/api/v1/attach"]);
    expect(seen.map((p) => p.run_id)).toEqual(["run-a", "run-b"]);

    cleanup();
    coordinator.close();
  });

  test("fallback closes the shared source on the final unsubscribe", () => {
    const source = new FakeEventSource();
    const coordinator = createFallbackCoordinator();
    const cleanup = subscribeToLiveEvents(() => {}, () => source, { coordinator });

    expect(source.closed).toBe(false);
    cleanup();
    expect(source.closed).toBe(true);

    coordinator.close();
  });
});

function createCoordinator(eventSourceFactory: (url: string) => EventSourceLike) {
  return createCrossTabSseCoordinator({
    tabId: "live-events-test",
    channelFactory: () => new FakeBroadcastChannel(),
    eventSourceFactory,
    addVisibilityChangeListener: () => () => {},
    addPagehideListener: () => () => {},
    timing: {
      heartbeatMs: 10,
      leaderStaleMs: 50,
      electionJitterMs: 0,
    },
  });
}

function createFallbackCoordinator() {
  return createCrossTabSseCoordinator({
    channelFactory: () => {
      throw new Error("BroadcastChannel unavailable");
    },
  });
}

async function waitFor(condition: () => boolean, timeoutMs = 200) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (condition()) return;
    await new Promise((resolve) => setTimeout(resolve, 2));
  }
  throw new Error("condition did not become true before timeout");
}
