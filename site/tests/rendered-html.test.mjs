import assert from "node:assert/strict";
import { access, stat } from "node:fs/promises";
import test from "node:test";

async function render(pathname = "/") {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  return worker.fetch(
    new Request(`http://localhost${pathname}`, { headers: { accept: "text/html" } }),
    { ASSETS: { fetch: async () => new Response("Not found", { status: 404 }) } },
    { waitUntil() {}, passThroughOnException() {} },
  );
}

test("server-renders the Pocket Live motion-capture site", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);

  const html = await response.text();
  assert.match(html, /<title>Pocket Live — Local camera-to-VRM motion capture<\/title>/i);
  assert.match(html, /Live as your avatar/);
  assert.match(html, /Face \+ body \+ hands/);
  assert.match(html, /Apple Vision/i);
  assert.match(html, /MediaPipe/i);
  assert.match(html, /Star on GitHub/i);
  assert.match(html, /Build from source/i);
  assert.match(html, /href="\/#signal">How it works<\/a>/);
  assert.match(html, /href="\/blog">Blog<\/a>/);
  assert.doesNotMatch(html, /href="\/#why">Why Pocket Live<\/a>/);
  assert.match(html, /A Pocket Lab Project/i);
  assert.match(html, /Powered by/i);
  assert.match(html, /youtube\.com\/watch\?v=HjOfFSyM-Mc/);
  assert.match(html, /<video\b[^>]*autoplay/i);
  assert.match(html, /<video\b[^>]*muted/i);
  assert.match(html, /pocket-live-demo\.mp4/);
  assert.doesNotMatch(html, /youtube-nocookie\.com|>\s*Download\s*</i);
  assert.doesNotMatch(html, /Made for going live|One camera\.<br\/>No cloud\.|Natural movement|Private by design|Ready for OBS/i);
  assert.doesNotMatch(html, /Local camera-to-VRM motion capture\.<\/p>|MACOS · APPLE SILICON|Pocket Character|Lose the Chromium|2184 MB|react-loading-skeleton/i);
});

test("renders Why Pocket Live as the single blog article", async () => {
  const response = await render("/blog");
  assert.equal(response.status, 200);
  const html = await response.text();
  assert.match(html, /Why Pocket Live/);
  assert.match(html, /What Pocket Live does/);
  assert.match(html, /One camera/);
  assert.match(html, /Fully local/);
  assert.match(html, /Stream-ready output/);
  assert.match(html, /Live2D and VRM: two good solutions, two different abstractions/);
  assert.match(html, /What VRM does not solve for us/);
  assert.match(html, /What the full local pipeline actually costs/);
  assert.match(html, /43\.1%/);
  assert.match(html, /638 MiB/);
  assert.match(html, /MediaPipe face \+ pose \+ hands/);
  assert.match(html, /Apple M5 Max/);
  assert.match(html, /OBS was deliberately excluded/);
  assert.match(html, /docs\.live2d\.com\/en\/cubism-editor-manual\/deformer/);
  assert.match(html, /vrm\.dev\/en\/vrm\/vrm_features/);
  assert.match(html, /pocketjs\.dev\/blog\/pocket-character/);
});

test("ships compact demo media and the real Pocket Live visuals", async () => {
  const [mp4, webm] = await Promise.all([
    stat(new URL("../public/media/pocket-live-demo.mp4", import.meta.url)),
    stat(new URL("../public/media/pocket-live-demo.webm", import.meta.url)),
  ]);

  assert.ok(mp4.size < 1_000_000, "MP4 demo should stay below 1 MB");
  assert.ok(webm.size < 1_000_000, "WebM demo should stay below 1 MB");
  await access(new URL("../public/media/pocket-live-stage.png", import.meta.url));
  await access(new URL("../public/media/pocket-live-tracking.png", import.meta.url));
});
