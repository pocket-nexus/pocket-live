import { $ } from "bun";

await $`bun scripts/apply-pocketjs-live-patch.ts`;
await $`bun scripts/fetch-assets.ts`;
await $`bun scripts/verify-assets.ts`;
await $`bun scripts/audit-offline.ts`;
await $`uv sync --frozen`;
await $`bun scripts/build-ui.ts`;
await $`swift build -c release --package-path native/PocketVisionBridge`;
await $`cargo build --release -p pocket-character`;

console.log("Pocket Live build is ready.");
