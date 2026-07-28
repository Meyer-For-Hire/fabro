import { useMemo } from "react";
import { Link } from "react-router";
import { ArrowTopRightOnSquareIcon } from "@heroicons/react/20/solid";
import { StageState } from "@qltysh/fabro-api-client";
import type { EventEnvelope } from "@qltysh/fabro-api-client";

import type { Stage } from "../stage-sidebar";
import { formatStageLabel, stageStatusLabel, stageStatusTone } from "../../lib/stage-sidebar";
import { formatDurationMs } from "../../lib/format";
import { StageMetaBar } from "./meta-bar";
import { parseParallelOverview } from "./helpers";

/** Branch row view state sourced from a live branch stage or completed result. */
interface BranchRow {
  branchIndex: number;
  id: string;
  status: StageState;
  stageHref: string | null;
}

function StatItem({
  label,
  value,
  tone = "default",
}: {
  label: string;
  value: string | number;
  tone?: "default" | "success" | "danger";
}) {
  const toneClass =
    tone === "success" ? "text-mint" : tone === "danger" ? "text-coral" : "text-fg";
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10px] font-medium uppercase tracking-[0.16em] text-fg-muted">
        {label}
      </span>
      <span data-stat={label} className={`font-mono text-xl tabular-nums ${toneClass}`}>
        {value}
      </span>
    </div>
  );
}

function ChildRow({
  row,
}: {
  row: BranchRow;
}) {
  const tone = stageStatusTone(row.status);

  const inner = (
    <>
      <span
        className={`inline-flex w-24 shrink-0 justify-center rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider ${tone}`}
      >
        {stageStatusLabel(row.status)}
      </span>
      <span className="min-w-0 flex-1 truncate font-mono text-sm text-fg-3">
        {row.id}
      </span>
      {row.stageHref && (
        <ArrowTopRightOnSquareIcon
          className="size-3.5 shrink-0 text-fg-muted transition-colors group-hover:text-fg-2"
          aria-hidden="true"
        />
      )}
    </>
  );

  return (
    <li className="flex items-center gap-3 px-4 py-2.5">
      {row.stageHref ? (
        <Link
          to={row.stageHref}
          className="group flex flex-1 items-center gap-3 rounded -m-1 p-1 transition-colors hover:bg-overlay focus-visible:bg-overlay focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-teal-500"
        >
          {inner}
        </Link>
      ) : (
        <span className="flex flex-1 items-center gap-3">{inner}</span>
      )}
    </li>
  );
}

export function ParallelChildren({
  stage,
  events,
  runId,
  allStages,
}: {
  stage: Stage;
  events: EventEnvelope[];
  runId: string;
  allStages: Stage[];
}) {
  const overview = useMemo(() => parseParallelOverview(events), [events]);

  const stagesByBranchIndex = useMemo(() => {
    const byIndex = new Map<number, Stage>();
    for (const candidate of allStages) {
      if (
        candidate.parallelGroupId === stage.id
        && candidate.parallelBranchIndex != null
      ) {
        byIndex.set(candidate.parallelBranchIndex, candidate);
      }
    }
    return byIndex;
  }, [allStages, stage.id]);

  const branchCount = overview.branchCount ?? stagesByBranchIndex.size;
  const rows = Array.from({ length: branchCount }, (_, index): BranchRow => {
    // A live branch stage is the freshest source; fall back to the completed
    // event's result for runs whose branches predate parallel identity.
    const branchStage = stagesByBranchIndex.get(index);
    if (branchStage) {
      return {
        branchIndex: index,
        id: formatStageLabel(branchStage),
        status: branchStage.status,
        stageHref: `/runs/${runId}/stages/${branchStage.id}`,
      };
    }
    const result = overview.results[index];
    if (result) {
      return {
        branchIndex: index,
        id: result.id,
        status: result.status,
        stageHref: null,
      };
    }
    return {
      branchIndex: index,
      id: `branch ${index + 1}`,
      status: StageState.PENDING,
      stageHref: null,
    };
  });

  let liveSuccessCount = 0;
  let liveFailureCount = 0;
  for (const branchStage of stagesByBranchIndex.values()) {
    if (branchStage.status === StageState.SUCCEEDED) liveSuccessCount += 1;
    else if (branchStage.status === StageState.FAILED) liveFailureCount += 1;
  }
  const successCount = overview.isComplete
    ? overview.successCount ?? 0
    : liveSuccessCount;
  const failureCount = overview.isComplete
    ? overview.failureCount ?? 0
    : liveFailureCount;

  return (
    <div className="space-y-6 pl-3 pr-4 sm:pr-6 lg:pr-8">
      <StageMetaBar stage={stage} />

      <section className="grid grid-cols-2 gap-x-6 gap-y-4 rounded-lg bg-panel p-5 outline-1 -outline-offset-1 outline-line sm:grid-cols-4">
        <StatItem label="Branches" value={overview.branchCount ?? "—"} />
        <StatItem
          label="Succeeded"
          value={successCount}
          tone="success"
        />
        <StatItem
          label="Failed"
          value={failureCount}
          tone={failureCount > 0 ? "danger" : "default"}
        />
        <StatItem
          label="Duration"
          value={overview.durationMs != null ? formatDurationMs(overview.durationMs) : overview.isComplete ? "—" : "running"}
        />
      </section>

      <section>
        <h3 className="mb-2 text-xs font-medium uppercase tracking-wider text-fg-muted">
          Branches
        </h3>
        {rows.length === 0 ? (
          <p className="text-sm text-fg-muted">No branches recorded yet.</p>
        ) : (
          <ul className="divide-y divide-line rounded-lg bg-panel outline-1 -outline-offset-1 outline-line">
            {rows.map((row) => (
              <ChildRow key={row.branchIndex} row={row} />
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
