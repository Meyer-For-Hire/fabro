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
  versions: readonly [ArtifactVersion, ...ArtifactVersion[]];
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

/** Stage display data keyed by ID, preserving the API's event order. */
function stageInfoById(stages: readonly Stage[]): Map<string, StageInfo> {
  const info = new Map<string, StageInfo>();
  stages.forEach((stage, order) => {
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
  const byPath = new Map<string, [ArtifactVersion, ...ArtifactVersion[]]>();

  for (const entry of entries) {
    if (!isVisibleStage(entry.node_slug)) continue;

    const info = stageInfo.get(entry.stage_id);
    const version: ArtifactVersion = {
      stageId: entry.stage_id,
      stageLabel: info?.label ?? entry.node_slug,
      retry: entry.retry,
      size: entry.size,
      delta: null,
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
        (stageInfo.get(a.stageId)?.order ?? -1) -
          (stageInfo.get(b.stageId)?.order ?? -1) ||
        a.retry - b.retry ||
        a.stageId.localeCompare(b.stageId),
    );
    versions.forEach((version, index) => {
      version.delta = index === 0 ? null : version.size - versions[index - 1].size;
    });

    versions.reverse();
    const latest = versions[0];
    const { dir, name } = splitArtifactPath(path);
    files.push({
      file: { path, dir, name, versions },
      order: stageInfo.get(latest.stageId)?.order ?? -1,
    });
  }

  // Most recently written file first — the page answers "what just happened?".
  files.sort((a, b) => b.order - a.order || a.file.path.localeCompare(b.file.path));
  return files.map((entry) => entry.file);
}
