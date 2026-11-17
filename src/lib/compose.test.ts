// 回复、转发时预先填好的内容。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  addressOf,
  attribution,
  formatAddress,
  forwardHeader,
  forwardSubject,
  isValidAddress,
  prefill,
  quote,
  replyRecipients,
  replySubject,
  splitAddresses,
  type ReplyContext,
} from "./compose.ts";

const mb = (address: string, name: string | null = null) => ({ name, address });

const CTX: ReplyContext = {
  account_id: "acc",
  message_id: "m@example.com",
  subject: "Lunch?",
  date: Date.UTC(2026, 10, 16, 8, 0),
  from: [mb("ada@example.com", "Ada")],
  reply_to: [],
  to: [mb("me@example.com", "Me"), mb("bob@example.com", "Bob")],
  cc: [mb("carol@example.com"), mb("ME@example.com")],
  text: "Noon?\n\n> earlier\nOK",
  thread: { in_reply_to: "m@example.com", references: ["m@example.com"] },
  attachments: [],
};

test("addresses are shown with their names and parsed back", () => {
  assert.equal(formatAddress(mb("ada@example.com", "Ada")), "Ada <ada@example.com>");
  assert.equal(formatAddress(mb("ada@example.com")), "ada@example.com");
  assert.equal(formatAddress(mb("ada@example.com", "ada@example.com")), "ada@example.com");
  assert.equal(formatAddress(mb("ada@example.com", 'Lovelace, "Ada"')), '"Lovelace, Ada" <ada@example.com>');
  assert.equal(addressOf("Ada <Ada@Example.com>"), "ada@example.com");
  assert.equal(addressOf(" bob@example.com "), "bob@example.com");
});

test("address checks match the core", () => {
  assert.ok(isValidAddress("Ada <ada@example.com>"));
  assert.ok(isValidAddress("a.b+c@sub.example.org"));
  for (const bad of ["", "ada", "a@b", "a b@example.com", "@example.com", "a@example.com.", "a@@example.com"]) {
    assert.ok(!isValidAddress(bad), bad);
  }
});

test("pasted lists are split into recipients, keeping quoted names whole", () => {
  assert.deepEqual(splitAddresses('a@example.com, "Lovelace, Ada" <ada@example.com>;b@example.com\nc@example.com'), [
    "a@example.com",
    '"Lovelace, Ada" <ada@example.com>',
    "b@example.com",
    "c@example.com",
  ]);
  assert.deepEqual(splitAddresses("甲 <a@example.com>，乙 <b@example.com>"), ["甲 <a@example.com>", "乙 <b@example.com>"]);
  assert.deepEqual(splitAddresses(" , ;"), []);
});

test("subjects get one prefix", () => {
  assert.equal(replySubject("Lunch?"), "Re: Lunch?");
  assert.equal(replySubject("RE: Lunch?"), "RE: Lunch?");
  assert.equal(replySubject("回复：午饭"), "回复：午饭");
  assert.equal(replySubject("Re[2]: x"), "Re[2]: x");
  assert.equal(replySubject("Fwd: x"), "Re: Fwd: x");
  assert.equal(forwardSubject("Lunch?"), "Fwd: Lunch?");
  assert.equal(forwardSubject("FW: Lunch?"), "FW: Lunch?");
  assert.equal(forwardSubject("转发：午饭"), "转发：午饭");
  assert.equal(replySubject(""), "Re: ");
});

test("quotes nest without extra spaces", () => {
  assert.equal(quote("a\n\n> b\nc"), "> a\n>\n>> b\n> c");
});

test("reply goes to Reply-To or the sender; reply all drops yourself", () => {
  const self = ["me@example.com"];
  assert.deepEqual(replyRecipients(CTX, false, self), { to: [mb("ada@example.com", "Ada")], cc: [] });
  assert.deepEqual(replyRecipients({ ...CTX, reply_to: [mb("list@example.com")] }, false, self).to, [
    mb("list@example.com"),
  ]);
  assert.deepEqual(replyRecipients(CTX, true, self), {
    to: [mb("ada@example.com", "Ada"), mb("bob@example.com", "Bob")],
    cc: [mb("carol@example.com")],
  });
});

test("replying to your own mail goes to the people you wrote to", () => {
  const mine: ReplyContext = { ...CTX, from: [mb("me@example.com", "Me")], to: [mb("ada@example.com")], cc: [] };
  assert.deepEqual(replyRecipients(mine, false, ["me@example.com"]).to, [mb("ada@example.com")]);
  // 写给自己的信，回复时还是写给自己
  const note: ReplyContext = { ...CTX, from: [mb("me@example.com")], to: [mb("me@example.com")], cc: [] };
  assert.deepEqual(replyRecipients(note, false, ["me@example.com"]).to, [mb("me@example.com")]);
  assert.deepEqual(replyRecipients(note, true, ["me@example.com"]).to, [mb("me@example.com")]);
});

test("mail between two of your own mailboxes is answered to the other one", () => {
  const ctx: ReplyContext = { ...CTX, from: [mb("work@example.com", "Sam")], to: [mb("me@example.org")], cc: [] };
  // 在 me@example.org 里回复
  assert.deepEqual(replyRecipients(ctx, false, ["me@example.org"]).to, [mb("work@example.com", "Sam")]);
  assert.deepEqual(replyRecipients(ctx, true, ["me@example.org"]), { to: [mb("work@example.com", "Sam")], cc: [] });
});

test("replies and forwards are prefilled", () => {
  const reply = prefill("reply-all", CTX, ["me@example.com"], "en");
  assert.deepEqual(reply.to, ["Ada <ada@example.com>", "Bob <bob@example.com>"]);
  assert.deepEqual(reply.cc, ["carol@example.com"]);
  assert.equal(reply.subject, "Re: Lunch?");
  assert.ok(reply.text.startsWith("\n\nOn "), reply.text);
  assert.ok(reply.text.endsWith("Ada wrote:\n> Noon?\n>\n>> earlier\n> OK"), reply.text);

  const zh = prefill("reply", CTX, ["me@example.com"], "zh");
  assert.ok(zh.text.includes("Ada 写道：\n> Noon?"), zh.text);

  const fwd = prefill("forward", CTX, ["me@example.com"], "en");
  assert.deepEqual(fwd.to, []);
  assert.equal(fwd.subject, "Fwd: Lunch?");
  assert.ok(fwd.text.includes("From: Ada <ada@example.com>\nDate: "), fwd.text);
  assert.ok(fwd.text.includes("To: Me <me@example.com>, Bob <bob@example.com>"), fwd.text);
  assert.ok(fwd.text.endsWith("\n\nNoon?\n\n> earlier\nOK"), fwd.text);

  assert.deepEqual(prefill("new", null, [], "en"), { to: [], cc: [], subject: "", text: "" });
});

test("attribution and forward header work without a date or name", () => {
  const bare: ReplyContext = { ...CTX, date: null, from: [] };
  assert.equal(attribution(bare, "en"), "Original message:");
  assert.equal(attribution(bare, "zh"), "原信：");
  assert.ok(!forwardHeader(bare, "zh").includes("发件人"));
});
