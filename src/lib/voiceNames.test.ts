// 朗读的本机声音：名字和选择。用 Node 自带的测试工具运行：npm run test:ui

import assert from "node:assert/strict";
import { test } from "node:test";
import { pickModel, pickSample, pickVoice, voiceAccent, voiceGender, voiceLabel, voiceLang, voicesFor } from "./voiceNames.ts";

const V11 = ["af_maple", "af_sol", "bf_vale", "zf_001", "zf_002", "zm_009", "zm_010"];
const V10 = ["af_heart", "am_adam", "bm_george", "jf_alpha", "ff_siwis", "zf_xiaoxiao", "zm_yunxi"];

test("names say the language, gender and accent", () => {
  assert.equal(voiceLang("zf_001"), "zh");
  assert.equal(voiceLang("bm_george"), "en");
  assert.equal(voiceLang("jf_alpha"), null);
  assert.equal(voiceGender("zm_009"), "male");
  assert.equal(voiceGender("af_heart"), "female");
  assert.equal(voiceAccent("bf_vale"), "uk");
  assert.equal(voiceAccent("af_sol"), "us");
  assert.equal(voiceAccent("zf_001"), null);
});

test("labels are numbers or names", () => {
  assert.deepEqual(voiceLabel("zf_001", "zh"), { n: 1 });
  assert.deepEqual(voiceLabel("zm_100", "en"), { n: 100 });
  assert.deepEqual(voiceLabel("af_heart", "zh"), { name: "Heart" });
  assert.deepEqual(voiceLabel("zf_xiaoxiao", "zh"), { name: "晓晓" });
  assert.deepEqual(voiceLabel("zf_xiaoxiao", "en"), { name: "Xiaoxiao" });
});

test("voices for a language, women first", () => {
  assert.deepEqual(voicesFor(V10, "en"), ["af_heart", "am_adam", "bm_george"]);
  assert.deepEqual(voicesFor(["zm_009", "zf_001", "zm_010", "zf_002"], "zh"), ["zf_001", "zf_002", "zm_009", "zm_010"]);
  // 日语、法语的声音这个版本念不了，不列出来
  assert.ok(!voicesFor(V10, "en").includes("ff_siwis"));
});

test("the chosen voice wins while it exists", () => {
  const options = { voice_zh: "zf_002", voice_en: "af_maple" };
  assert.equal(pickVoice(V11, options, "zh"), "zf_002");
  assert.equal(pickVoice(V11, options, "zh", "zm_010"), "zm_010");
  // 选过的声音在这个模型里没有（换了模型）：用推荐的
  assert.equal(pickVoice(V11, options, "zh", "zf_xiaoxiao"), "zf_002");
  // 选的是别的语言的声音也不行
  assert.equal(pickVoice(V11, options, "en", "zf_001"), "af_maple");
  assert.equal(pickVoice(V11, undefined, "en"), "af_maple");
  assert.equal(pickVoice([], options, "zh"), null);
});

test("which model reads a language", () => {
  const models = [
    { id: "kokoro-v1.1-zh", installed: true, supported: true, languages: ["zh", "en"], recommended: ["zh"], speakers: V11 },
    { id: "kokoro-v1.0", installed: true, supported: true, languages: ["en", "zh"], recommended: ["en"], speakers: V10 },
  ];
  assert.equal(pickModel(models, "zh", "auto"), "kokoro-v1.1-zh");
  assert.equal(pickModel(models, "en", "auto"), "kokoro-v1.0");
  assert.equal(pickModel(models, "en", "kokoro-v1.1-zh"), "kokoro-v1.1-zh");
  assert.equal(pickModel(models, "zh", "system"), null);
  // 日文、韩文只能用系统语音
  assert.equal(pickModel(models, "ja", "auto"), null);
  // 只装了一个：两种语言都用它
  const one = [{ ...models[0] }, { ...models[1], installed: false }];
  assert.equal(pickModel(one, "en", "auto"), "kokoro-v1.1-zh");
  // 指定的模型被删了：用系统语音，不悄悄换成别的
  assert.equal(pickModel(one, "en", "kokoro-v1.0"), null);
  assert.equal(pickModel([], "zh", "auto"), null);
});

test("ZipVoice reads with the user's samples and comes first once it has one", () => {
  const models = [
    { id: "kokoro-v1.1-zh", kind: "kokoro", installed: true, supported: true, languages: ["zh", "en"], recommended: ["zh"], speakers: V11 },
    { id: "zipvoice", kind: "zipvoice", installed: true, supported: true, languages: ["zh", "en"], recommended: [] },
  ];
  // 还没有声音样本：ZipVoice 用不了
  assert.equal(pickModel(models, "zh", "auto"), "kokoro-v1.1-zh");
  assert.equal(pickModel(models, "zh", "zipvoice"), null);
  // 有了：自动时先用它，中英文都用它
  assert.equal(pickModel(models, "zh", "auto", ["a1"]), "zipvoice");
  assert.equal(pickModel(models, "en", "auto", ["a1"]), "zipvoice");
  assert.equal(pickModel(models, "en", "kokoro-v1.1-zh", ["a1"]), "kokoro-v1.1-zh");
  assert.equal(pickSample(["a1", "b2"], "b2"), "b2");
  assert.equal(pickSample(["a1", "b2"], "gone"), "a1");
  assert.equal(pickSample([], "gone"), null);
});
