import { test, expect } from "bun:test";
import { existsSync } from "node:fs";
import { lstat, readFile, readdir, readlink } from "node:fs/promises";
import { basename, join } from "node:path";

const root = Bun.fileURLToPath(new URL("..", import.meta.url));

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

test("production build copies Pierre worker assets", async () => {
  await runBuild();

  const workerDist = join(root, "dist", "assets", "pierre-diffs-worker");
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
}, 60000);

test("dist is a symlink into .dist-builds and old builds are pruned", async () => {
  await runBuild();
  await runBuild();

  const distPath = join(root, "dist");
  const stat = await lstat(distPath);
  expect(stat.isSymbolicLink()).toBe(true);

  const target = await readlink(distPath);
  expect(target.startsWith(".dist-builds/")).toBe(true);

  const buildId = target.slice(".dist-builds/".length);
  const buildsRoot = join(root, ".dist-builds");
  const remaining = await readdir(buildsRoot);
  expect(remaining).toEqual([buildId]);

  expect(existsSync(join(distPath, "index.html"))).toBe(true);
}, 60000);

test("publishes a build id that index.html and build-id.json agree on", async () => {
  await runBuild();

  const distPath = join(root, "dist");
  const published = JSON.parse(
    await readFile(join(distPath, "build-id.json"), "utf8"),
  ) as { buildId: string };

  expect(published.buildId).toMatch(/^[a-z0-9]{8}$/);

  const html = await readFile(join(distPath, "index.html"), "utf8");
  expect(html).toContain(
    `<meta name="fabro-build-id" content="${published.buildId}" />`,
  );
}, 60000);

// The id is derived from source inputs rather than emitted filenames precisely
// so this holds: Bun's minified identifier naming is not deterministic, so the
// entry bundle's content hash changes between builds of an unchanged tree
// roughly one run in three. An id that moved with it would fire the client's
// "new version" toast on redeploys of identical code.
test("build id is stable across rebuilds of an unchanged tree", async () => {
  const distPath = join(root, "dist");
  const readBuildId = async () =>
    (
      JSON.parse(await readFile(join(distPath, "build-id.json"), "utf8")) as {
        buildId: string;
      }
    ).buildId;

  await runBuild();
  const first = await readBuildId();
  await runBuild();
  const second = await readBuildId();

  expect(second).toBe(first);
}, 120000);

// A stable-named stylesheet is served `no-cache`, letting a tab revalidate into
// new CSS while running old JS; Tailwind purges per build, so classes the old
// bundle still emits can vanish. The hash must match the `[a-z0-9]{8}` shape
// `is_content_hashed` in static_files.rs keys on.
test("stylesheet is content-hashed and referenced from index.html", async () => {
  await runBuild();

  const distPath = join(root, "dist");
  const assets = await readdir(join(distPath, "assets"));
  const stylesheets = assets.filter((file) => /^app-.*\.css$/.test(file));

  expect(stylesheets).toHaveLength(1);
  expect(stylesheets[0]).toMatch(/^app-[a-z0-9]{8}\.css$/);
  expect(assets).not.toContain("app.css");

  const html = await readFile(join(distPath, "index.html"), "utf8");
  expect(html).toContain(`href="/assets/${stylesheets[0]}"`);
  expect(html).not.toContain('href="/assets/app.css"');
}, 60000);

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
