export type FileCacheSide = "old" | "new";

export function stringHash(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

export function fileCacheKey({
  runId,
  toSha,
  side,
  path,
  contents,
}: {
  runId: string;
  toSha: string | null | undefined;
  side: FileCacheSide;
  path: string;
  contents: string;
}): string {
  return `fabro-run-file:${runId}:${toSha ?? "no-sha"}:${side}:${path}:${stringHash(contents)}`;
}
