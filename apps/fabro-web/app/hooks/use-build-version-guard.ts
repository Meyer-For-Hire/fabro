import { useEffect, useRef, useState } from "react";

import { useToast } from "../components/toast";
import {
  documentBuildId,
  isStaleBuild,
  useLatestBuildId,
} from "../lib/build-version";

const NEW_VERSION_MESSAGE = "A new version of Fabro is available.";

/**
 * Synchronizes a reload prompt with the build id the server is publishing.
 *
 * Client-side routing never re-fetches `index.html`, and hashed bundles are
 * served `immutable`, so a tab left open across a deploy keeps running the
 * previous build's JavaScript indefinitely with nothing to reveal it. This
 * offers a reload when that happens; it never reloads on its own.
 */
export function useBuildVersionGuard(): void {
  const { dismiss, push } = useToast();
  // Read once. It describes the document this tab loaded, which cannot change
  // without a full page load — and that remounts the hook anyway.
  const [loadedBuildId] = useState<string | null>(documentBuildId);
  const latestBuildId = useLatestBuildId();
  const promptRef = useRef<{ buildId: string; toastId: string } | null>(null);

  const stale = isStaleBuild(loadedBuildId, latestBuildId);

  useEffect(() => {
    if (!latestBuildId) return;
    if (!stale) {
      // A rollback to the document's own build makes an existing prompt false.
      if (promptRef.current) {
        dismiss(promptRef.current.toastId);
        promptRef.current = null;
      }
      return;
    }
    if (promptRef.current?.buildId === latestBuildId) return;
    if (promptRef.current) {
      dismiss(promptRef.current.toastId);
    }

    const toastId = push({
      message: NEW_VERSION_MESSAGE,
      // Persistent by design: a prompt that vanishes after a few seconds is one
      // the user will miss, which is the whole failure this exists to fix.
      autoDismissMs: Infinity,
      action: {
        label: "Reload",
        onClick: () => window.location.reload(),
      },
    });
    promptRef.current = { buildId: latestBuildId, toastId };
  }, [dismiss, latestBuildId, push, stale]);
}
