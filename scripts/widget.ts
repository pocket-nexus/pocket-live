// Build + run the widget: guest bundle, release binary, launch.
import { $ } from "bun";

await $`bun scripts/apply-pocketjs-live-patch.ts`;
await $`bun scripts/fetch-assets.ts`;
await $`bun scripts/build-character-assets.ts`;
await $`bun scripts/build-ui.ts`;
await $`cargo build --release -p pocket-character`;
// Preserve the original transparent desktop-widget preset. Forwarded flags
// come last, so callers can still select a background plugin/mode explicitly.
await $`target/release/pocket-character --background transparent ${process.argv.slice(2)}`;
