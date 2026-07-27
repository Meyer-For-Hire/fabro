import { useMemo, useState } from "react";
import { useParams } from "react-router";
import { ArrowDownTrayIcon, ChevronRightIcon, PaperClipIcon } from "@heroicons/react/24/outline";

import { EmptyState, ErrorState, LoadingState } from "../components/state";
import { StageSidebar } from "../components/stage-sidebar";
import { stageArtifactDownloadUrl } from "../lib/api-client";
import { formatBytes } from "../lib/format";
import { useRunArtifacts, useRunStages } from "../lib/queries";
import { mapRunStagesToSidebarStages } from "../lib/stage-sidebar";
import type { ArtifactFile, ArtifactVersion } from "./run-artifacts/group";
import { groupArtifactsByFile } from "./run-artifacts/group";

export const handle = { wide: true };

export default function RunArtifacts() {
  const { id } = useParams();
  const stagesQuery = useRunStages(id);
  const artifactsQuery = useRunArtifacts(id);
  const stages = useMemo(
    () => mapRunStagesToSidebarStages(stagesQuery.data),
    [stagesQuery.data],
  );

  return (
    <div className="flex gap-6">
      <StageSidebar stages={stages} runId={id!} activeLink="artifacts" />
      <div className="min-w-0 flex-1">
        <RunArtifactsBody runId={id!} artifactsQuery={artifactsQuery} stages={stages} />
      </div>
    </div>
  );
}

function RunArtifactsBody({
  runId,
  artifactsQuery,
  stages,
}: {
  runId: string;
  artifactsQuery: ReturnType<typeof useRunArtifacts>;
  stages: ReturnType<typeof mapRunStagesToSidebarStages>;
}) {
  const entries = artifactsQuery.data?.data ?? [];
  const files = useMemo(() => groupArtifactsByFile(entries, stages), [entries, stages]);

  if (artifactsQuery.error) {
    return (
      <ErrorState
        title="Couldn't load artifacts"
        description={errorMessage(artifactsQuery.error)}
        onRetry={() => void artifactsQuery.mutate()}
      />
    );
  }
  if (artifactsQuery.data === undefined) {
    return <LoadingState label="Loading artifacts…" />;
  }
  if (files.length === 0) {
    return (
      <EmptyState
        icon={PaperClipIcon}
        title="No artifacts captured"
        description="No stage in this run produced any artifacts."
      />
    );
  }
  return <ArtifactList runId={runId} files={files} />;
}

function ArtifactList({ runId, files }: { runId: string; files: readonly ArtifactFile[] }) {
  const { captures, latestBytes, storedBytes } = useMemo(() => {
    let captures = 0;
    let latestBytes = 0;
    let storedBytes = 0;
    for (const file of files) {
      captures += file.versions.length;
      latestBytes += file.latest.size;
      for (const version of file.versions) storedBytes += version.size;
    }
    return { captures, latestBytes, storedBytes };
  }, [files]);

  // Only mention versions once some file actually has more than one.
  const versioned = captures > files.length;

  return (
    <div className="space-y-4">
      <div className="flex items-baseline justify-between gap-4">
        <h2 className="text-sm font-medium text-fg">
          {files.length} {files.length === 1 ? "file" : "files"}
          {versioned && (
            <span className="font-normal text-fg-muted"> · {captures} versions</span>
          )}
        </h2>
        <span className="text-xs tabular-nums text-fg-muted">
          {versioned
            ? `${formatBytes(latestBytes)} latest · ${formatBytes(storedBytes)} stored`
            : `${formatBytes(latestBytes)} total`}
        </span>
      </div>

      <section className="overflow-hidden rounded-md border border-line bg-panel-alt">
        {files.map((file) => (
          <ArtifactFileRow key={file.path} runId={runId} file={file} />
        ))}
      </section>
    </div>
  );
}

