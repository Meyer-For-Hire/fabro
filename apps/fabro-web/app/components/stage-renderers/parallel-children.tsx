import { useMemo } from "react";
import { Link } from "react-router";
import { ArrowTopRightOnSquareIcon } from "@heroicons/react/20/solid";
import { StageState } from "@qltysh/fabro-api-client";
import type { EventEnvelope } from "@qltysh/fabro-api-client";

import type { Stage } from "../stage-sidebar";
import { stageStatusLabel, stageStatusTone } from "../../lib/stage-sidebar";
import { formatDurationMs } from "../../lib/format";
import { StageMetaBar } from "./meta-bar";
import { parseParallelOverview } from "./helpers";
import type { ParallelBranchSummary } from "./helpers";

/** Branch row view state: completed outcomes plus a synthesized in-flight row. */
interface BranchRow extends Omit<ParallelBranchSummary, "status"> {
  status: StageState;
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
      <span className={`font-mono text-xl tabular-nums ${toneClass}`}>{value}</span>
    </div>
  );
}

function ChildRow({
  result,
  stageHref,
}: {
  result: BranchRow;
  stageHref: string | null;
}) {
  const tone = stageStatusTone(result.status);

  const inner = (
    <>
      <span
        className={`inline-flex w-24 shrink-0 justify-center rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider ${tone}`}
      >
        {stageStatusLabel(result.status)}
      </span>
      <span className="min-w-0 flex flex-1 items-baseline gap-2">
        <span className="truncate font-mono text-sm text-fg-3">
          {result.itemLabel ?? result.id}
        </span>
        {result.itemLabel && (
          <span className="shrink-0 font-mono text-[11px] text-fg-muted">
            {result.id}
          </span>
        )}
      </span>
      {stageHref && (
        <ArrowTopRightOnSquareIcon
          className="size-3.5 shrink-0 text-fg-muted transition-colors group-hover:text-fg-2"
          aria-hidden="true"
        />
      )}
    </>
  );

  return (
    <li className="flex items-center gap-3 px-4 py-2.5">
      {stageHref ? (
        <Link
          to={stageHref}
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

  // Map node_id -> latest stage_id so we can deep-link branches.
  const latestStageByNode = useMemo(() => {
    const latest = new Map<string, Stage>();
    for (const s of allStages) {
      const prev = latest.get(s.nodeId);
      if (!prev || s.visit > prev.visit) latest.set(s.nodeId, s);
    }
    return new Map(Array.from(latest.entries()).map(([nodeId, s]) => [nodeId, s.id]));
  }, [allStages]);

  const resultCountByNode = useMemo(() => {
    const counts = new Map<string, number>();
    for (const result of overview.results) {
      counts.set(result.id, (counts.get(result.id) ?? 0) + 1);
    }
    return counts;
  }, [overview.results]);

  const items: BranchRow[] = overview.results.length > 0
    ? overview.results
    : overview.branchCount && overview.branchCount > 0
      ? Array.from({ length: overview.branchCount }, (_, i) => ({
          id: `branch ${i + 1}`,
          index: i,
          itemLabel: null,
          status: StageState.RUNNING,
        }))
      : [];

  return (
    <div className="space-y-6 pl-3 pr-4 sm:pr-6 lg:pr-8">
      <StageMetaBar stage={stage} />

      <section className="grid grid-cols-2 gap-x-6 gap-y-4 rounded-lg bg-panel p-5 outline-1 -outline-offset-1 outline-line sm:grid-cols-4">
        <StatItem label="Branches" value={overview.branchCount ?? "—"} />
        <StatItem
          label="Succeeded"
          value={overview.successCount ?? (overview.isComplete ? 0 : "—")}
          tone="success"
        />
        <StatItem
          label="Failed"
          value={overview.failureCount ?? (overview.isComplete ? 0 : "—")}
          tone={overview.failureCount && overview.failureCount > 0 ? "danger" : "default"}
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
        {items.length === 0 ? (
          <p className="text-sm text-fg-muted">No branches recorded yet.</p>
        ) : (
          <ul className="divide-y divide-line rounded-lg bg-panel outline-1 -outline-offset-1 outline-line">
            {items.map((result, i) => {
              // A node id alone cannot identify one dynamic item when several
              // results share the template target. Avoid linking every row to
              // whichever execution happened to finish last.
              const stageId = resultCountByNode.get(result.id) === 1
                ? latestStageByNode.get(result.id)
                : null;
              const href = stageId ? `/runs/${runId}/stages/${stageId}` : null;
              return (
                <ChildRow
                  key={`${result.id}-${result.index ?? i}`}
                  result={result}
                  stageHref={href}
                />
              );
            })}
          </ul>
        )}
      </section>
    </div>
  );
}
