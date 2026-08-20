// Build an original bull-like VRM from the pinned sample's humanoid skeleton.
// The visible mesh, textures, expressions, and metadata are generated here;
// no geometry or image from the source avatar is copied into the output.
import { deflateSync } from "node:zlib";
import { dirname, join } from "node:path";

type Vec3 = [number, number, number];
type Mat3 = [Vec3, Vec3, Vec3];
type Json = Record<string, any>;

const root = join(import.meta.dir, "..");
const sourcePath = join(root, "assets/AvatarSample_A.vrm");
const outputPath = join(
  root,
  "plugins/characters/golden-horn/generated/golden-horn.vrm",
);

const source = new Uint8Array(await Bun.file(sourcePath).arrayBuffer());
const sourceView = new DataView(source.buffer, source.byteOffset, source.byteLength);
if (new TextDecoder().decode(source.slice(0, 4)) !== "glTF") {
  throw new Error(`${sourcePath} is not a GLB/VRM file`);
}
const sourceJsonLength = sourceView.getUint32(12, true);
const sourceJsonEnd = 20 + sourceJsonLength;
const sourceJson = JSON.parse(
  new TextDecoder().decode(source.slice(20, sourceJsonEnd)).trim(),
) as Json;
const sourceBinLength = sourceView.getUint32(sourceJsonEnd, true);
const sourceBinStart = sourceJsonEnd + 8;
const sourceBin = source.slice(sourceBinStart, sourceBinStart + sourceBinLength);

function accessorFloats(index: number): Float32Array {
  const accessor = sourceJson.accessors[index];
  const view = sourceJson.bufferViews[accessor.bufferView];
  if (accessor.componentType !== 5126 || view.byteStride) {
    throw new Error(`source accessor ${index} must be tightly packed float32`);
  }
  const components = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 }[
    accessor.type as "SCALAR"
  ];
  if (!components) throw new Error(`unsupported accessor type ${accessor.type}`);
  const start = (view.byteOffset ?? 0) + (accessor.byteOffset ?? 0);
  const count = accessor.count * components;
  return new Float32Array(
    sourceBin.slice(start, start + count * 4).buffer,
  );
}

const sourceSkin = sourceJson.skins[0];
const inverseBind = accessorFloats(sourceSkin.inverseBindMatrices);
const skeletonNodes: Json[] = sourceJson.nodes.slice(0, 91).map((node: Json) =>
  structuredClone(node),
);

const parents = new Array<number>(skeletonNodes.length).fill(-1);
for (const [parent, node] of skeletonNodes.entries()) {
  for (const child of node.children ?? []) {
    if (child < skeletonNodes.length) parents[child] = parent;
  }
}
const globals: Vec3[] = skeletonNodes.map(() => [0, 0, 0]);
function globalPosition(index: number): Vec3 {
  const cached = globals[index];
  if (cached.some((value) => value !== 0)) return cached;
  const local = (skeletonNodes[index].translation ?? [0, 0, 0]) as Vec3;
  globals[index] = parents[index] < 0 ? [...local] : add(globalPosition(parents[index]), local);
  return globals[index];
}
for (let index = 0; index < skeletonNodes.length; index += 1) globalPosition(index);

const binaryParts: Uint8Array[] = [];
let binaryLength = 0;
const bufferViews: Json[] = [];
const accessors: Json[] = [];

function alignBinary(alignment = 4): void {
  const padding = (alignment - (binaryLength % alignment)) % alignment;
  if (padding > 0) {
    binaryParts.push(new Uint8Array(padding));
    binaryLength += padding;
  }
}

function appendBytes(bytes: Uint8Array, target?: number): number {
  alignBinary(4);
  const byteOffset = binaryLength;
  binaryParts.push(bytes);
  binaryLength += bytes.byteLength;
  const view: Json = { buffer: 0, byteOffset, byteLength: bytes.byteLength };
  if (target) view.target = target;
  bufferViews.push(view);
  return bufferViews.length - 1;
}

