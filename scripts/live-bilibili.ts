import { $ } from "bun";

const monitor = process.env.POCKET_LIVE_MONITOR ?? "G27T8W";

console.log(`Pocket Live Bilibili canvas: fullscreen monitor=${monitor}`);
await $`bun scripts/live.ts --character-plugin plugins/characters/default/plugin.json --background-plugin plugins/backgrounds/japanese-station/plugin.json --fullscreen --monitor ${monitor} ${process.argv.slice(2)}`;
