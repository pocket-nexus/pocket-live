import { $ } from "bun";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const bridge = join(
  root,
  "native/PocketVisionBridge/.build/release/pocket-vision-bridge",
);
const host = join(root, "target/release/pocket-character");
const output = join(root, "artifacts/diagnose-tracking.png");

type Check = { name: string; detail: string };
const checks: Check[] = [];

function pass(name: string, detail: string): void {
  checks.push({ name, detail });
  console.log(`PASS  ${name}: ${detail}`);
}

function requireFile(path: string, minimumBytes: number): void {
  const file = Bun.file(path);
  if (file.size < minimumBytes) {
    throw new Error(`${path} is missing or smaller than ${minimumBytes} bytes`);
  }
}

async function pngSize(path: string): Promise<[number, number]> {
  const bytes = new Uint8Array(await Bun.file(path).arrayBuffer());
  if (bytes.length < 24 || new TextDecoder().decode(bytes.slice(1, 4)) !== "PNG") {
    throw new Error(`${path} is not a PNG`);
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return [view.getUint32(16), view.getUint32(20)];
}

requireFile(join(root, "assets/AvatarSample_A.vrm"), 1_000_000);
requireFile(join(root, "assets/idle_loop.vrma"), 1_000);
requireFile(join(root, "plugins/characters/default/dist/character.js"), 100);
requireFile(bridge, 10_000);
requireFile(host, 10_000);
pass("artifacts", "default character plugin assets, Vision bridge, and host exist");

await $`bun scripts/verify-plugins.ts --runtime`.quiet();
pass("plugins", "character and background manifests resolve to local runtime files");

await $`bun scripts/verify-character-assets.ts`.quiet();
pass("generated character", "Golden Horn VRM structure, embedded textures, bones, and morphs are valid");

await $`bun scripts/verify-assets.ts`.quiet();
pass("integrity", "runtime assets match the pinned SHA-256 manifest");

await $`bun scripts/audit-offline.ts`.quiet();
pass("offline", "runtime-owned sources contain no network client surface");

const mockText = await $`${bridge} --mock`.quiet().text();
const mock = JSON.parse(mockText);
if (
  mock.schema_version !== 3 ||
  mock.body?.joints?.length !== 16 ||
  mock.hands?.length !== 2
) {
  throw new Error("Vision mock does not match TrackingFrame schema v1");
}
pass("protocol", "Swift mock matches TrackingFrame schema v3");

const deviceText = await $`${bridge} --list-devices`.quiet().text();
const devices = JSON.parse(deviceText) as Array<{
  id: string;
  name: string;
  suspended: boolean;
}>;
pass(
  "cameras",
  devices.length === 0
    ? "no camera discovered"
    : devices
        .map((device) => `${device.name}${device.suspended ? " (suspended)" : ""}`)
        .join(", "),
);

await $`mkdir -p ${join(root, "artifacts")}`.quiet();
await $`${host} --headless-shot ${output} --tracking mock --ticks 90 --output-size 1920x1080 --background matte`.quiet();
requireFile(output, 10_000);
const renderedSize = await pngSize(output);
if (renderedSize[0] !== 1920 || renderedSize[1] !== 1080) {
  throw new Error(`expected a 1920x1080 render; received ${renderedSize.join("x")}`);
}
pass("render", `mock camera + person matte + VRM composited at 1920x1080 to ${output}`);

if (process.argv.includes("--camera")) {
  const camera = devices.find((device) => !device.suspended);
  if (!camera) throw new Error("no non-suspended camera is available");
  const cameraProc = Bun.spawn(
    [bridge, "--device", camera.id, "--tracking-fps", "5", "--max-frames", "3"],
    { cwd: root, stdout: "pipe", stderr: "pipe" },
  );
  const [text, cameraDiagnostics, cameraExit] = await Promise.all([
    new Response(cameraProc.stdout).text(),
    new Response(cameraProc.stderr).text(),
    cameraProc.exited,
  ]);
  if (cameraExit !== 0) throw new Error(cameraDiagnostics.trim() || `camera exited ${cameraExit}`);
  const frames = text
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
  if (frames.length !== 3 || frames.some((frame) => frame.schema_version !== 3)) {
    throw new Error("camera did not return three valid tracking frames");
  }
  const durationNs = frames.at(-1).captured_at_ns - frames[0].captured_at_ns;
  const observedFPS = durationNs > 0 ? ((frames.length - 1) * 1e9) / durationNs : 0;
  const imageSize = frames[0].image_size.join("x");
  const capture = cameraDiagnostics.match(/capture=(\d+x\d+)@(\d+)/);
  pass(
    "camera tracking",
    `${camera.name} capture=${capture?.[1] ?? imageSize}@${capture?.[2] ?? "device-default"}, ` +
      `emitted 3 local Vision frames at ${observedFPS.toFixed(1)} tracking fps`,
  );
}

console.log(`\n${checks.length} diagnostic checks passed.`);
if (!process.argv.includes("--camera")) {
  console.log("Camera pixels were not opened. Use `bun run live:diagnose --camera` for that check.");
}
