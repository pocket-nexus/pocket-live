import { dirname, isAbsolute, join, resolve } from "node:path";

const root = join(import.meta.dir, "..");
const requireRuntimeBundles = process.argv.includes("--runtime");
const manifests: string[] = [];
for await (const relative of new Bun.Glob("**/plugin.json").scan({
  cwd: join(root, "plugins"),
  onlyFiles: true,
})) {
  manifests.push(join(root, "plugins", relative));
}
manifests.sort();
if (manifests.length === 0) throw new Error("no plugin manifests found");

const ids = new Set<string>();
let characterCount = 0;
let backgroundCount = 0;

for (const manifestPath of manifests) {
  const manifest = (await Bun.file(manifestPath).json()) as Record<string, unknown>;
  if (manifest.schema_version !== 1) {
    throw new Error(`${manifestPath}: schema_version must be 1`);
  }
  const id = String(manifest.id ?? "");
  if (!/^[A-Za-z0-9._-]+$/.test(id)) {
    throw new Error(`${manifestPath}: invalid plugin id '${id}'`);
  }
  if (ids.has(id)) throw new Error(`${manifestPath}: duplicate plugin id '${id}'`);
  ids.add(id);
  const pluginRoot = dirname(manifestPath);
  const file = async (field: string, value: unknown, required = true): Promise<string> => {
    if (typeof value !== "string" || value.length === 0) {
      throw new Error(`${manifestPath}: ${field} must be a non-empty path`);
    }
    const path = isAbsolute(value) ? value : resolve(pluginRoot, value);
    if (required && !(await Bun.file(path).exists())) {
      throw new Error(`${manifestPath}: ${field} does not exist: ${path}`);
    }
    return path;
  };

  if (manifest.kind === "character") {
    characterCount += 1;
    await file("model", manifest.model);
    await file("idle_animation", manifest.idle_animation);
    const policy = manifest.policy as Record<string, unknown> | undefined;
    if (!policy) throw new Error(`${manifestPath}: policy is required`);
    await file("policy.entry", policy.entry);
    await file("policy.bundle", policy.bundle, requireRuntimeBundles);
  } else if (manifest.kind === "background") {
    backgroundCount += 1;
    const mode = String(manifest.default_mode ?? "");
    if (!["transparent", "virtual", "camera", "matte", "clean", "split"].includes(mode)) {
      throw new Error(`${manifestPath}: invalid default_mode '${mode}'`);
    }
    const shaderPath = await file("shader", manifest.shader);
    const shader = await Bun.file(shaderPath).text();
    if (!shader.includes("fn plugin_background(")) {
      throw new Error(`${manifestPath}: shader does not implement plugin_background`);
    }
  } else {
    throw new Error(`${manifestPath}: kind must be 'character' or 'background'`);
  }
}

if (characterCount === 0 || backgroundCount === 0) {
  throw new Error("at least one character and one background plugin are required");
}
console.log(
  `verified ${manifests.length} local plugins (${characterCount} character, ${backgroundCount} background)${requireRuntimeBundles ? " with runtime bundles" : ""}`,
);