function appendAccessor(
  values: Float32Array | Uint16Array | Uint32Array,
  componentType: number,
  type: string,
  count: number,
  target?: number,
  min?: number[],
  max?: number[],
): number {
  const view = appendBytes(
    new Uint8Array(values.buffer, values.byteOffset, values.byteLength),
    target,
  );
  const accessor: Json = { bufferView: view, componentType, count, type };
  if (min) accessor.min = min;
  if (max) accessor.max = max;
  accessors.push(accessor);
  return accessors.length - 1;
}

const inverseBindAccessor = appendAccessor(
  inverseBind,
  5126,
  "MAT4",
  sourceSkin.joints.length,
);

const crcTable = new Uint32Array(256);
for (let n = 0; n < 256; n += 1) {
  let value = n;
  for (let bit = 0; bit < 8; bit += 1) {
    value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  }
  crcTable[n] = value >>> 0;
}

function crc32(type: Uint8Array, data: Uint8Array): number {
  let crc = 0xffffffff;
  for (const bytes of [type, data]) {
    for (const byte of bytes) crc = crcTable[(crc ^ byte) & 255] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function be32(value: number): Uint8Array {
  const bytes = new Uint8Array(4);
  new DataView(bytes.buffer).setUint32(0, value, false);
  return bytes;
}

function pngChunk(name: string, data: Uint8Array): Uint8Array {
  const type = new TextEncoder().encode(name);
  return concat([be32(data.length), type, data, be32(crc32(type, data))]);
}

function encodePng(width: number, height: number, pixels: Uint8Array): Uint8Array {
  const rows = new Uint8Array(height * (width * 4 + 1));
  for (let y = 0; y < height; y += 1) {
    rows[y * (width * 4 + 1)] = 0;
    rows.set(pixels.slice(y * width * 4, (y + 1) * width * 4), y * (width * 4 + 1) + 1);
  }
  const header = new Uint8Array(13);
  const headerView = new DataView(header.buffer);
  headerView.setUint32(0, width, false);
  headerView.setUint32(4, height, false);
  header.set([8, 6, 0, 0, 0], 8);
  return concat([
    new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk("IHDR", header),
    pngChunk("IDAT", new Uint8Array(deflateSync(rows))),
    pngChunk("IEND", new Uint8Array()),
  ]);
}

function hash(x: number, y: number, seed: number): number {
  let value = Math.imul(x + seed * 1013, 374761393) ^ Math.imul(y + seed * 7919, 668265263);
  value = Math.imul(value ^ (value >>> 13), 1274126177);
  return ((value ^ (value >>> 16)) >>> 0) / 0xffffffff;
}

function texture(
  name: string,
  color: Vec3,
  seed: number,
  noise = 0,
  fur = false,
): Uint8Array {
  const size = fur ? 128 : 16;
  const pixels = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      const random = (hash(x, y, seed) - 0.5) * noise;
      const strand = fur && hash(Math.floor(x / 3), Math.floor(y / 9), seed + 17) > 0.78
        ? -18 * Math.max(0, 1 - ((y + x * 2) % 11) / 11)
        : 0;
      const index = (y * size + x) * 4;
      for (let channel = 0; channel < 3; channel += 1) {
        pixels[index + channel] = clampByte(color[channel] + random + strand);
      }
      pixels[index + 3] = 255;
    }
  }
  const png = encodePng(size, size, pixels);
  Object.defineProperty(png, "name", { value: name });
  return png;
}

const textureSpecs = [
  ["golden coarse fur", [204, 136, 15] as Vec3, 11, 38, true],
  ["dusty pink muzzle", [202, 132, 145] as Vec3, 23, 14, false],
  ["charcoal horn", [53, 49, 61] as Vec3, 31, 22, false],
  ["warm eye white", [242, 228, 202] as Vec3, 41, 5, false],
  ["soft black", [28, 25, 30] as Vec3, 47, 5, false],
  ["hoof gray", [86, 73, 82] as Vec3, 53, 18, false],
  ["deep rose lip", [166, 91, 112] as Vec3, 61, 12, false],
] as const;

