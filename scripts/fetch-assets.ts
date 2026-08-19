// Fetch the airi-parity character assets. They are not committed: the VRoid
// sample-model terms are not MIT, so (like airi) we pull them at setup time.
import { $ } from "bun";
import { rename, unlink } from "node:fs/promises";

const ASSETS: Array<{ url: string; out: string; bytes: number }> = [
  {
    url: "https://dist.ayaka.moe/vrm-models/VRoid-Hub/AvatarSample-A/AvatarSample_A.vrm",
    out: "assets/AvatarSample_A.vrm",
    bytes: 26_781_812,
  },
  {
    url: "https://raw.githubusercontent.com/moeru-ai/airi/main/packages/stage-ui-three/src/assets/vrm/animations/idle_loop.vrma",
    out: "assets/idle_loop.vrma",
    bytes: 157_664,
  },
  {
    url: "https://storage.googleapis.com/mediapipe-models/face_landmarker/face_landmarker/float16/latest/face_landmarker.task",
    out: "assets/face_landmarker.task",
    bytes: 3_758_596,
  },
  {
    url: "https://storage.googleapis.com/mediapipe-models/pose_landmarker/pose_landmarker_lite/float16/latest/pose_landmarker_lite.task",
    out: "assets/pose_landmarker_lite.task",
    bytes: 5_777_746,
  },
  {
    url: "https://storage.googleapis.com/mediapipe-models/hand_landmarker/hand_landmarker/float16/latest/hand_landmarker.task",
    out: "assets/hand_landmarker.task",
    bytes: 7_819_105,
  },
];

await $`mkdir -p assets`;
for (const a of ASSETS) {
  const f = Bun.file(a.out);
  if ((await f.exists()) && f.size === a.bytes) {
    console.log(`ok      ${a.out} (${f.size} bytes)`);
    continue;
  }
  console.log(`fetch   ${a.url}`);
  // curl streams large task bundles without the high-CPU buffering behavior
  // observed in Bun.fetch on macOS. Rename only after a successful transfer,
  // so setup never mistakes a partial model for a finished asset.
  const partial = `${a.out}.download`;
  await unlink(partial).catch(() => {});
  const download = Bun.spawn(
    ["curl", "--fail", "--location", "--max-time", "120", "--output", partial, a.url],
    { stdout: "inherit", stderr: "inherit" },
  );
  if ((await download.exited) !== 0) {
    await unlink(partial).catch(() => {});
    throw new Error(`failed to fetch ${a.url}`);
  }
  await rename(partial, a.out);
  const got = Bun.file(a.out).size;
  if (got !== a.bytes)
    console.warn(`warn    ${a.out}: expected ${a.bytes} bytes, got ${got} (upstream may have changed)`);
  else console.log(`done    ${a.out} (${got} bytes)`);
}
