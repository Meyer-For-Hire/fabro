import { documentBuildId } from "./build-version";

const RELOAD_MARKER_PREFIX = "fabro:chunk-reload:";

/**
 * Loads a lazily-imported chunk, reloading the page once if it cannot be
 * fetched.
 *
 * Each deploy replaces the served assets and the previous build's hashed
 * filenames stop existing, so a tab open across a deploy can request a chunk
 * that now 404s. Static route imports mean most of the graph is already in
 * memory, but the handful of genuinely lazy imports — the terminal, Graphviz
 * rendering, the file tree — are loaded on demand and can land in that window.
 *
 * A failed chunk means the feature is already broken, so reloading is recovery
 * rather than an interruption. This is the one place the app reloads without an
 * explicit click; the build-version toast never does.
 */
export function importChunk<T>(load: () => Promise<T>): Promise<T> {
  return load().catch((error: unknown) => {
    reloadOnceForStaleChunk();
    // Rethrow rather than returning a never-settling promise. The reload
    // normally replaces the document before this surfaces; if it doesn't, an
    // error boundary is a better outcome than a spinner that hangs forever.
    throw error;
  });
}

/**
 * Reloads at most once per build.
 *
 * Keyed by build id rather than a bare flag so a tab that recovers from one
 * deploy still has a reload available for the next one. Without the key, a
 * single chunk failure would disarm the backstop for the rest of the session.
 *
 * A module that loads fine but throws while evaluating is indistinguishable
 * here from a missing chunk, so it also spends the reload. The per-build key
 * bounds the cost at one wasted reload, after which the real error surfaces.
 */
function reloadOnceForStaleChunk(): void {
  if (typeof window === "undefined") return;

  const key = `${RELOAD_MARKER_PREFIX}${documentBuildId() ?? "unknown"}`;
  try {
    if (window.sessionStorage.getItem(key)) return;
    window.sessionStorage.setItem(key, "1");
  } catch {
    // Storage disabled or full. Without a durable marker we can't guarantee
    // "only once", and a reload loop is far worse than a surfaced error.
    return;
  }

  window.location.reload();
}
