import { join } from "node:path";

const root = join(import.meta.dir, "..");
const host = join(root, "target/release/pocket-character");
const child = Bun.spawn(
  [
    host,
    "--tracking",
    "mock",
    "--background",
    "matte",
    "--output-size",
    "1920x1080",
    "--frames",
    "600",
  ],
  { cwd: root, stdout: "ignore", stderr: "inherit" },
);

try {
  await Bun.sleep(2_000);
  if (child.exitCode !== null) throw new Error(`Pocket host exited early with ${child.exitCode}`);
  const ps = Bun.spawn(["ps", "-axo", "pid=,ppid="], {
    stdout: "pipe",
  });
  const processes = await new Response(ps.stdout).text();
  await ps.exited;
  const rows: Array<[number, number]> = processes
    .trim()
    .split("\n")
    .map((line: string) => line.trim().split(/\s+/).map(Number) as [number, number])
    .filter(([pid, parent]: [number, number]) => Number.isInteger(pid) && Number.isInteger(parent));
  const pids = new Set<number>([child.pid]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const [pid, parent] of rows) {
      if (pids.has(parent) && !pids.has(pid)) {
        pids.add(pid);
        changed = true;
      }
    }
  }
  const lsof = Bun.spawn(["lsof", "-nP", "-a", "-p", [...pids].join(","), "-i"], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const [sockets, exitCode] = await Promise.all([
    new Response(lsof.stdout).text(),
    lsof.exited,
  ]);
  // lsof returns 1 when the intersection has no matching network handles.
  if (exitCode === 0 && sockets.trim()) {
    throw new Error(`Pocket runtime opened network handles:\n${sockets.trim()}`);
  }
  if (exitCode !== 0 && exitCode !== 1) {
    throw new Error(`lsof failed with exit code ${exitCode}`);
  }
  console.log(`runtime offline smoke passed for pid(s): ${[...pids].join(", ")}`);
} finally {
  child.kill();
  await child.exited;
}
