import { join, normalize } from "node:path";

interface AssetManifest {
  schema_version: number;
  assets: Array<{
    path: string;
    bytes: number;
    sha256: string;
    source: string;
    license_note: string;
  }>;
}

const root = normalize(join(import.meta.dir, ".."));
const manifest = (await Bun.file(join(root, "assets/manifest.json")).json()) as AssetManifest;
if (manifest.schema_version !== 1) {
  throw new Error(`unsupported asset manifest schema ${manifest.schema_version}`);
}

for (const asset of manifest.assets) {
  if (asset.path.startsWith("/") || asset.path.split("/").includes("..")) {
    throw new Error(`unsafe asset path: ${asset.path}`);
  }
  const file = Bun.file(join(root, asset.path));
  if (!(await file.exists())) throw new Error(`missing ${asset.path}`);
  if (file.size !== asset.bytes) {
    throw new Error(`${asset.path}: expected ${asset.bytes} bytes, got ${file.size}`);
  }
  const hasher = new Bun.CryptoHasher("sha256");
  hasher.update(await file.arrayBuffer());
  const digest = hasher.digest("hex");
  if (digest !== asset.sha256) {
    throw new Error(`${asset.path}: SHA-256 mismatch (got ${digest})`);
  }
  console.log(`verified ${asset.path} ${digest.slice(0, 12)}…`);
}
