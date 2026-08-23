import { resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const settleSeconds = Number(process.env.POCKET_LIVE_SETTLE_SECONDS ?? 25);
const sampleCount = Number(process.env.POCKET_LIVE_SAMPLE_COUNT ?? 13);
const intervalSeconds = Number(process.env.POCKET_LIVE_SAMPLE_INTERVAL_SECONDS ?? 5);

type ProcessSample = {
  pid: number;
  ppid: number;
  cpu: number;
  rssKiB: number;
  command: string;
  role: string;
};

const sleep = (milliseconds: number) => new Promise((resolveSleep) => setTimeout(resolveSleep, milliseconds));

function readProcesses(): ProcessSample[] {
  const result = Bun.spawnSync(["/bin/ps", "-axo", "pid=,ppid=,%cpu=,rss=,command="], {
    stdout: "pipe",
    stderr: "pipe",
  });
  if (result.exitCode !== 0) throw new Error(result.stderr.toString());
  return result.stdout
    .toString()
    .split("\n")
    .map((line) => line.match(/^\s*(\d+)\s+(\d+)\s+([\d.]+)\s+(\d+)\s+(.+)$/))
    .filter((match): match is RegExpMatchArray => Boolean(match))
    .map((match) => {
      const command = match[5]!;
      let role = "other";
      if (command.includes("pocket-character")) role = "renderer";
      else if (command.includes("pocket-vision-bridge")) role = "vision";
      else if (command.includes("mediapipe_face_bridge.py")) role = "mediapipe";
      else if (command.includes("scripts/vibe.ts")) role = "launcher";
      return {
        pid: Number(match[1]),
        ppid: Number(match[2]),
        cpu: Number(match[3]),
        rssKiB: Number(match[4]),
        command,
        role,
      };
    });
}

function descendants(rootPid: number, processes = readProcesses()): ProcessSample[] {
  const ids = new Set([rootPid]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const process of processes) {
      if (ids.has(process.ppid) && !ids.has(process.pid)) {
        ids.add(process.pid);
        changed = true;
      }
    }
  }
  return processes.filter((process) => ids.has(process.pid));
}

function percentile(values: number[], fraction: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const index = (sorted.length - 1) * fraction;
  const lower = Math.floor(index);
  const upper = Math.ceil(index);
  if (lower === upper) return sorted[lower]!;
  return sorted[lower]! + (sorted[upper]! - sorted[lower]!) * (index - lower);
}

function summarize(values: number[]) {
  return {
    median: percentile(values, 0.5),
    p10: percentile(values, 0.1),
    p90: percentile(values, 0.9),
    min: Math.min(...values),
    max: Math.max(...values),
  };
}

const child = Bun.spawn(["bun", "scripts/vibe.ts", "--vibe", "default"], {
  cwd: root,
  stdin: "ignore",
  stdout: "pipe",
  stderr: "pipe",
  env: process.env,
});
const stdoutPromise = new Response(child.stdout).text();
const stderrPromise = new Response(child.stderr).text();

await sleep(settleSeconds * 1_000);
const samples: Array<{
  seconds: number;
  totalCpu: number;
  totalRssMiB: number;
  processes: ProcessSample[];
}> = [];

for (let index = 0; index < sampleCount; index += 1) {
  if (child.exitCode !== null) break;
  const productProcesses = descendants(child.pid).filter((process) => process.role !== "launcher");
  samples.push({
    seconds: settleSeconds + index * intervalSeconds,
    totalCpu: productProcesses.reduce((sum, process) => sum + process.cpu, 0),
    totalRssMiB: productProcesses.reduce((sum, process) => sum + process.rssKiB, 0) / 1024,
    processes: productProcesses,
  });
  await sleep(intervalSeconds * 1_000);
}

const remaining = descendants(child.pid);
for (const sampledProcess of [...remaining].sort((a, b) => b.pid - a.pid)) {
  try { globalThis.process.kill(sampledProcess.pid, "SIGTERM"); } catch {}
}
try { child.kill("SIGTERM"); } catch {}
await sleep(2_000);
for (const sampledProcess of descendants(child.pid)) {
  try { globalThis.process.kill(sampledProcess.pid, "SIGKILL"); } catch {}
}

const [stdout, stderr] = await Promise.all([stdoutPromise, stderrPromise]);
const logs = `${stdout}\n${stderr}`;
const frameMatches = [...logs.matchAll(/character: t=([\d.]+)s fps=([\d.]+) frameMs=([\d.]+)/g)];
const frameStats = frameMatches.map((match) => ({
  seconds: Number(match[1]),
  fps: Number(match[2]),
  frameMs: Number(match[3]),
}));

if (samples.length === 0) {
  throw new Error(`Pocket Live exited before sampling.\n${logs}`);
}

const roles = ["renderer", "vision", "mediapipe", "other"];
const byRole = Object.fromEntries(
  roles.map((role) => {
    const cpu = samples.map((sample) => sample.processes.filter((process) => process.role === role).reduce((sum, process) => sum + process.cpu, 0));
    const rss = samples.map((sample) => sample.processes.filter((process) => process.role === role).reduce((sum, process) => sum + process.rssKiB, 0) / 1024);
    return [role, { cpu: summarize(cpu), rssMiB: summarize(rss) }];
  }),
);

const commands = [...new Map(samples.flatMap((sample) => sample.processes).map((process) => [process.pid, {
  pid: process.pid,
  role: process.role,
  command: process.command,
}])).values()];

console.log(JSON.stringify({
  measuredAt: new Date().toISOString(),
  configuration: {
    vibe: "default",
    output: "1920x1080",
    tracking: "camera",
    settleSeconds,
    sampleCount: samples.length,
    intervalSeconds,
    cpuDefinition: "macOS ps %CPU; 100% equals one logical CPU core",
    memoryDefinition: "sum of resident set size (RSS) for Pocket Live product processes",
    obsIncluded: false,
  },
  totals: {
    cpu: summarize(samples.map((sample) => sample.totalCpu)),
    rssMiB: summarize(samples.map((sample) => sample.totalRssMiB)),
  },
  byRole,
  frameStats,
  samples: samples.map(({ seconds, totalCpu, totalRssMiB }) => ({ seconds, totalCpu, totalRssMiB })),
  commands,
  logTail: logs.trim().split("\n").slice(-30),
}, null, 2));
