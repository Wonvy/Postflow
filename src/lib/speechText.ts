// 朗读：把正文切成一句一句（太长的再切），语音引擎一次念一小段，暂停、跳过都按段来。
// 只有纯函数，方便测试：npm run test:ui

/** 一段最多多少个字。浏览器的语音合成一次念太长会被截断。 */
export const MAX_CHUNK = 220;

/** 按句号、问号、叹号（中西文）和换行切开，过长的按逗号再切，再不行硬切。空的丢掉。 */
export function chunks(text: string, max = MAX_CHUNK): string[] {
  // 不用后行断言（`(?<=…)`）：Safari 16.4 以前不认，整个界面的脚本都会解析失败。先在句末标点后面
  // 插一个记号再按它切，切出来的和用后行断言一样
  const sentences = text
    .replace(/\r/g, "")
    .replace(/[。！？!?；;]/g, "$&\u0000")
    .replace(/\.\s+/g, ".\u0000")
    .split(/\u0000|\n+/)
    .map((s) => s.replace(/\s+/g, " ").trim())
    .filter(Boolean);
  const out: string[] = [];
  for (const s of sentences) {
    if (s.length <= max) {
      out.push(s);
      continue;
    }
    let rest = s;
    while (rest.length > max) {
      const window = rest.slice(0, max);
      const cut = Math.max(window.lastIndexOf("，"), window.lastIndexOf(", "), window.lastIndexOf("、"), window.lastIndexOf(" "));
      const at = cut > max / 3 ? cut + 1 : max;
      out.push(rest.slice(0, at).trim());
      rest = rest.slice(at).trim();
    }
    if (rest) out.push(rest);
  }
  return out;
}

/** 正文是中文还是别的：汉字多就按中文念。 */
export function guessLang(text: string): "zh" | "ja" | "ko" | "en" {
  const sample = text.slice(0, 2000);
  const count = (re: RegExp) => (sample.match(re) ?? []).length;
  const han = count(/[一-鿿]/g);
  const kana = count(/[぀-ヿ]/g);
  const hangul = count(/[가-힯]/g);
  const latin = count(/[a-zA-Z]/g);
  if (kana > 20 && kana >= hangul) return "ja";
  if (hangul > 20) return "ko";
  return han > latin / 4 ? "zh" : "en";
}
