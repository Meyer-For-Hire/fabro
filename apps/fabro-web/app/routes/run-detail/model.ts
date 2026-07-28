import {
  isRunStatus,
  mapRunToRunItem,
  runStatusDisplay,
  type Run,
} from "../../data/runs";

export function classNames(...classes: Array<string | false | null | undefined>) {
  return classes.filter(Boolean).join(" ");
}

export type RunDetailRun = ReturnType<typeof mapRunToRunItem> & {
  statusLabel: string;
  statusDot: string;
  statusText: string;
};

export function buildRunDetailRun(summary: Run): RunDetailRun {
  const item = mapRunToRunItem(summary);
  const rawStatus = summary.lifecycle.status;
  const statusKind = rawStatus.kind;
  const display = isRunStatus(statusKind)
    ? runStatusDisplay[statusKind]
    : { label: statusKind, dot: "bg-fg-muted", text: "text-fg-muted" };

  return {
    ...item,
    statusLabel: display.label,
    statusDot: display.dot,
    statusText: display.text,
  };
}