function ArtifactFileRow({ runId, file }: { runId: string; file: ArtifactFile }) {
  const [expanded, setExpanded] = useState(false);
  const earlier = file.versions.slice(1);

  return (
    <div className="border-t border-line first:border-t-0">
      <div className="flex items-center gap-4 px-4 py-2.5">
        {earlier.length > 0 ? (
          <button
            type="button"
            aria-expanded={expanded}
            onClick={() => setExpanded((prev) => !prev)}
            className="shrink-0 rounded-md p-1 text-fg-3 transition-colors hover:bg-overlay hover:text-fg-2 focus-visible:outline-2 focus-visible:-outline-offset-1 focus-visible:outline-teal-500"
          >
            <span className="sr-only">Show earlier versions of {file.name}</span>
            <ChevronRightIcon
              className={`size-3.5 transition-transform ${expanded ? "rotate-90" : ""}`}
              aria-hidden="true"
            />
          </button>
        ) : (
          <span className="size-5 shrink-0" aria-hidden="true" />
        )}

        <span className="flex min-w-0 flex-1 font-mono text-xs" title={file.path}>
          <span className="shrink-0 text-fg-muted">{file.dir}</span>
          <span className="truncate text-fg-2">{file.name}</span>
        </span>

        {earlier.length > 0 && (
          <span className="shrink-0 rounded-full bg-overlay-strong px-2 py-0.5 text-[11px] text-fg-3">
            {file.versions.length} versions
          </span>
        )}

        <span className="shrink-0 text-xs text-fg-3">{file.latest.stageLabel}</span>
        <span className="shrink-0 text-xs tabular-nums text-fg-muted">
          {formatBytes(file.latest.size)}
        </span>
        <DownloadLink runId={runId} path={file.path} version={file.latest} />
      </div>

      {expanded && earlier.length > 0 && (
        <ul className="border-t border-line bg-black/15 py-1">
          {earlier.map((version) => (
            <li
              key={`${version.stageId}#${version.retry}`}
              className="flex items-center gap-4 py-1.5 pl-14 pr-4 hover:bg-overlay"
            >
              <span className="min-w-0 flex-1 truncate text-xs text-fg-3">
                {version.stageLabel}
                {version.retry > 1 && (
                  <span className="ml-2 text-fg-muted">attempt {version.retry}</span>
                )}
              </span>
              <span className="shrink-0 text-xs tabular-nums text-fg-muted">
                {formatBytes(version.size)}
              </span>
              <SizeDelta delta={version.delta} />
              <DownloadLink runId={runId} path={file.path} version={version} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function SizeDelta({ delta }: { delta: number | null }) {
  if (delta === null) {
    return <span className="shrink-0 text-[11px] tabular-nums text-fg-muted">first</span>;
  }
  const tone = delta < 0 ? "text-amber" : "text-mint";
  const sign = delta < 0 ? "−" : "+";
  return (
    <span className={`shrink-0 text-[11px] tabular-nums ${tone}`}>
      {sign}
      {formatBytes(Math.abs(delta))}
    </span>
  );
}

function DownloadLink({
  runId,
  path,
  version,
}: {
  runId: string;
  path: string;
  version: ArtifactVersion;
}) {
  const href = stageArtifactDownloadUrl(runId, version.stageId, path, version.retry);
  return (
    <a
      href={href}
      download={basename(path)}
      className="inline-flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-xs text-fg-3 transition-colors hover:bg-overlay hover:text-fg focus-visible:outline-2 focus-visible:-outline-offset-1 focus-visible:outline-teal-500"
    >
      <ArrowDownTrayIcon className="size-3.5" aria-hidden="true" />
      Download
    </a>
  );
}

function basename(path: string): string {
  const idx = path.lastIndexOf("/");
  return idx >= 0 ? path.slice(idx + 1) : path;
}

function errorMessage(error: unknown): string | undefined {
  return error instanceof Error ? error.message : undefined;
}
