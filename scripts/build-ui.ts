// Build local character-plugin policy bundles. An explicit manifest builds
// only that plugin; the default build discovers every local character plugin.
import { basename, dirname, isAbsolute, join, resolve } from "node:path";

type CharacterPluginManifest = {
  schema_version: number;
  kind: "character";
  id: string;
  name: string;
  policy: { entry: string; bundle: string };
};

const root = join(import.meta.dir, "..");
const flagIndex = process.argv.lastIndexOf("--character-plugin");
if (flagIndex >= 0 && !process.argv[flagIndex + 1]) {
  throw new Error("--character-plugin requires a manifest path");
}
const requested = flagIndex >= 0 ? process.argv[flagIndex + 1] : undefined;
const manifestPaths: string[] = [];
if (requested) {
  manifestPaths.push(resolve(process.cwd(), requested));
} else {
  for await (const relative of new Bun.Glob("**/plugin.json").scan({
    cwd: join(root, "plugins/characters"),
    onlyFiles: true,
  })) {
    manifestPaths.push(join(root, "plugins/characters", relative));
  }
  manifestPaths.sort();
}
if (manifestPaths.length === 0) throw new Error("no character plugins found");

for (const manifestPath of manifestPaths) {
  const manifest = (await Bun.file(manifestPath).json()) as CharacterPluginManifest;
  if (manifest.schema_version !== 1 || manifest.kind !== "character") {
    throw new Error(`${manifestPath} is not a character plugin schema v1 manifest`);
  }
  if (!manifest.id || !manifest.policy?.entry || !manifest.policy?.bundle) {
    throw new Error(`${manifestPath} is missing id, policy.entry, or policy.bundle`);
  }

  const pluginRoot = dirname(manifestPath);
  const fromPlugin = (value: string): string =>
    isAbsolute(value) ? value : resolve(pluginRoot, value);
  const entryPath = fromPlugin(manifest.policy.entry);
  const bundlePath = fromPlugin(manifest.policy.bundle);
  if (!(await Bun.file(entryPath).exists())) {
    throw new Error(`character plugin entry does not exist: ${entryPath}`);
  }

  const result = await Bun.build({
    entrypoints: [entryPath],
    outdir: dirname(bundlePath),
    naming: basename(bundlePath),
    format: "iife",
    target: "browser",
    minify: false,
  });
  if (!result.success) {
    for (const log of result.logs) console.error(log);
    process.exit(1);
  }
  if (!(await Bun.file(bundlePath).exists())) {
    throw new Error(`build succeeded but did not produce manifest bundle: ${bundlePath}`);
  }
  console.log(`character plugin built: ${manifest.id} -> ${bundlePath}`);
}