const images: Json[] = [];
const textures: Json[] = [];
const materials: Json[] = [];
for (const [index, [name, color, seed, noise, fur]] of textureSpecs.entries()) {
  const bytes = texture(name, color, seed, noise, fur);
  images.push({ bufferView: appendBytes(bytes), mimeType: "image/png", name });
  textures.push({ sampler: 0, source: index });
  materials.push({
    name,
    doubleSided: true,
    pbrMetallicRoughness: {
      baseColorTexture: { index },
      metallicFactor: 0,
      roughnessFactor: 0.95,
    },
  });
}

type Morpher = (position: Vec3) => Vec3;
type Shape = {
  name: string;
  positions: Vec3[];
  normals: Vec3[];
  uvs: [number, number][];
  indices: number[];
  joint: number;
  material: number;
  morphers?: Morpher[];
};
const shapes: Shape[] = [];

function ellipsoid(
  name: string,
  center: Vec3,
  radii: Vec3,
  joint: number,
  material: number,
  options: { basis?: Mat3; bump?: number; morphers?: Morpher[]; segments?: number } = {},
): void {
  const segments = options.segments ?? 24;
  const rings = Math.max(10, Math.floor(segments / 2));
  const basis: Mat3 = options.basis ?? [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
  const positions: Vec3[] = [];
  const normals: Vec3[] = [];
  const uvs: [number, number][] = [];
  const indices: number[] = [];
  for (let ring = 0; ring <= rings; ring += 1) {
    const v = ring / rings;
    const phi = v * Math.PI;
    for (let segment = 0; segment <= segments; segment += 1) {
      const u = segment / segments;
      const theta = u * Math.PI * 2;
      const direction: Vec3 = [
        Math.sin(phi) * Math.cos(theta),
        Math.cos(phi),
        Math.sin(phi) * Math.sin(theta),
      ];
      const ripple = 1 + (options.bump ?? 0) * Math.sin(theta * 11 + phi * 17) * Math.sin(theta * 5 - phi * 13);
      const local: Vec3 = [
        direction[0] * radii[0] * ripple,
        direction[1] * radii[1] * ripple,
        direction[2] * radii[2] * ripple,
      ];
      positions.push(add(center, transform(basis, local)));
      const localNormal = normalize([
        direction[0] / radii[0],
        direction[1] / radii[1],
        direction[2] / radii[2],
      ]);
      normals.push(normalize(transform(basis, localNormal)));
      uvs.push([u, v]);
    }
  }
  const stride = segments + 1;
  for (let ring = 0; ring < rings; ring += 1) {
    for (let segment = 0; segment < segments; segment += 1) {
      const a = ring * stride + segment;
      const b = a + stride;
      indices.push(a, b, a + 1, b, b + 1, a + 1);
    }
  }
  shapes.push({ name, positions, normals, uvs, indices, joint, material, morphers: options.morphers });
}

function segment(
  name: string,
  start: Vec3,
  end: Vec3,
  radius: number,
  joint: number,
  material: number,
  bump = 0,
): void {
  const axis = normalize(sub(end, start));
  const reference: Vec3 = Math.abs(axis[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0];
  const x = normalize(cross(reference, axis));
  const z = normalize(cross(x, axis));
  ellipsoid(
    name,
    scale(add(start, end), 0.5),
    [radius, length(sub(end, start)) * 0.5 + radius * 0.25, radius],
    joint,
    material,
    { basis: [x, axis, z], bump, segments: 18 },
  );
}

function cone(
  name: string,
  base: Vec3,
  tip: Vec3,
  radius: number,
  joint: number,
  material: number,
): void {
  const axis = normalize(sub(tip, base));
  const reference: Vec3 = Math.abs(axis[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0];
  const xAxis = normalize(cross(reference, axis));
  const zAxis = normalize(cross(xAxis, axis));
  const positions: Vec3[] = [tip, base];
  const normals: Vec3[] = [axis, scale(axis, -1)];
  const uvs: [number, number][] = [[0.5, 0], [0.5, 1]];
  const indices: number[] = [];
  const segments = 18;
  for (let index = 0; index < segments; index += 1) {
    const angle = (index / segments) * Math.PI * 2;
    const radial = add(scale(xAxis, Math.cos(angle)), scale(zAxis, Math.sin(angle)));
    positions.push(add(base, scale(radial, radius)));
    normals.push(normalize(add(radial, scale(axis, radius / Math.max(length(sub(tip, base)), 0.01)))));
    uvs.push([index / segments, 1]);
  }
  for (let index = 0; index < segments; index += 1) {
    const current = 2 + index;
    const next = 2 + ((index + 1) % segments);
    indices.push(0, next, current, 1, current, next);
  }
  shapes.push({ name, positions, normals, uvs, indices, joint, material });
}

const hips = globalPosition(1);
const chest = globalPosition(3);
const headBone = globalPosition(18);
const head: Vec3 = [0, headBone[1] + 0.075, -0.02];
const bodyCenter: Vec3 = [0, hips[1] + 0.17, 0.015];
const fur = 0;
const pink = 1;
const horn = 2;
const eyeWhite = 3;
const black = 4;
const hoof = 5;
const lip = 6;

ellipsoid("round belly", bodyCenter, [0.35, 0.49, 0.25], 1, fur, { bump: 0.025 });
ellipsoid("upper torso", [0, chest[1] + 0.015, 0], [0.32, 0.33, 0.225], 3, fur, { bump: 0.025 });
ellipsoid("large bull head", head, [0.25, 0.235, 0.195], 18, fur, { bump: 0.03 });

for (const [side, upper, lower, hand] of [
  [-1, 46, 47, 48],
  [1, 65, 66, 67],
] as const) {
  const shoulder = globalPosition(upper);
  const elbow = globalPosition(lower);
  const wrist = globalPosition(hand);
  segment(`${side < 0 ? "left" : "right"} upper arm`, shoulder, elbow, 0.082, upper, fur, 0.025);
  segment(`${side < 0 ? "left" : "right"} lower arm`, elbow, wrist, 0.073, lower, fur, 0.025);
  ellipsoid(
    `${side < 0 ? "left" : "right"} hoof hand`,
    add(wrist, [side * 0.035, -0.002, -0.006]),
    [0.072, 0.062, 0.065],
    hand,
    hoof,
    { bump: 0.01, segments: 18 },
  );
}

for (const [side, upper, lower, foot] of [
  [-1, 83, 84, 85],
  [1, 87, 88, 89],
] as const) {
  const hip = globalPosition(upper);
  const knee = globalPosition(lower);
  const ankle = globalPosition(foot);
  segment(`${side < 0 ? "left" : "right"} thigh`, hip, knee, 0.105, upper, fur, 0.025);
  segment(`${side < 0 ? "left" : "right"} shin`, knee, ankle, 0.09, lower, fur, 0.025);
  ellipsoid(
    `${side < 0 ? "left" : "right"} hoof foot`,
    add(ankle, [0, -0.045, -0.045]),
    [0.11, 0.075, 0.135],
    foot,
    hoof,
    { bump: 0.01, segments: 18 },
  );
}

segment("left ear", add(head, [-0.18, 0.06, -0.005]), add(head, [-0.31, 0.075, -0.015]), 0.064, 18, fur, 0.015);
segment("right ear", add(head, [0.18, 0.06, -0.005]), add(head, [0.31, 0.075, -0.015]), 0.064, 18, fur, 0.015);
segment("left inner ear", add(head, [-0.205, 0.057, -0.067]), add(head, [-0.285, 0.068, -0.07]), 0.024, 18, pink);
segment("right inner ear", add(head, [0.205, 0.057, -0.067]), add(head, [0.285, 0.068, -0.07]), 0.024, 18, pink);

segment("left short horn", add(head, [-0.12, 0.18, 0.005]), add(head, [-0.19, 0.285, -0.005]), 0.043, 18, horn, 0.01);
segment("right short horn", add(head, [0.12, 0.18, 0.005]), add(head, [0.19, 0.285, -0.005]), 0.043, 18, horn, 0.01);

const leftEyeCenter: Vec3 = add(head, [-0.078, 0.072, -0.182]);
const rightEyeCenter: Vec3 = add(head, [0.078, 0.072, -0.182]);
const blinkMorph = (center: Vec3): Morpher => (position) => [0, -(position[1] - center[1]) * 0.92, 0];
const zeroMorph: Morpher = () => [0, 0, 0];
for (const [name, center, joint] of [
  ["left eye", leftEyeCenter, 19],
  ["right eye", rightEyeCenter, 20],
] as const) {
  const morphers = [blinkMorph(center), zeroMorph, zeroMorph, zeroMorph];
  ellipsoid(name, center, [0.07, 0.032, 0.025], joint, eyeWhite, { morphers, segments: 20 });
  const pupilCenter = add(center, [0, -0.005, -0.027]);
  ellipsoid(`${name} pupil`, pupilCenter, [0.023, 0.021, 0.011], joint, black, {
    morphers: [blinkMorph(pupilCenter), zeroMorph, zeroMorph, zeroMorph],
    segments: 18,
  });
}

ellipsoid("left sleepy lid", add(leftEyeCenter, [0, 0.017, -0.043]), [0.076, 0.018, 0.009], 18, fur, { segments: 18 });
ellipsoid("right sleepy lid", add(rightEyeCenter, [0, 0.017, -0.043]), [0.076, 0.018, 0.009], 18, fur, { segments: 18 });

segment("left heavy brow", add(head, [-0.135, 0.118, -0.215]), add(head, [-0.038, 0.111, -0.215]), 0.009, 18, black);
segment("right heavy brow", add(head, [0.038, 0.111, -0.215]), add(head, [0.135, 0.118, -0.215]), 0.009, 18, black);

const muzzleCenter: Vec3 = add(head, [0, -0.045, -0.188]);
ellipsoid("oversized pink muzzle", muzzleCenter, [0.185, 0.112, 0.09], 18, pink, { bump: 0.012, segments: 24 });
ellipsoid("left nostril", add(muzzleCenter, [-0.055, 0.035, -0.078]), [0.021, 0.014, 0.009], 18, black, { segments: 14 });
ellipsoid("right nostril", add(muzzleCenter, [0.055, 0.035, -0.078]), [0.021, 0.014, 0.009], 18, black, { segments: 14 });

const mouthCenter: Vec3 = add(muzzleCenter, [0, -0.035, -0.084]);
const mouthOpen: Morpher = (position) => [
  -(position[0] - mouthCenter[0]) * 0.18,
  (position[1] - mouthCenter[1]) * 3.2,
  -0.008,
];
const mouthJoy: Morpher = (position) => [
  0,
  0.04 * Math.pow(Math.abs(position[0] - mouthCenter[0]) / 0.11, 1.5),
  0,
];
const mouthSurprised: Morpher = (position) => [
  -(position[0] - mouthCenter[0]) * 0.5,
  (position[1] - mouthCenter[1]) * 4.0,
  -0.012,
];
ellipsoid("upper rounded lip", add(mouthCenter, [0, 0.021, 0.005]), [0.118, 0.032, 0.023], 18, lip, { segments: 22 });
ellipsoid("lower rounded lip", add(mouthCenter, [0, -0.024, 0.005]), [0.12, 0.034, 0.025], 18, lip, { segments: 22 });
ellipsoid("expressive mouth", mouthCenter, [0.108, 0.011, 0.012], 18, black, {
  morphers: [zeroMorph, mouthOpen, mouthJoy, mouthSurprised],
  segments: 22,
});

for (let tuft = -4; tuft <= 4; tuft += 1) {
  const x = tuft * 0.027;
  const base = add(head, [x, 0.205 + 0.012 * Math.cos(tuft), -0.015]);
  cone(`head fur tuft ${tuft + 4}`, base, add(base, [x * 0.08, 0.035 + 0.008 * (tuft % 2), -0.004]), 0.011, 18, fur);
}


function primitive(shape: Shape): Json {
  const positions = new Float32Array(shape.positions.flat());
  const normals = new Float32Array(shape.normals.flat());
  const uvs = new Float32Array(shape.uvs.flat());
  const joints = new Uint16Array(shape.positions.length * 4);
  const weights = new Float32Array(shape.positions.length * 4);
  for (let index = 0; index < shape.positions.length; index += 1) {
    joints[index * 4] = shape.joint;
    weights[index * 4] = 1;
  }
  const min: Vec3 = [Infinity, Infinity, Infinity];
  const max: Vec3 = [-Infinity, -Infinity, -Infinity];
  for (const position of shape.positions) {
    for (let axis = 0; axis < 3; axis += 1) {
      min[axis] = Math.min(min[axis], position[axis]);
      max[axis] = Math.max(max[axis], position[axis]);
    }
  }
  const result: Json = {
    attributes: {
      POSITION: appendAccessor(positions, 5126, "VEC3", shape.positions.length, 34962, min, max),
      NORMAL: appendAccessor(normals, 5126, "VEC3", shape.positions.length, 34962),
      TEXCOORD_0: appendAccessor(uvs, 5126, "VEC2", shape.positions.length, 34962),
      JOINTS_0: appendAccessor(joints, 5123, "VEC4", shape.positions.length, 34962),
      WEIGHTS_0: appendAccessor(weights, 5126, "VEC4", shape.positions.length, 34962),
    },
    indices: appendAccessor(new Uint32Array(shape.indices), 5125, "SCALAR", shape.indices.length, 34963),
    material: shape.material,
    mode: 4,
    extras: { name: shape.name },
  };
  if (shape.morphers) {
    result.targets = shape.morphers.map((morpher) => {
      const deltas = new Float32Array(shape.positions.flatMap((position) => morpher(position)));
      return {
        POSITION: appendAccessor(deltas, 5126, "VEC3", shape.positions.length, 34962),
      };
    });
  }
  return result;
}

const vrm = structuredClone(sourceJson.extensions.VRM);
vrm.exporterVersion = "Pocket Live procedural character builder 1";
vrm.meta = {
  title: "Golden Horn Wanderer",
  version: "1.0",
  author: "Pocket Live — original procedural character",
  contactInformation: "",
  reference: "Original design inspired by broad cinematic 3D cartoon aesthetics",
  texture: -1,
  allowedUserName: "Everyone",
  violentUssageName: "Disallow",
  sexualUssageName: "Disallow",
  commercialUssageName: "Allow",
  otherPermissionUrl: "",
  licenseName: "Other",
  otherLicenseUrl: "",
};
vrm.firstPerson = {
  firstPersonBone: 18,
  firstPersonBoneOffset: { x: 0, y: 0.06, z: 0 },
  meshAnnotations: [{ mesh: 0, firstPersonFlag: "Auto" }],
  lookAtTypeName: "Bone",
  lookAtHorizontalInner: { curve: [0, 0, 1, 1, 0, 0, 0, 0], xRange: 90, yRange: 8 },
  lookAtHorizontalOuter: { curve: [0, 0, 1, 1, 0, 0, 0, 0], xRange: 90, yRange: 12 },
  lookAtVerticalDown: { curve: [0, 0, 1, 1, 0, 0, 0, 0], xRange: 90, yRange: 10 },
  lookAtVerticalUp: { curve: [0, 0, 1, 1, 0, 0, 0, 0], xRange: 90, yRange: 10 },
};
vrm.blendShapeMaster = {
  blendShapeGroups: [
    expression("Blink", "blink", 0),
    // pocket3d currently stores each sparse morph target twice (position and
    // normal overlay slots), so the runtime indices advance by two.
    expression("A", "a", 2),
    expression("Joy", "joy", 4),
    expression("Surprised", "unknown", 6),
  ],
};
vrm.secondaryAnimation = { boneGroups: [], colliderGroups: [] };
vrm.materialProperties = materials.map((material) => ({
  name: material.name,
  shader: "VRM/UnlitTexture",
  renderQueue: 2000,
  floatProperties: {},
  vectorProperties: {},
  textureProperties: {},
  keywordMap: {},
  tagMap: { RenderType: "Opaque" },
}));

const meshNodeIndex = skeletonNodes.length;
const json: Json = {
  asset: { version: "2.0", generator: "Pocket Live Golden Horn Builder" },
  scene: 0,
  scenes: [{ name: "Golden Horn", nodes: [0, meshNodeIndex] }],
  nodes: [
    ...skeletonNodes,
    { name: "Golden Horn procedural mesh", mesh: 0, skin: 0 },
  ],
  skins: [{
    name: "Humanoid",
    inverseBindMatrices: inverseBindAccessor,
    joints: sourceSkin.joints,
    skeleton: 0,
  }],
  meshes: [{
    name: "Golden Horn Wanderer",
    primitives: shapes.map(primitive),
    weights: [0, 0, 0, 0],
    extras: { targetNames: ["Blink", "A", "Joy", "Surprised"] },
  }],
  samplers: [{ magFilter: 9729, minFilter: 9987, wrapS: 10497, wrapT: 10497 }],
  images,
  textures,
  materials,
  accessors,
  bufferViews,
  buffers: [{ byteLength: 0 }],
  extensionsUsed: ["VRM"],
  extensions: { VRM: vrm },
};

alignBinary(4);
json.buffers[0].byteLength = binaryLength;
const binary = concat(binaryParts);
const jsonBytes = new TextEncoder().encode(JSON.stringify(json));
const paddedJson = new Uint8Array(jsonBytes.length + ((4 - (jsonBytes.length % 4)) % 4));
paddedJson.fill(0x20);
paddedJson.set(jsonBytes);
const paddedBinary = new Uint8Array(binary.length + ((4 - (binary.length % 4)) % 4));
paddedBinary.set(binary);

const totalLength = 12 + 8 + paddedJson.length + 8 + paddedBinary.length;
const glb = new Uint8Array(totalLength);
const glbView = new DataView(glb.buffer);
glb.set(new TextEncoder().encode("glTF"), 0);
glbView.setUint32(4, 2, true);
glbView.setUint32(8, totalLength, true);
glbView.setUint32(12, paddedJson.length, true);
glbView.setUint32(16, 0x4e4f534a, true);
glb.set(paddedJson, 20);
const binHeader = 20 + paddedJson.length;
glbView.setUint32(binHeader, paddedBinary.length, true);
glbView.setUint32(binHeader + 4, 0x004e4942, true);
glb.set(paddedBinary, binHeader + 8);

await Bun.$`mkdir -p ${dirname(outputPath)}`.quiet();
await Bun.write(outputPath, glb);
console.log(
  `golden-horn VRM built: ${outputPath} (${shapes.length} primitives, ${(glb.length / 1024).toFixed(0)} KiB)`,
);

function expression(name: string, presetName: string, index: number): Json {
  return {
    name,
    presetName,
    binds: [{ mesh: 0, index, weight: 100 }],
    materialValues: [],
    isBinary: false,
  };
}

function concat(parts: Uint8Array[]): Uint8Array {
  const output = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.length;
  }
  return output;
}

function add(a: Vec3, b: Vec3): Vec3 {
  return [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
}
function sub(a: Vec3, b: Vec3): Vec3 {
  return [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
}
function scale(value: Vec3, amount: number): Vec3 {
  return [value[0] * amount, value[1] * amount, value[2] * amount];
}
function length(value: Vec3): number {
  return Math.hypot(value[0], value[1], value[2]);
}
function normalize(value: Vec3): Vec3 {
  const magnitude = Math.max(length(value), 1e-8);
  return scale(value, 1 / magnitude);
}
function cross(a: Vec3, b: Vec3): Vec3 {
  return [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ];
}
function transform(basis: Mat3, value: Vec3): Vec3 {
  return add(add(scale(basis[0], value[0]), scale(basis[1], value[1])), scale(basis[2], value[2]));
}
function clampByte(value: number): number {
  return Math.max(0, Math.min(255, Math.round(value)));
}
