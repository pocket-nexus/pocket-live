import { $ } from "bun";

await $`bun scripts/live.ts --character-plugin plugins/characters/golden-horn/plugin.json --background-plugin plugins/backgrounds/golden-sunset/plugin.json ${process.argv.slice(2)}`;
