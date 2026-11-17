// 原网页模式里的设备尺寸。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { desktopUserAgent, devicePreset, fitFrame, isWebUrl } from "./webDevice.ts";

test("unknown saved devices fall back to desktop", () => {
  assert.equal(devicePreset("toaster").id, "desktop");
  assert.equal(devicePreset("iphone").id, "iphone");
});

test("desktop fills the area", () => {
  assert.deepEqual(fitFrame(devicePreset("desktop"), false, { width: 700.6, height: 500 }), {
    width: 700,
    height: 500,
    scale: 1,
  });
});

test("a phone keeps its viewport and shrinks to fit", () => {
  const phone = devicePreset("iphone");
  assert.deepEqual(fitFrame(phone, false, { width: 1000, height: 1000 }), { width: 393, height: 852, scale: 1 });
  const small = fitFrame(phone, false, { width: 600, height: 426 });
  assert.equal(small.width, 393);
  assert.equal(small.height, 852);
  assert.equal(small.scale, 0.5);
  const turned = fitFrame(phone, true, { width: 2000, height: 2000 });
  assert.deepEqual([turned.width, turned.height], [852, 393]);
});

test("only http and https pages are opened", () => {
  assert.ok(isWebUrl("https://example.com/a"));
  assert.ok(isWebUrl("http://127.0.0.1:1145/post"));
  assert.ok(!isWebUrl("javascript:alert(1)"));
  assert.ok(!isWebUrl("file:///etc/passwd"));
  assert.ok(!isWebUrl("not a url"));
  assert.ok(!isWebUrl(null));
});

test("the desktop user agent says Safari on macOS only", () => {
  assert.match(desktopUserAgent(true) ?? "", /Safari/);
  assert.equal(desktopUserAgent(false), null);
});
