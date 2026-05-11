export function workerFactory(): Worker {
  return new Worker(
    "/assets/pierre-diffs-worker/worker-portable.js",
    { type: "module" },
  );
}
