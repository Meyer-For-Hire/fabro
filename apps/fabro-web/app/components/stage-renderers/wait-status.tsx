import { PauseCircleIcon } from "@heroicons/react/20/solid";

import type { Stage } from "../stage-sidebar";
import { ACTIVE_STAGE_STATES } from "../../lib/stage-sidebar";
import { useTickingNow } from "../../lib/time";
import { StageMetaBar } from "./meta-bar";

function formatHms(startedAt: string | null): string {
  if (!startedAt) return "—";
  const startMs = Date.parse(startedAt);
  if (Number.isNaN(startMs)) return "—";
  const totalSecs = Math.max(0, Math.floor((Date.now() - startMs) / 1000));
  const hours = Math.floor(totalSecs / 3600);
  const minutes = Math.floor((totalSecs % 3600) / 60);
  const seconds = totalSecs % 60;
  return `${hours.toString().padStart(2, "0")}:${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
}

export function WaitStatus({ stage }: { stage: Stage }) {
  const isActive = ACTIVE_STAGE_STATES.has(stage.status);
  // Tick every second while waiting so the clock keeps moving.
  useTickingNow(isActive);
  const elapsed = isActive ? formatHms(stage.startedAt) : stage.duration;
  const label = isActive ? "Waiting" : "Wait complete";

  return (
    <div className="space-y-6 pl-3 pr-4 sm:pr-6 lg:pr-8">
      <StageMetaBar stage={stage} />

      <section className="flex items-center justify-center rounded-lg bg-panel py-12 outline-1 -outline-offset-1 outline-line">
        <div className="flex flex-col items-center gap-4">
          <div className="flex items-center gap-2 text-xs font-medium uppercase tracking-[0.18em] text-fg-muted">
            {isActive && (
              <PauseCircleIcon className="size-3.5 text-amber" aria-hidden="true" />
            )}
            {label}
          </div>
          <div
            className={`font-mono text-5xl tabular-nums ${
              isActive ? "text-fg" : "text-fg-2"
            }`}
          >
            {elapsed}
          </div>
        </div>
      </section>
    </div>
  );
}
