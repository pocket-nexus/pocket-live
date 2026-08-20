# pocket-character

> This workspace is extending the upstream character widget into **Pocket
> Live**, a fully local macOS camera-to-VRM live pipeline. The executable
> design, milestones, latency budget, and completion criteria are in
> [TECHNICAL_PLAN.md](TECHNICAL_PLAN.md).

## Pocket Live quick start

Pocket Live is fully local. AVFoundation captures the camera; native Apple
Vision produces the person matte, body pose and hands; pinned MediaPipe Face
Landmarker reads the same shared frame for semantic facial blendshapes. One
body/hand source owns shoulder, elbow and wrist positions, so competing
detectors cannot make the arm jump. Camera frames are not sent to QuickJS,
written to disk, or sent to a network service; no LLM is used.

```sh
# Fetch pinned VRM/MediaPipe assets and install JS/Python dependencies.
bun run setup

# Build the Swift Vision bridge, default character plugin, and Rust release host.
bun run live:build

# Validate assets, the Swift/Rust protocol, cameras, and a tracked render.
# This command only lists cameras; it does not open one.
bun run live:diagnose

# Optional explicit camera-pixel check (emits three tracking frames).
bun run live:diagnose --camera

# Start the local camera-driven Pocket window.
bun run live

# Start the original golden bull character + sunset wanderland preset.
bun run live:golden-horn
bun run live:bilibili

# Select independent character/background plugins.
bun run live -- --character-plugin /absolute/path/to/character/plugin.json
bun run live -- --background-plugin /absolute/path/to/background/plugin.json

# Verify full 1920x1080 render/compositor throughput on the local GPU.
bun run live:benchmark

# Open a real camera and verify face, pose, hands, matte and 1080p60 together.
bun run live:camera-smoke
```

The live command opens an opaque 1920×1080, 60 fps avatar-only window. The
camera remains a local tracking input, but no camera pixels enter the rendered
output. `--background split` remains available as an explicit alternate mode.
OBS configuration and background-mode choices are documented in
[docs/OBS_SETUP.md](docs/OBS_SETUP.md). The local character/background plugin
contracts are documented in [docs/PLUGINS.md](docs/PLUGINS.md).

Voice conversion was evaluated and rejected from the product because the
available target models did not meet the required Chinese speech quality. The
implementation has been removed; measurements, failed approaches and future
acceptance criteria are preserved in
[docs/VOICE_RESEARCH.md](docs/VOICE_RESEARCH.md).

Useful direct host flags:

```sh
target/release/pocket-character --tracking off
target/release/pocket-character --model-info /absolute/path/to/model.vrm
target/release/pocket-character --tracking mock
target/release/pocket-character --tracking camera --device CAMERA_UNIQUE_ID
target/release/pocket-character --character-plugin /absolute/path/to/character/plugin.json
target/release/pocket-character --background-plugin /absolute/path/to/background/plugin.json
target/release/pocket-character --tracking camera --output-size 1920x1080 --background split
target/release/pocket-character --tracking camera --output-size 1920x1080 --background matte
target/release/pocket-character --tracking camera --output-size 1920x1080 --background clean --clean-plate-delay 5
```

The default character plugin's `AvatarSample_A` is visibly a development
fixture, not the final character. Replace the plugin with one containing an
original, properly licensed spider-themed VRM 0.x before visual review or
commercial use. `--model-info` reports missing
humanoid bones and whether blink, mouth, smile, brow and eye controls are
actually present; a good-looking mesh without those controls will still track
poorly. Use `--vrma /absolute/path/to/idle.vrma` to override the idle
animation.

