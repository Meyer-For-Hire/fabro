import { afterEach, describe, expect, test } from "bun:test";

import { importChunk } from "./import-chunk";

interface WindowStub {
  reloads: number;
  store: Map<string, string>;
  restore: () => void;
}

/**
 * bun:test runs without a DOM, so install a `window` carrying just the surface
 * `importChunk` touches. Follows the descriptor save/restore pattern used by
 * stage-insights-sidebar.test.tsx so other test files can install their own.
 */
function installWindow({ throwOnStorage = false } = {}): WindowStub {
  const store = new Map<string, string>();
  const stub: WindowStub = {
    reloads: 0,
    store,
    restore: () => undefined,
  };

  const windowStub = {
    sessionStorage: {
      getItem: (key: string) => {
        if (throwOnStorage) throw new Error("storage disabled");
        return store.get(key) ?? null;
      },
      setItem: (key: string, value: string) => {
        if (throwOnStorage) throw new Error("storage disabled");
        store.set(key, value);
      },
    },
    location: {
      reload: () => {
        stub.reloads += 1;
      },
    },
  };

  const had = "window" in globalThis;
  const prev = (globalThis as { window?: unknown }).window;
  Object.defineProperty(globalThis, "window", {
    value:        windowStub,
    writable:     true,
    configurable: true,
  });
  stub.restore = () => {
    if (had) {
      Object.defineProperty(globalThis, "window", {
        value:        prev,
        writable:     true,
        configurable: true,
      });
    } else {
      delete (globalThis as { window?: unknown }).window;
    }
  };
  return stub;
}

let installed: WindowStub | null = null;
afterEach(() => {
  installed?.restore();
  installed = null;
});

describe("importChunk", () => {
  test("passes a successful import through untouched", async () => {
    installed = installWindow();
    await expect(importChunk(async () => "loaded")).resolves.toBe("loaded");
    expect(installed.reloads).toBe(0);
  });

  test("reloads once and rethrows when a chunk fails to load", async () => {
    installed = installWindow();
    const failure = new Error("Failed to fetch dynamically imported module");

    await expect(
      importChunk(async () => {
        throw failure;
      }),
    ).rejects.toThrow(failure);

    expect(installed.reloads).toBe(1);
  });

  // Without this, a chunk that fails for a reason a reload cannot fix would
  // reload forever.
  test("does not reload again for the same build", async () => {
    installed = installWindow();
    const load = async () => {
      throw new Error("Failed to fetch dynamically imported module");
    };

    await expect(importChunk(load)).rejects.toThrow();
    await expect(importChunk(load)).rejects.toThrow();
    await expect(importChunk(load)).rejects.toThrow();

    expect(installed.reloads).toBe(1);
  });

  // The marker is keyed by build id, so a tab that recovers from one deploy
  // still has a reload available for the next.
  test("keys the once-only marker by build id", async () => {
    installed = installWindow();
    await expect(
      importChunk(async () => {
        throw new Error("boom");
      }),
    ).rejects.toThrow();

    expect([...installed.store.keys()]).toEqual([
      "fabro:chunk-reload:unknown",
    ]);
  });

  // No durable marker means no way to promise "only once", and a reload loop is
  // far worse than a surfaced error.
  test("does not reload when session storage is unavailable", async () => {
    installed = installWindow({ throwOnStorage: true });

    await expect(
      importChunk(async () => {
        throw new Error("boom");
      }),
    ).rejects.toThrow();

    expect(installed.reloads).toBe(0);
  });
});
