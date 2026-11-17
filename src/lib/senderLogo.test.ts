// 发件人头像取哪个网站的图标。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { logoDomain, siteDomain } from "./senderLogo.ts";

test("subdomains are dropped, country second levels are kept", () => {
  assert.equal(siteDomain("mail.notion.so"), "notion.so");
  assert.equal(siteDomain("github.com"), "github.com");
  assert.equal(siteDomain("em.example.co.uk"), "example.co.uk");
  assert.equal(siteDomain("service.alipay.com.cn"), "alipay.com.cn");
  assert.equal(siteDomain("a.b.example.io"), "example.io");
});

test("company senders get their site, personal mailboxes do not", () => {
  assert.equal(logoDomain("noreply@github.com"), "github.com");
  assert.equal(logoDomain("Notifications@Mail.Linear.App"), "linear.app");
  assert.equal(logoDomain("someone@gmail.com"), null);
  assert.equal(logoDomain("12345@qq.com"), null);
  assert.equal(logoDomain("a@yahoo.co.jp"), null);
  assert.equal(logoDomain("a@outlook.jp"), null);
  assert.equal(logoDomain("a@vip.163.com"), null);
});

test("broken addresses give nothing", () => {
  assert.equal(logoDomain(null), null);
  assert.equal(logoDomain(""), null);
  assert.equal(logoDomain("no-at-sign"), null);
  assert.equal(logoDomain("x@localhost"), null);
  assert.equal(logoDomain("x@127.0.0.1"), null);
  assert.equal(logoDomain("x@exa mple.com"), null);
});
