import assert from "node:assert/strict";
import { test } from "node:test";
import { splitCodes } from "./codeNotice.ts";

const mail = (id: string, address: string, code?: string) => ({ id, from: id, address, subject: `s-${id}`, code });

test("a batch without codes is left alone", () => {
  const event = { mails: [mail("a", "a@x.com"), mail("b", "b@x.com")], total: 2, senders: ["a@x.com", "b@x.com"] };
  const { code, rest } = splitCodes(event);
  assert.equal(code, null);
  assert.equal(rest, event);
});

test("the newest code mail is picked and taken out of the ordinary notification", () => {
  const event = {
    mails: [mail("a", "a@x.com"), mail("c1", "noreply@x.com", "111111"), mail("b", "b@x.com"), mail("c2", "noreply@x.com", "222222")],
    total: 6,
    senders: ["a@x.com", "noreply@x.com", "b@x.com", "noreply@x.com", "d@x.com", "noreply@x.com"],
  };
  const { code, rest } = splitCodes(event);
  assert.equal(code?.code, "111111");
  assert.deepEqual(rest.mails.map((m) => m.id), ["a", "b"]);
  assert.equal(rest.total, 4);
  // 只去掉带验证码的那两封的地址，同一个发件人别的信还在
  assert.deepEqual(rest.senders, ["a@x.com", "b@x.com", "d@x.com", "noreply@x.com"]);
});

test("a single code mail leaves nothing to notify about", () => {
  const { code, rest } = splitCodes({ mails: [mail("c", "n@x.com", "482913")], total: 1 });
  assert.equal(code?.id, "c");
  assert.equal(rest.total, 0);
  assert.deepEqual(rest.mails, []);
  assert.equal(rest.senders, undefined);
});
