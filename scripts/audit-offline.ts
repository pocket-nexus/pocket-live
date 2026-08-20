import { join } from "node:path";

const root = join(import.meta.dir, "..");
const runtimeRoots = [
  "plugin-sdk",
  "plugins",
  "crates/pocket-character",
  "crates/pocket-live-core",
  "native",
];
const forbidden: Array<[RegExp, string]> = [
  [/\bfetch\s*\(/, "JavaScript fetch"],
  [/\bWebSocket\b/, "JavaScript WebSocket"],
  [/\bXMLHttpRequest\b/, "JavaScript XMLHttpRequest"],
  [/\bURLSession\b/, "Apple URLSession"],
  [/^\s*import\s+Network\s*$/m, "Apple Network framework"],
  [/\bTcpStream\b/, "Rust TCP socket"],
  [/\bUdpSocket\b/, "Rust UDP socket"],
  [/\breqwest\b/, "Rust reqwest client"],
  [/^\s*(?:from|import)\s+(?:socket|requests|urllib|httpx)\b/m, "Python network client"],
];
const sourceExtensions = new Set([".rs", ".swift", ".ts", ".py", ".c", ".h"]);

const violations: string[] = [];
for (const relativeRoot of runtimeRoots) {
  const glob = new Bun.Glob("**/*");
  for await (const relative of glob.scan({ cwd: join(root, relativeRoot), onlyFiles: true })) {
    const extension = relative.slice(relative.lastIndexOf("."));
    if (!sourceExtensions.has(extension)) continue;
    const path = join(root, relativeRoot, relative);
    const text = await Bun.file(path).text();
    for (const [pattern, label] of forbidden) {
      if (pattern.test(text)) violations.push(`${relativeRoot}/${relative}: ${label}`);
    }
  }
}

if (violations.length > 0) {
  throw new Error(`offline runtime audit failed:\n${violations.join("\n")}`);
}
console.log("offline runtime audit passed");
