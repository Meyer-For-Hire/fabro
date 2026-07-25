import { test, expect } from "bun:test";
import { existsSync } from "node:fs";
import { lstat, readFile, readdir, readlink } from "node:fs/promises";
import { basename, join, relative, sep } from "node:path";

import {
  BUILD_ID_FILE_NAME,
  BUILD_ID_META_NAME,
  parseBuildIdDocument,
} from "../app/lib/build-version-contract";
import { localBundlerInputPaths } from "./build";

const root = Bun.fileURLToPath(new URL("..", import.meta.url));

test("resolved build inputs retain workspace sources and omit installed packages", () => {
  const inputs = localBundlerInputPaths([
    "app/entry.tsx",
    "../../lib/packages/fabro-api-client/src/index.ts",
    "../../node_modules/example/index.js",
  ]).map((path) => relative(root, path).split(sep).join("/"));

  expect(inputs).toEqual([
    "app/entry.tsx",
    "../../lib/packages/fabro-api-client/src/index.ts",
  ]);
});

async function runBuild() {
  const process = Bun.spawn(["bun", "run", "scripts/build.ts"], {
    cwd:    root,
    stdout: "pipe",
    stderr: "pipe",
  });

  const code = await process.exited;
  if (code !== 0) {
    const stderr = await new Response(process.stderr).text();
    const stdout = await new Response(process.stdout).text();
    throw new Error(
      `build failed with code ${code}\nstdout:\n${stdout}\nstderr:\n${stderr}`,
    );
  }
}

test("production builds publish a stable asset set and prune the previous build", async () => {
  await runBuild();

  const distPath = join(root, "dist");
  const firstTarget = await readlink(distPath);
  expect((await lstat(distPath)).isSymbolicLink()).toBe(true);
  expect(firstTarget.startsWith(".dist-builds/")).toBe(true);

  const workerDist = join(distPath, "assets", "pierre-diffs-worker");
  expect(existsSync(join(workerDist, "worker-portable.js"))).toBe(true);

  const upstreamWorkerDir = join(
    root,
    "node_modules",
    "@pierre",
    "diffs",
    "dist",
    "worker",
  );
  const wasmFiles = (await readdir(upstreamWorkerDir))
    .filter((file) => /^wasm-.*\.js$/.test(file))
    .map((file) => basename(file));

  for (const wasmFile of wasmFiles) {
    expect(existsSync(join(workerDist, wasmFile))).toBe(true);
  }

  const published = JSON.parse(
    await readFile(join(distPath, BUILD_ID_FILE_NAME), "utf8"),
  );
  const firstBuildId = parseBuildIdDocument(published);
  expect(firstBuildId).not.toBeNull();

  const html = await readFile(join(distPath, "index.html"), "utf8");
  expect(html).toContain(
    `<meta name="${BUILD_ID_META_NAME}" content="${firstBuildId}" />`,
  );

  const assets = await readdir(join(distPath, "assets"));
  const stylesheets = assets.filter((file) => /^app-.*\.css$/.test(file));
  expect(stylesheets).toHaveLength(1);
  expect(stylesheets[0]).toMatch(/^app-[a-z0-9]{8}\.css$/);
  expect(assets).not.toContain("app.css");
  expect(html).toContain(`href="/assets/${stylesheets[0]}"`);
  expect(html).not.toContain('href="/assets/app.css"');

  // The id is derived from source inputs rather than emitted filenames. Bun's
  // minified identifiers are nondeterministic, so output hashes can move even
  // when the source graph is unchanged.
  await runBuild();

  const secondTarget = await readlink(distPath);
  expect(secondTarget.startsWith(".dist-builds/")).toBe(true);
  expect(secondTarget).not.toBe(firstTarget);
  const secondBuildId = parseBuildIdDocument(
    JSON.parse(await readFile(join(distPath, BUILD_ID_FILE_NAME), "utf8")),
  );
  expect(secondBuildId).toBe(firstBuildId);

  const currentDirName = secondTarget.slice(".dist-builds/".length);
  expect(await readdir(join(root, ".dist-builds"))).toEqual([currentDirName]);
  expect(existsSync(join(distPath, "index.html"))).toBe(true);
}, 120000);

test("watch mode keeps running until interrupted", async () => {
  const process = Bun.spawn([
    "bun",
    "run",
    "scripts/build.ts",
    "--watch",
  ], {
    cwd: root,
    stdout: "pipe",
    stderr: "pipe",
  });

  const result = await Promise.race([
    process.exited.then((code) => ({ kind: "exited" as const, code })),
    Bun.sleep(1000).then(() => ({ kind: "running" as const })),
  ]);

  if (result.kind === "exited") {
    const stderr = await new Response(process.stderr).text();
    const stdout = await new Response(process.stdout).text();
    throw new Error(
      `watch process exited unexpectedly with code ${result.code}\nstdout:\n${stdout}\nstderr:\n${stderr}`,
    );
  }

  process.kill("SIGINT");
  expect([0, 130]).toContain(await process.exited);
});
