import { $ } from "bun";

await $`bun scripts/live.ts --character-plugin plugins/characters/default/plugin.json --background-plugin plugins/backgrounds/japanese-station/plugin.json ${process.argv.slice(2)}`;
