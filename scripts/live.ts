import { $ } from "bun";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const host = join(root, "target/release/pocket-character");
const forwarded = process.argv.slice(2);

if (!(await Bun.file(host).exists())) {
  console.log("Release host is missing; building Pocket Live first.");
  await $`bun scripts/build-live.ts`;
}

const proc = Bun.spawn(
  [
    host,
    "--tracking",
    "camera",
    "--output-size",
    "1920x1080",
    "--background",
    "virtual",
    ...forwarded,
  ],
  {
    cwd: root,
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  },
);
process.exit(await proc.exited);
