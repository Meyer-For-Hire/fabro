import { getString } from "./unknown";

export const BUILD_ID_FILE_NAME = "build-id.json";
export const BUILD_ID_URL = `/${BUILD_ID_FILE_NAME}`;
export const BUILD_ID_META_NAME = "fabro-build-id";
export const BUILD_ID_FIELD = "buildId";

const BUILD_ID_PATTERN = /^[a-z0-9]{8}$/;

export function parseBuildId(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const normalized = value.trim();
  return BUILD_ID_PATTERN.test(normalized) ? normalized : null;
}

export function parseBuildIdDocument(value: unknown): string | null {
  return parseBuildId(getString(value, BUILD_ID_FIELD));
}

export function buildIdDocument(buildId: string): Record<typeof BUILD_ID_FIELD, string> {
  return { [BUILD_ID_FIELD]: buildId };
}
