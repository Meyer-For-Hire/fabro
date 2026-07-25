import useSWR from "swr";

/** Where `scripts/build.ts` publishes the id of the build being served. */
const BUILD_ID_URL = "/build-id.json";

/**
 * How often a visible tab re-checks. SWR does not poll while the document is
 * hidden (`refreshWhenHidden` defaults to false), so background tabs stay
 * silent without any extra gating, and a hidden tab revalidates on focus.
 */
const POLL_INTERVAL_MS = 60_000;

export const BUILD_ID_META_NAME = "fabro-build-id";

/**
 * The build this document loaded, from the meta tag `scripts/build.ts` writes
 * into `index.html`.
 *
 * The meta tag is the honest source for "what is this tab running": client-side
 * routing never re-fetches `index.html`, so it stays pinned to the build the
 * tab actually started with, however long the tab lives.
 */
export function documentBuildId(): string | null {
  if (typeof document === "undefined") return null;
  const content = document
    .querySelector(`meta[name="${BUILD_ID_META_NAME}"]`)
    ?.getAttribute("content")
    ?.trim();
  return content ? content : null;
}

export async function fetchBuildId(url: string): Promise<string | null> {
  // Served `no-cache` with an ETag, so the browser revalidates and normally
  // gets a 304 rather than a fresh body.
  const response = await fetch(url);
  if (!response.ok) return null;
  const body: unknown = await response.json();
  const buildId = (body as { buildId?: unknown } | null)?.buildId;
  return typeof buildId === "string" && buildId ? buildId : null;
}

/**
 * True only when the running document is provably behind what the server is
 * serving now.
 *
 * Unknown on either side means "claim nothing". A missing meta tag (a build
 * predating this feature, or a non-DOM test environment) or a failed fetch must
 * never produce a reload prompt — a false "new version" claim is worse than a
 * missed one, because it teaches people to ignore the real ones.
 */
export function isStaleBuild(
  loaded: string | null,
  latest: string | null,
): boolean {
  if (!loaded || !latest) return false;
  return loaded !== latest;
}

/** Synchronizes React with the build id the server is currently publishing. */
export function useLatestBuildId(): string | null {
  const { data } = useSWR(BUILD_ID_URL, fetchBuildId, {
    refreshInterval: POLL_INTERVAL_MS,
    revalidateOnFocus: true,
    // A failed check is not worth retry storms; the next poll covers it.
    shouldRetryOnError: false,
  });
  return data ?? null;
}
