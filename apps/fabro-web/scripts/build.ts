import { createHash } from "node:crypto";
import { watch as fsWatch } from "node:fs";
import {
  cp,
  lstat,
  mkdir,
  readFile,
  readdir,
  rename,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";

import {
  BUILD_ID_FILE_NAME,
  BUILD_ID_META_NAME,
  buildIdDocument,
} from "../app/lib/build-version-contract";

declare const Bun: any;

const root = new URL("..", import.meta.url);
const rootPath = Bun.fileURLToPath(root);
const buildsRootDir = join(rootPath, ".dist-builds");
const distPath = join(rootPath, "dist");
const publicDir = join(rootPath, "public");
const templatePath = join(rootPath, "index.template.html");
const watch = Bun.argv.includes("--watch");

// Locate dependencies through module resolution rather than hardcoded
// node_modules paths: where packages land on disk depends on the Bun install
// linker (hoisted puts them at the workspace root, isolated symlinks them into
// the app's node_modules), so any fixed path breaks on one of the layouts.
const pierreWorkerDir = join(dirname(Bun.resolveSync("@pierre/diffs", rootPath)), "worker");

const tailwindCliPackageJsonPath = Bun.resolveSync("@tailwindcss/cli/package.json", rootPath);
const tailwindCliBin = join(
  dirname(tailwindCliPackageJsonPath),
  JSON.parse(await readFile(tailwindCliPackageJsonPath, "utf8")).bin.tailwindcss,
);

// Names the `.dist-builds/` staging directory only. Time-ordered so builds sort
// chronologically on disk, and unique so concurrent builds never collide. This
// is deliberately NOT the id published to browsers: see `publishedBuildId`.
function newBuildDirName(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * Lowercase-alphanumeric 8-char digest, matching the `[a-z0-9]{8}` shape the
 * bundler uses for its own content hashes — and which the server's cache
 * classifier (`is_content_hashed` in `static_files.rs`) keys on to decide
 * between `immutable` and `no-cache`.
 */
function toShortId(hex: string): string {
  return BigInt(`0x${hex.slice(0, 32)}`)
    .toString(36)
    .padStart(8, "0")
    .slice(0, 8);
}

function contentHash8(content: string | Uint8Array): string {
  return toShortId(createHash("sha256").update(content).digest("hex"));
}

// Inputs outside Bun's JavaScript module graph. Tailwind scans `app/` for class
// names, `public/` is copied verbatim, and the remaining files control template
// rendering, module resolution, dependency versions, or the build itself.
const BUILD_INPUT_DIRS = ["app", "public"];
const BUILD_INPUT_FILES = [
  "index.template.html",
  "package.json",
  "scripts/build.ts",
  "tsconfig.json",
  "../../bun.lock",
];
const FILE_HASH_BATCH_SIZE = 32;

/**
 * The build id published to browsers, derived from the bundle's *source inputs*.
 *
 * The obvious implementation — hash the emitted asset filenames, which already
 * embed content hashes — does not work, because **Bun's minified identifier
 * naming is not deterministic**. Building this app twice from an unchanged tree
 * produces byte-different output roughly one run in three: same length, ~100k
 * differing bytes, all of it mangled names (`var Gr=C3((Pl5,qq)=>` in one run,
 * `var yr=C3((Uc5,Oq)=>` in the next). Output hashes therefore change without
 * any source change.
 *
 * That matters because the client shows a "new version" toast on mismatch. An id
 * that flips at random would fire the toast on redeploys of identical code and
 * train people to ignore it, which is worse than having no toast at all. Hashing
 * the inputs makes the id change if and only if something we actually control
 * changed.
 *
 * The tradeoff: when Bun emits a different permutation for the same source, the
 * asset filenames change while the build id does not, so an open tab isn't told
 * to reload. That is the correct call — the two builds are the same program —
 * and `importChunk` covers the case where such a tab later needs a chunk whose
 * name moved.
 */
async function publishedBuildId(bundlerInputs: Iterable<string>): Promise<string> {
  const files = new Set<string>();
  for (const dir of BUILD_INPUT_DIRS) {
    for (const file of await collectFiles(join(rootPath, dir))) {
      files.add(file);
    }
  }
  for (const file of BUILD_INPUT_FILES) {
    files.add(resolve(rootPath, file));
  }
  // Bun's metafile is the source of truth for resolved production modules. In
  // particular, it captures workspace sources reached through tsconfig path
  // aliases, which a hand-maintained app-local file list would miss.
  for (const file of localBundlerInputPaths(bundlerInputs)) {
    files.add(file);
  }

  const digest = createHash("sha256");
  // The bundler itself is an input: a Bun upgrade can change output semantics.
  digest.update(`bun:${Bun.version}\n`);
  const inputs = [...files]
    .map((file) => ({
      file,
      name: toUrlPath(relative(rootPath, file)),
    }))
    .sort((left, right) =>
      left.name < right.name ? -1 : left.name > right.name ? 1 : 0
    );

  // Bound parallel reads so hashing stays off the rebuild critical path without
  // exhausting low per-process file-descriptor limits on macOS.
  for (let start = 0; start < inputs.length; start += FILE_HASH_BATCH_SIZE) {
    const batch = inputs.slice(start, start + FILE_HASH_BATCH_SIZE);
    const hashes = await Promise.all(
      batch.map(async ({ file, name }) => ({
        name,
        hash: createHash("sha256").update(await readFile(file)).digest(),
      })),
    );
    for (const { name, hash } of hashes) {
      // Hash the repo-relative path, not the absolute one, so the id doesn't
      // depend on where the repo is checked out.
      digest.update(name);
      digest.update("\0");
      digest.update(hash);
    }
  }
  return toShortId(digest.digest("hex"));
}

async function collectFiles(dir: string): Promise<string[]> {
  const glob = new Bun.Glob("**/*");
  const files: string[] = [];
  for await (const file of glob.scan({ cwd: dir, dot: true, onlyFiles: true })) {
    files.push(join(dir, file));
  }
  return files;
}

export function localBundlerInputPaths(inputs: Iterable<string>): string[] {
  return [...inputs]
    .map((input) => resolve(rootPath, input))
    .filter((path) => !isInstalledDependency(path));
}

function isInstalledDependency(path: string): boolean {
  return toUrlPath(relative(rootPath, path))
    .split("/")
    .includes("node_modules");
}

function toUrlPath(path: string): string {
  return path.split(sep).join("/");
}

function assetHref(path: string): string {
  return `/${toUrlPath(path)}`;
}

async function buildOnce() {
  const buildDirName = newBuildDirName();
  const buildDir = join(buildsRootDir, buildDirName);
  const buildAssetsDir = join(buildDir, "assets");
  await mkdir(buildAssetsDir, { recursive: true });

  const [result, cssResult] = await Promise.all([
    Bun.build({
      entrypoints: [join(rootPath, "app", "entry.tsx")],
      outdir: buildAssetsDir,
      naming: "[name]-[hash].[ext]",
      minify: true,
      splitting: true,
      target: "browser",
      metafile: true,
      root: rootPath,
    }),
    Bun.spawn([
      process.execPath,
      tailwindCliBin,
      "-i",
      "app/app.css",
      "-o",
      relative(rootPath, join(buildAssetsDir, "app.css")),
      "--minify",
    ], {
      cwd: rootPath,
      stdout: "inherit",
      stderr: "inherit",
    }).exited,
  ]);

  if (cssResult !== 0) {
    throw new Error("Tailwind build failed");
  }

  if (!result.success) {
    throw new Error(result.logs.map((log: any) => log.message).join("\n"));
  }

  const [stylesheetPath, buildId] = await Promise.all([
    hashStylesheet(buildAssetsDir),
    publishedBuildId(Object.keys(result.metafile.inputs)),
    cp(publicDir, buildDir, { recursive: true }),
    copyPierreWorkerAssets(join(buildAssetsDir, "pierre-diffs-worker")),
  ]);

  const outputs: IndexHtmlOutput[] = [
    { kind: "asset", path: stylesheetPath },
    ...result.outputs.map((output: any) => ({
      kind: output.kind,
      path: relative(buildDir, output.path),
    })),
  ];

  await writeIndexHtml(buildDir, outputs, buildId);
  // Served with `no-cache` + ETag (it doesn't match the server's content-hash
  // pattern), so a polling client revalidates it as a cheap 304.
  await writeFile(
    join(buildDir, BUILD_ID_FILE_NAME),
    `${JSON.stringify(buildIdDocument(buildId), null, 2)}\n`,
    "utf8",
  );

  await publishBuild(buildDir);
  await pruneOldBuilds(buildDirName);
}

/**
 * Renames Tailwind's stable-named `app.css` to `app-<hash>.css`.
 *
 * A stable name forces `no-cache`, which lets a tab revalidate into the new
 * stylesheet while still running the previous build's JavaScript. Tailwind
 * purges unused classes per build, so classes the old JS still emits can vanish
 * from the new CSS and elements silently render unstyled. Hashing pins the two
 * together and lets the server cache the stylesheet immutably.
 */
async function hashStylesheet(buildAssetsDir: string): Promise<string> {
  const source = join(buildAssetsDir, "app.css");
  const css = await readFile(source);
  const hashedName = `app-${contentHash8(css)}.css`;
  await rename(source, join(buildAssetsDir, hashedName));
  return join("assets", hashedName);
}

async function copyPierreWorkerAssets(targetDir: string) {
  await mkdir(targetDir, { recursive: true });
  await cp(
    join(pierreWorkerDir, "worker-portable.js"),
    join(targetDir, "worker-portable.js"),
  );

  const files = await readdir(pierreWorkerDir);
  for (const file of files) {
    if (!/^wasm-.*\.js$/.test(file)) continue;
    await cp(join(pierreWorkerDir, file), join(targetDir, file));
  }
}

// `kind` mirrors Bun's `BuildArtifact.kind`; the union keeps the
// "entry-point" comparison below typo-safe.
type IndexHtmlOutput = {
  kind: "entry-point" | "chunk" | "asset" | "sourcemap" | "bytecode";
  path: string;
};

async function writeIndexHtml(
  buildDir: string,
  outputs: IndexHtmlOutput[],
  buildId: string,
) {
  const template = await readFile(templatePath, "utf8");
  // Only entry points get <script> tags. Bun's `splitting: true` emits
  // hundreds of chunks reachable from the entry through static and dynamic
  // imports; listing every chunk here force-downloads the whole bundle
  // (13+ MB) before first render, defeating the code splitting. The
  // browser's module graph pulls static imports itself, and dynamic
  // import() chunks load on demand.
  const scripts = outputs
    .filter((output) => output.kind === "entry-point" && output.path.endsWith(".js"))
    .map((output) => `<script type="module" src="${assetHref(output.path)}"></script>`)
    .join("\n    ");
  const styles = outputs
    .filter((output) => output.path.endsWith(".css"))
    .map((output) => assetHref(output.path))
    .filter((value, index, array) => array.indexOf(value) === index)
    .map((path) => `<link rel="stylesheet" href="${path}" />`)
    .join("\n    ");

  // Records which build this document loaded. A tab reads it back at runtime
  // and compares against /build-id.json; the meta tag is the honest answer
  // because client-side routing never re-fetches index.html, so it stays
  // pinned to the build the tab actually started with.
  const buildMeta = `<meta name="${BUILD_ID_META_NAME}" content="${buildId}" />`;

  const html = template
    .replace("{{styles}}", styles)
    .replace("{{buildMeta}}", buildMeta)
    .replace("{{scripts}}", scripts);

  await writeFile(join(buildDir, "index.html"), html, "utf8");
}

// Atomically point `dist` at the freshly-built directory. Symlink replacement
// via rename(2) is atomic on macOS and Linux, so readers never see a partial
// build: they either resolve through the old symlink or the new one.
async function publishBuild(buildDir: string) {
  // Migrate from the pre-symlink layout: if `dist` exists as a real directory
  // (left over from an older version of this script), remove it so we can
  // replace it with a symlink. Hit at most once per machine.
  const existing = await lstatOrNull(distPath);
  if (existing && !existing.isSymbolicLink()) {
    await rm(distPath, { recursive: true, force: true });
  }

  const tmpLink = `${distPath}.tmp.${process.pid}.${Date.now()}`;
  await symlink(relative(rootPath, buildDir), tmpLink);
  await rename(tmpLink, distPath);
}

async function lstatOrNull(path: string) {
  try {
    return await lstat(path);
  } catch (error: any) {
    if (error?.code === "ENOENT") return null;
    throw error;
  }
}

async function pruneOldBuilds(currentId: string) {
  let entries: string[];
  try {
    entries = await readdir(buildsRootDir);
  } catch (error: any) {
    if (error?.code === "ENOENT") return;
    throw error;
  }

  for (const entry of entries) {
    if (entry === currentId) continue;
    try {
      await rm(join(buildsRootDir, entry), { recursive: true, force: true });
    } catch (error) {
      console.error(`Failed to prune ${entry}:`, error);
    }
  }
}

// Coalesce rapid filesystem events. macOS recursive `fs.watch` fires several
// events per logical save (atomic-write produces create + write + rename) and
// can emit spurious bubble events; without a quiet window the watcher thrashes.
const REBUILD_DEBOUNCE_MS = 75;

// Only file kinds the bundle actually consumes can change what gets built.
// Filtering noise (editor swap files, OS metadata, .tsbuildinfo, lock files)
// keeps the watcher quiet for events that can't change output.
const REBUILD_RELEVANT_EXTS = new Set([
  ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs",
  ".css", ".html", ".json",
  ".svg", ".png", ".jpg", ".jpeg", ".webp", ".gif", ".ico", ".avif",
  ".woff", ".woff2", ".ttf", ".otf",
]);

function rebuildRelevant(filename: string | null | undefined): boolean {
  if (!filename) return true;
  const dot = filename.lastIndexOf(".");
  if (dot < 0) return false;
  return REBUILD_RELEVANT_EXTS.has(filename.slice(dot).toLowerCase());
}

async function main() {
  if (!watch) {
    await buildOnce();
    return;
  }

  await buildOnce();
  let building = false;
  let rebuildQueued = false;
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  const debug = !!process.env.FABRO_BUILD_DEBUG;

  async function rebuild() {
    if (building) {
      rebuildQueued = true;
      return;
    }

    building = true;
    do {
      rebuildQueued = false;
      try {
        await buildOnce();
      } catch (error) {
        console.error(error);
      }
    } while (rebuildQueued);
    building = false;
  }

  function scheduleRebuild(eventType: string, filename: string | null) {
    if (!rebuildRelevant(filename)) {
      if (debug) console.log(`[watch] skip ${eventType} ${filename}`);
      return;
    }
    if (debug) console.log(`[watch] queue ${eventType} ${filename}`);
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      debounceTimer = null;
      void rebuild();
    }, REBUILD_DEBOUNCE_MS);
  }

  const watchers = [
    fsWatch(join(rootPath, "app"), { recursive: true }, scheduleRebuild),
    fsWatch(publicDir, { recursive: true }, scheduleRebuild),
    fsWatch(templatePath, scheduleRebuild),
  ];

  process.on("SIGINT", () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    for (const watcher of watchers) {
      watcher.close();
    }
    process.exit(0);
  });
}

if (import.meta.main) {
  main().catch((error) => {
    console.error(error);
    process.exit(1);
  });
}
