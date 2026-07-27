import type { RunArtifactEntry } from "@qltysh/fabro-api-client";

import { isVisibleStage } from "../../data/runs";
import type { Stage } from "../../lib/stage-sidebar";
import { formatStageLabel } from "../../lib/stage-sidebar";

/** One capture of a file, written by a single stage attempt. */
export interface ArtifactVersion {
  stageId: string;
  stageLabel: string;
  retry: number;
  size: number;
  /** Byte change this capture introduced; null for the first capture. */
  delta: number | null;
}

/** One artifact path together with its capture history, newest first. */
export interface ArtifactFile {
  path: string;
  /** Directory prefix including the trailing slash, or "" at the root. */
  dir: string;
  name: string;
  versions: ArtifactVersion[];
  latest: ArtifactVersion;
}

export function splitArtifactPath(path: string): { dir: string; name: string } {
  const idx = path.lastIndexOf("/");
  return idx >= 0
    ? { dir: path.slice(0, idx + 1), name: path.slice(idx + 1) }
    : { dir: "", name: path };
}

interface StageInfo {
  label: string;
  order: number;
}

/**
 * Chronological position of each stage, keyed by stage ID.
 *
 * Stage IDs are `node@visit`, where `visit` counts visits to that one node — it
 * is not a run-wide ordinal, so it cannot order stages against each other.
 * `startedAt` is the only run-wide clock available here.
 */
function stageInfoById(stages: readonly Stage[]): Map<string, StageInfo> {
  const chronological = stages
    .map((stage, index) => ({ stage, index }))
    .sort((a, b) => {
      const at = a.stage.startedAt;
      const bt = b.stage.startedAt;
      // Stages that have not started yet sort last but keep a stable order.
      if (at === null && bt === null) return a.index - b.index;
      if (at === null) return 1;
      if (bt === null) return -1;
      const cmp = at.localeCompare(bt);
      return cmp !== 0 ? cmp : a.index - b.index;
    });

  const info = new Map<string, StageInfo>();
  chronological.forEach(({ stage }, order) => {
    info.set(stage.id, { label: formatStageLabel(stage), order });
  });
  return info;
}

/**
 * Collapse raw `(stage, retry, path)` capture keys into one entry per file,
 * carrying the ordered history of every capture of that path.
 *
 * Captures from graph control nodes (`start`, `exit`) are dropped: those nodes
 * run no work, so anything they match is a pre-existing workspace file rather
 * than something the run produced.
 */
export function groupArtifactsByFile(
  entries: readonly RunArtifactEntry[],
  stages: readonly Stage[],
): ArtifactFile[] {
  const stageInfo = stageInfoById(stages);
  const byPath = new Map<string, Array<ArtifactVersion & { order: number }>>();

  for (const entry of entries) {
    if (!isVisibleStage(entry.node_slug)) continue;

    // Until the stages request resolves there is no clock to order by, so
    // unresolved stages fall back to a stable sort on stage ID.
    const info = stageInfo.get(entry.stage_id);
    const version: ArtifactVersion & { order: number } = {
      stageId: entry.stage_id,
      stageLabel: info?.label ?? entry.node_slug,
      retry: entry.retry,
      size: entry.size,
      delta: null,
      order: info?.order ?? Number.MAX_SAFE_INTEGER,
    };
    const bucket = byPath.get(entry.relative_path);
    if (bucket) bucket.push(version);
    else byPath.set(entry.relative_path, [version]);
  }

  const files: Array<{ file: ArtifactFile; order: number }> = [];
  for (const [path, versions] of byPath) {
    // Oldest first, so each version's delta is the change that capture introduced.
    versions.sort(
      (a, b) =>
        a.order - b.order || a.retry - b.retry || a.stageId.localeCompare(b.stageId),
    );
    versions.forEach((version, index) => {
      version.delta = index === 0 ? null : version.size - versions[index - 1].size;
    });

    const newestFirst = versions.slice().reverse();
    const latest = newestFirst[0];
    const { dir, name } = splitArtifactPath(path);
    files.push({
      file: { path, dir, name, versions: newestFirst, latest },
      order: latest.order,
    });
  }

  // Most recently written file first — the page answers "what just happened?".
  files.sort((a, b) => b.order - a.order || a.file.path.localeCompare(b.file.path));
  return files.map((entry) => entry.file);
}
