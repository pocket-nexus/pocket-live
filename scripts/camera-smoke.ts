import { join } from "node:path";

const root = join(import.meta.dir, "..");
const bridge = join(root, "native/PocketVisionBridge/.build/release/pocket-vision-bridge");
const host = join(root, "target/release/pocket-character");
const deviceProc = Bun.spawn([bridge, "--list-devices"], { stdout: "pipe", stderr: "inherit" });
const devices = JSON.parse(await new Response(deviceProc.stdout).text()) as Array<{
  id: string;
  name: string;
  suspended: boolean;
  max_1080p_fps: number;
}>;
await deviceProc.exited;
const camera = devices
  .filter((device) => !device.suspended)
  .sort((left, right) => right.max_1080p_fps - left.max_1080p_fps)[0];
if (!camera) throw new Error("no non-suspended camera is available");

console.log(
  `opening ${camera.name} (1080p max ${camera.max_1080p_fps} fps) for a five-second local compositor smoke test`,
);
const proc = Bun.spawn(
  [
    host,
    "--tracking",
    "camera",
    "--device",
    camera.id,
    "--background",
    "matte",
    "--output-size",
    "1920x1080",
    "--max-fps",
    "60",
    "--frames",
    "420",
    "--frame-warmup",
    "120",
  ],
  { cwd: root, env: { ...process.env, RUST_LOG: "warn" }, stdout: "pipe", stderr: "inherit" },
);
const stdoutPromise = new Response(proc.stdout).text();
await Bun.sleep(3000);
const childLookup = Bun.spawn(["pgrep", "-P", String(proc.pid)], { stdout: "pipe", stderr: "ignore" });
const childText = await new Response(childLookup.stdout).text();
await childLookup.exited;
const pids = [proc.pid, ...childText.trim().split(/\s+/).map(Number).filter(Number.isFinite)];
const resourceProc = Bun.spawn(
  ["ps", "-o", "%cpu=,rss=", "-p", pids.join(",")],
  { stdout: "pipe", stderr: "ignore" },
);
const resourceLines = (await new Response(resourceProc.stdout).text())
  .trim()
  .split("\n")
  .map((line) => line.trim().split(/\s+/).map(Number))
  .filter(([cpu, rss]) => Number.isFinite(cpu) && Number.isFinite(rss));
await resourceProc.exited;
const resourceSample = {
  processes: resourceLines.length,
  cpu_percent_of_one_core: resourceLines.reduce((sum, [cpu]) => sum + cpu, 0),
  rss_mb: resourceLines.reduce((sum, [, rss]) => sum + rss, 0) / 1024,
};
const stdout = await stdoutPromise;
const exitCode = await proc.exited;
if (exitCode !== 0) throw new Error(`camera compositor smoke exited ${exitCode}`);
const line = stdout.split("\n").find((value) => value.startsWith("WINDOW_BENCH "));
if (!line) throw new Error("camera compositor smoke did not emit WINDOW_BENCH");
const result = JSON.parse(line.slice("WINDOW_BENCH ".length));
console.log(JSON.stringify(result, null, 2));
console.log("RESOURCE_SAMPLE " + JSON.stringify(resourceSample));
if (result.width !== 1920 || result.height !== 1080) throw new Error("camera window is not 1080p");
if (result.tracking_frames < 1) throw new Error("no real Vision tracking frame reached Pocket");
if (result.face_backend_samples < 1) {
  throw new Error("MediaPipe face backend did not consume shared camera frames");
}
if (result.pose_backend_detected < 1) {
  throw new Error("MediaPipe pose backend did not detect a body");
}
if (result.face_frames < 1) {
  console.warn("WARN  no face was visible during this smoke run; schema-v3 face controls are covered by the mock diagnostic");
}
if (result.hand_backend_detected < 1) {
  console.warn("WARN  no hand was visible during this smoke run; raise a hand for manual gesture verification");
}
if (result.shared_video_frames < 1) throw new Error("no real BGRA frame reached Pocket");
if (result.person_matte_frames < 1) throw new Error("no real person matte reached Pocket");
if (result.observed_tracking_fps < 15) {
  throw new Error(`steady tracking ${result.observed_tracking_fps.toFixed(1)} fps is below 15`);
}
