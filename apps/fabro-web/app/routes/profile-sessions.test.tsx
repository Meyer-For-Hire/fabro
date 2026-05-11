import { afterEach, describe, expect, mock, test } from "bun:test";
import TestRenderer, { act } from "react-test-renderer";

import type {
  AuthSession,
  AuthSessionsResponse,
} from "@qltysh/fabro-api-client";

let currentResponse: AuthSessionsResponse | undefined;

const deleteAuthSessionMock = mock((_id: string) => Promise.resolve({ data: undefined }));
const mutateMock = mock((..._args: unknown[]) => Promise.resolve(undefined));

mock.module("../lib/queries", () => ({
  useAuthSessions: () => ({ data: currentResponse, error: undefined }),
}));

mock.module("../lib/api-client", () => ({
  apiData: async function apiData<T>(
    call: () => Promise<{ data: T }>,
  ): Promise<T> {
    const response = await call();
    return response.data;
  },
  authApi: {
    deleteAuthSession: (id: string) => deleteAuthSessionMock(id),
  },
  ApiError: class ApiError extends Error {
    readonly status: number;
    readonly requestId: string | null;
    readonly body: unknown;

    constructor({
      status,
      message,
      requestId,
      body,
    }: {
      status: number;
      message: string;
      requestId: string | null;
      body: unknown;
    }) {
      super(message);
      this.name = "ApiError";
      this.status = status;
      this.requestId = requestId;
      this.body = body;
    }
  },
}));

mock.module("swr", () => ({
  useSWRConfig: () => ({ mutate: mutateMock }),
}));

const { default: ProfileSessions } = await import("./profile-sessions");

const browserSession: AuthSession = {
  id:          "browser:current",
  kind:        "browser",
  current:     true,
  provider:    "github",
  login:       "alice",
  label:       "This browser",
  createdAt:   "2026-05-10T10:00:00Z",
  lastSeenAt:  "2026-05-10T12:00:00Z",
  expiresAt:   "2026-05-17T10:00:00Z",
  revocable:   false,
};

const cliSession: AuthSession = {
  id:          "cli:abcd-1234",
  kind:        "cli",
  current:     false,
  provider:    "github",
  login:       "alice",
  label:       "Fabro CLI",
  userAgent:   "fabro/0.1.0 Darwin",
  createdAt:   "2026-05-09T08:00:00Z",
  lastSeenAt:  "2026-05-10T11:30:00Z",
  expiresAt:   "2026-06-09T08:00:00Z",
  revocable:   true,
};

function textFromNode(
  node: ReturnType<TestRenderer.ReactTestRenderer["toJSON"]>,
): string {
  if (!node) return "";
  if (typeof node === "string") return node;
  if (Array.isArray(node)) return node.map(textFromNode).join(" ");
  return (node.children ?? []).map(textFromNode).join(" ");
}

function render(): TestRenderer.ReactTestRenderer {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  let renderer!: TestRenderer.ReactTestRenderer;
  act(() => {
    renderer = TestRenderer.create(<ProfileSessions />);
  });
  return renderer;
}

const mountedRenderers: TestRenderer.ReactTestRenderer[] = [];

function renderAndTrack(): TestRenderer.ReactTestRenderer {
  const renderer = render();
  mountedRenderers.push(renderer);
  return renderer;
}

afterEach(() => {
  for (const renderer of mountedRenderers.splice(0)) {
    act(() => renderer.unmount());
  }
  currentResponse = undefined;
  deleteAuthSessionMock.mockClear();
  mutateMock.mockClear();
  delete (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;
});

describe("ProfileSessions", () => {
  test("renders profile-style skeletons while loading", () => {
    currentResponse = undefined;
    const renderer = renderAndTrack();
    const text = textFromNode(renderer.toJSON());

    // Skeletons render placeholder bars only, no panel headings or content.
    expect(text).not.toContain("Browser");
    expect(text).not.toContain("CLI");
  });

  test("splits browser and CLI sessions into separate panels", () => {
    currentResponse = { sessions: [cliSession, browserSession] };
    const renderer = renderAndTrack();
    const text = textFromNode(renderer.toJSON());

    expect(text).toContain("Browser");
    expect(text).toContain("CLI");
    expect(text).toContain("Signed in");
    expect(text).toContain("Expires");
    expect(text).toContain("Last active");
    // Login, provider, user agent, and kind labels are intentionally absent.
    expect(text).not.toContain("alice");
    expect(text).not.toContain("github");
    expect(text).not.toContain("fabro/0.1.0 Darwin");
    expect(text).not.toContain("This browser");
    expect(text).not.toContain("Fabro CLI");
  });

  test("shows an empty CLI panel when no CLI sessions exist", () => {
    currentResponse = { sessions: [browserSession] };
    const renderer = renderAndTrack();
    const text = textFromNode(renderer.toJSON());

    expect(text).toContain("No CLI sessions.");
    const buttons = renderer.root.findAllByType("button");
    expect(buttons).toHaveLength(0);
  });

  test("shows a revoke button for revocable CLI sessions", () => {
    currentResponse = { sessions: [cliSession] };
    const renderer = renderAndTrack();

    const buttons = renderer.root.findAllByType("button");
    expect(buttons).toHaveLength(1);
    expect(buttons[0].props["aria-label"]).toBe("Revoke CLI session");
  });

  test("clicking revoke calls the delete endpoint and refreshes the sessions query", async () => {
    currentResponse = { sessions: [cliSession] };
    const renderer = renderAndTrack();

    const button = renderer.root.findByType("button");

    await act(async () => {
      await button.props.onClick();
    });

    expect(deleteAuthSessionMock).toHaveBeenCalledTimes(1);
    expect(deleteAuthSessionMock.mock.calls[0]?.[0]).toBe("cli:abcd-1234");

    expect(mutateMock).toHaveBeenCalledTimes(1);
    const mutateKey = mutateMock.mock.calls[0]?.[0] as readonly unknown[];
    expect(Array.isArray(mutateKey)).toBe(true);
    expect(mutateKey).toEqual(["auth", "sessions"]);
  });
});
