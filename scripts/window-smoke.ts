import { join } from "node:path";

const root = join(import.meta.dir, "..");
const host = join(root, "target/release/pocket-character");
const frames = Number(process.argv[2] ?? "180");
const warmupFrames = 120;
const proc = Bun.spawn(
  [
    host,
    "--tracking",
    "mock",
    "--background",
    "matte",
    "--output-size",
    "1920x1080",
    "--max-fps",
    "60",
    "--frames",
    String(frames + warmupFrames),
    "--frame-warmup",
    String(warmupFrames),
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
if (exitCode !== 0) throw new Error(`window smoke exited ${exitCode}`);
const line = stdout.split("\n").find((value) => value.startsWith("WINDOW_BENCH "));
if (!line) throw new Error("window smoke did not emit WINDOW_BENCH");
const result = JSON.parse(line.slice("WINDOW_BENCH ".length));
console.log(JSON.stringify(result, null, 2));
if (result.paced_fps < 55 || result.paced_fps > 65) {
  throw new Error(`window pacing ${result.paced_fps.toFixed(1)} fps is outside 55–65 fps`);
}
if (result.width !== 1920 || result.height !== 1080) {
  throw new Error(`window surface is ${result.width}x${result.height}, expected physical 1920x1080`);
}
if (result.one_percent_low_fps < 55) {
  throw new Error(`window 1% low ${result.one_percent_low_fps.toFixed(1)} fps is below 55`);
}
