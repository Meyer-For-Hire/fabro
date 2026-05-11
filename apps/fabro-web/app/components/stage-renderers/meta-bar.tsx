import type { ReactNode } from "react";
import { ClockIcon } from "@heroicons/react/20/solid";

import type { Stage } from "../stage-sidebar";
import { Tooltip } from "../ui";
import { formatAbsoluteTs } from "../../lib/format";
import { ACTIVE_STAGE_STATES, stageStatusLabel, stageStatusTone } from "../../lib/stage-sidebar";
import { useTickingNow } from "../../lib/time";

function liveDuration(startedAt: string | null, fallback: string): string {
  if (!startedAt) return fallback;
  const startMs = Date.parse(startedAt);
  if (Number.isNaN(startMs)) return fallback;
  const secs = Math.max(0, Math.floor((Date.now() - startMs) / 1000));
  if (secs < 60) return `${secs}s`;
  const minutes = Math.floor(secs / 60);
  const remainSecs = secs % 60;
  if (minutes < 60) return remainSecs > 0 ? `${minutes}m ${remainSecs}s` : `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const remainMin = minutes % 60;
  return remainMin > 0 ? `${hours}h ${remainMin}m` : `${hours}h`;
}

/**
 * Compact horizontal status strip used at the top of every specialized stage
 * renderer. Shows the status pill and live-or-final duration; the timestamp
 * is exposed via tooltip on the duration to avoid stuffing the bar with
 * extra labels. The trailing slot is right-aligned for renderer-specific
 * metadata (chips, counts, model badges, etc.).
 */
export function StageMetaBar({
  stage,
  trailing,
}: {
  stage: Stage;
  trailing?: ReactNode;
}) {
  const isActive = ACTIVE_STAGE_STATES.has(stage.status);
  // Re-render every second while running so the elapsed clock keeps up.
  useTickingNow(isActive);
  const duration = isActive ? liveDuration(stage.startedAt, stage.duration) : stage.duration;

  const durationNode = (
    <span className="inline-flex items-center gap-1 font-mono tabular-nums text-fg-muted">
      <ClockIcon className="size-3.5" aria-hidden="true" />
      {duration}
    </span>
  );

  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 text-xs">
      <span
        className={`inline-flex items-center rounded-full px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wider ${stageStatusTone(stage.status)}`}
      >
        {stageStatusLabel(stage.status)}
      </span>
      {stage.startedAt ? (
        <Tooltip label={`Started ${formatAbsoluteTs(stage.startedAt)}`}>
          {durationNode}
        </Tooltip>
      ) : (
        durationNode
      )}
      {trailing && <span className="ml-auto inline-flex items-center gap-3">{trailing}</span>}
    </div>
  );
}

/**
 * Section heading used inside renderer bodies. Pairs an h3 title with optional
 * supporting text underneath.
 */
export function SectionHeader({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <header className="flex items-baseline justify-between gap-3 pb-3">
      <div>
        <h3 className="text-sm font-medium text-fg">{title}</h3>
        {description && <p className="mt-0.5 text-xs text-fg-muted">{description}</p>}
      </div>
      {action}
    </header>
  );
}