A 3D digital-human desktop widget on the Pocket runtime family — the
[airi](https://github.com/moeru-ai/airi) VRM stage, reimplemented as **one
native process**: a transparent, always-on-top, frameless window rendering a
VRM character with idle animation, auto-blink, eye saccades and spring-bone
physics, driven by a QuickJS policy bundle.

Built to answer a measured question: *what does the same character widget
cost on the Pocket architecture instead of Electron?* See
[DESIGN.md](DESIGN.md) for the architecture and the parity contract, and the
measurement section below for the answer.

## What it does

- **AvatarSample_A** (VRoid official sample) with airi's `idle_loop.vrma`
  looped natively — retargeted at load, not baked.
- **Auto-blink** (sine 0.2 s envelope, 1–6 s uniform interval) and **idle eye
  saccades** (airi's interval distribution table), both from a deterministic
  seeded sim in `pocket-character-core`.
- **Spring bones** (hair / hood / bust) from the model's VRM data, solved by
  `pocket-vrm`'s verlet solver each tick.
- **Widget window**: 450×600 (airi's stage geometry), transparent,
  undecorated, always-on-top, drag anywhere to move, frame-paced at 60 fps
  (the loop sleeps; `--max-fps` to taste).
- **Character policy bundle** (`plugins/characters/*/main.ts` → QuickJS): the `character` surface
  delivers per-tick facts (`blink`, `hovered`, `fps`, events) and accepts
  intent ops (`setTracking`, `setExpression`, `playClip`, `quit`). The
  airi-parity personality is deliberately near-empty policy; a different
  character is a different bundle, no rebuild of the host.

## Layout

| Path | What |
|---|---|
| `crates/pocket-character` | macOS widget host (winit + wgpu via `pocket3d`) |
| `crates/pocket-character-core` | portable behavior sim (blink/saccade/look-at) |
| `crates/pocket-live-core` | tracking contract, calibration, filtering and gesture FSM |
| `native/PocketVisionBridge` | AVFoundation capture, Apple body/hands/person matte and shared frames |
| `native/mediapipe_face_bridge.py` | pinned local semantic face inference |
| `plugin-sdk/` | stable TypeScript API shared by character plugins |
| `plugins/characters/` | local character manifests, assets and policy entries |
| `plugins/backgrounds/` | local background manifests and WGSL themes |
| `scripts/` | Bun TS: asset fetch, bundle build, run, measurement |
| `vendor/pocketjs` | the engine, pinned as a submodule |

The generic halves live in the PocketJS main repo:
`pocket3d` (morph targets, pose injection, widget windows) and `pocket-vrm`
(VRM 0.x parsing, spring bones, VRMA retargeting) — see
[pocket-stack/pocketjs#125](https://github.com/pocket-stack/pocketjs/pull/125).

## Manual verification, from scratch

Prerequisites: a stable Rust toolchain, [Bun](https://bun.sh), Python 3.12 and
[uv](https://docs.astral.sh/uv/). macOS Apple Silicon is the measured platform.

```sh
# 1. Clone with the engine submodule
git clone --recurse-submodules https://github.com/dozycat/pocket-live
cd pocket-live

# 2. One-time setup: vendored bun install, node_modules symlinks,
#    and the model assets (downloaded, never committed)
bun run setup

# 3. Build guest bundle + release binary and launch the widget
bun run widget
```

`bun run widget` leaves the process attached to your terminal — quit with
Ctrl-C. Once built, launch directly:

```sh
target/release/pocket-character                # default character + comic background
target/release/pocket-character --background transparent # 450×600 parity widget
target/release/pocket-character --max-fps 30   # low-power variant
```

With `--background transparent`, you should see a transparent, undecorated, always-on-top 450×600
window with the character idling — looping motion, blinks every 1–6 s, eye
saccades, hair/hood physics. Drag anywhere on the character to move it.

Headless verification (no window; renders the same `Game` object offscreen —
the PNG's alpha channel is the actual window transparency):

```sh
target/release/pocket-character --headless-shot shot.png --ticks 90
```

Reproduce the measurements (launches its own instance, settles 15 s, then
samples ≥60 s and prints a `RESULT` JSON line + markdown row):

```sh
bun scripts/measure.ts                 # 60 fps
bun scripts/measure.ts --max-fps 30    # 30 fps variant
```

### Reproducing the airi side of the comparison

airi's out-of-the-box stage is **Live2D (a different character on a
different renderer)** — for an apples-to-apples screenshot or measurement it
must be switched to its VRM stage with the same model pocket-character
renders (AvatarSample_A). Either pick **AvatarSample_A** in airi's settings
→ Models, or script it:

```sh
/Applications/AIRI.app/Contents/MacOS/AIRI --remote-debugging-port=9222 &
bun scripts/airi-vrm.ts                    # switch to preset-vrm-1 (AvatarSample_A)
bun scripts/airi-vrm.ts preset-live2d-1    # revert
```

Give the VRM scene ~60 s to settle, hands off, then measure. Sum **all**
AIRI processes (search "AIRI" in Activity Monitor — besides the two obvious
helpers there are GPU/network/audio services and a hidden beat-sync
renderer; close the onboarding window first or count its renderer too).

### Reading CPU numbers

All CPU percentages here (and in Activity Monitor's per-process column,
`ps`, `top`) are **percent of one core** — 100 % = one core saturated, so a
16-core machine totals 1600 %. Activity Monitor's bottom System/User/Idle
summary is normalized to the whole machine instead, and covers *all*
processes, not this one. Two things inflate a casual glance right after
launch: the first ~2 CPU-seconds are model decode (init), and clicking or
dragging the widget adds work — judge idle cost only after ~1 min of
hands-off settling, which is what `scripts/measure.ts` automates.

## Model & animation assets

Fetched at setup and never committed: the VRoid sample, animation, and pinned
MediaPipe task bundles. `assets/manifest.json` records exact byte lengths,
SHA-256 digests, sources and license notes.

## Measurements

See [REPORT.md](REPORT.md) for the full comparison against airi on the same
machine, same methodology.
