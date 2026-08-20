import { $ } from "bun";

// One local entry point for generated character assets. Future procedural
// characters can be added here without teaching setup/build scripts about
// individual plugin names.
await $`bun scripts/build-golden-horn.ts`;
