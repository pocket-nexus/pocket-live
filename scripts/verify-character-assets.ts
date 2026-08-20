import { join } from "node:path";

const root = join(import.meta.dir, "..");
const modelPath = join(
  root,
  "plugins/characters/golden-horn/generated/golden-horn.vrm",
);
const bytes = new Uint8Array(await Bun.file(modelPath).arrayBuffer());
if (bytes.length < 100_000 || bytes.length > 2_000_000) {
  throw new Error(`golden-horn VRM size ${bytes.length} is outside the generated-asset bounds`);
}
const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
if (new TextDecoder().decode(bytes.slice(0, 4)) !== "glTF" || view.getUint32(4, true) !== 2) {
  throw new Error("golden-horn output is not a GLB 2.0 file");
}
if (view.getUint32(8, true) !== bytes.length) {
  throw new Error("golden-horn GLB header length does not match the file");
}
const jsonLength = view.getUint32(12, true);
const json = JSON.parse(new TextDecoder().decode(bytes.slice(20, 20 + jsonLength)).trim());
const vrm = json.extensions?.VRM;
const expressions = new Set(
  (vrm?.blendShapeMaster?.blendShapeGroups ?? []).map(
    (expression: { presetName: string }) => expression.presetName,
  ),
);
for (const required of ["blink", "a", "joy"]) {
  if (!expressions.has(required)) throw new Error(`golden-horn is missing '${required}' morph`);
}
const humanBones = vrm?.humanoid?.humanBones ?? [];
const boneNames = new Set(humanBones.map((bone: { bone: string }) => bone.bone));
for (const required of [
  "hips",
  "spine",
  "head",
  "leftUpperArm",
  "leftLowerArm",
  "leftHand",
  "rightUpperArm",
  "rightLowerArm",
  "rightHand",
]) {
  if (!boneNames.has(required)) throw new Error(`golden-horn is missing humanoid bone '${required}'`);
}
const primitives = json.meshes?.[0]?.primitives ?? [];
if (primitives.length < 40) {
  throw new Error(`golden-horn has only ${primitives.length} procedural primitives`);
}
if ((json.images?.length ?? 0) < 7 || json.images.some((image: object) => !("bufferView" in image))) {
  throw new Error("golden-horn textures must be generated and embedded in the GLB");
}
if (vrm?.meta?.title !== "Golden Horn Wanderer") {
  throw new Error("golden-horn VRM metadata is missing or incorrect");
}
console.log(
  `verified Golden Horn VRM (${humanBones.length} humanoid bones, ${primitives.length} primitives, ${json.images.length} embedded textures, ${expressions.size} expressions)`,
);
