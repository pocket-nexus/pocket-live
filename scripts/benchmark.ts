import { join } from "node:path";

const root = join(import.meta.dir, "..");
const host = join(root, "target/release/pocket-character");
const frames = Number(process.argv[2] ?? "600");
if (!Number.isInteger(frames) || frames <= 0) {
  throw new Error("usage: bun run live:benchmark [positive-frame-count]");
}

const proc = Bun.spawn(
  [
    host,
    "--tracking",
    "mock",
    "--background",
    "matte",
    "--output-size",
    "1920x1080",
    "--headless-bench",
    String(frames),
  ],
  {
    cwd: root,
    env: { ...process.env, RUST_LOG: "warn" },
    stdout: "pipe",
    stderr: "inherit",
  },
);
const stdout = await new Response(proc.stdout).text();
const exitCode = await proc.exited;
if (exitCode !== 0) throw new Error(`benchmark host exited ${exitCode}`);
const line = stdout
  .split("\n")
  .find((candidate) => candidate.startsWith("BENCH "));
if (!line) throw new Error("benchmark did not emit a BENCH result");
const result = JSON.parse(line.slice("BENCH ".length));
await Bun.write(
  join(root, "artifacts/performance.json"),
  `${JSON.stringify({ ...result, generated_at: new Date().toISOString() }, null, 2)}\n`,
);
console.log(JSON.stringify(result, null, 2));
if (!result.meets_60_fps) {
  throw new Error(`1080p compositor throughput ${result.throughput_fps.toFixed(1)} fps is below 60`);
}
