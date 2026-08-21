import assert from "node:assert/strict";
import { access, stat } from "node:fs/promises";
import test from "node:test";

async function render() {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  return worker.fetch(
    new Request("http://localhost/", { headers: { accept: "text/html" } }),
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
  assert.match(html, /Camera frames stay on your Mac/i);
  assert.match(html, /youtube-nocookie\.com\/embed\/HjOfFSyM-Mc/);
  assert.match(html, /pocket-live-demo\.mp4/);
  assert.doesNotMatch(html, /Pocket Character|Lose the Chromium|2184 MB|react-loading-skeleton/i);
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
