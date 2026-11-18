// 汉字的拼音首字母，命令面板里输入「ssp」能找到「少数派」。
// 不带拼音字典：用浏览器自带的中文排序（按拼音），和每个声母开头的第一个字比一比就知道它落在哪个字母里。
// 常用字都准，生僻字可能差一个字母。这里只有纯函数，方便测试：npm run test:ui

/**
 * 每个首字母开头、按拼音排序最靠前的字。拼音里没有 i、u、v 开头的。
 * 这一组对着 80 多个常用词挑过，只有多音字可能取到另一个读音（比如「咳」）。
 */
const BOUNDARIES: [string, string][] = [
  ["a", "吖"],
  ["b", "八"],
  ["c", "嚓"],
  ["d", "咑"],
  ["e", "妸"],
  ["f", "发"],
  ["g", "旮"],
  ["h", "哈"],
  ["j", "丌"],
  ["k", "咔"],
  ["l", "垃"],
  ["m", "呣"],
  ["n", "拏"],
  ["o", "噢"],
  ["p", "妑"],
  ["q", "七"],
  ["r", "呥"],
  ["s", "仨"],
  ["t", "他"],
  ["w", "屲"],
  ["x", "夕"],
  ["y", "丫"],
  ["z", "帀"],
];

let collator: Intl.Collator | null | undefined;
function zhCollator(): Intl.Collator | null {
  if (collator === undefined) {
    try {
      const c = new Intl.Collator("zh-Hans-CN-u-co-pinyin");
      // 不支持按拼音排序的环境：「阿」不会排在「八」前面
      collator = c.compare("阿", "八") < 0 && c.compare("八", "嚓") < 0 && c.compare("然", "撒") < 0 ? c : null;
    } catch {
      collator = null;
    }
  }
  return collator;
}

/** 一个汉字的拼音首字母；不是汉字或认不出时为空。 */
export function initialOf(ch: string): string {
  if (!/[一-鿿]/.test(ch)) return "";
  const c = zhCollator();
  if (!c) return "";
  let letter = "";
  for (const [l, first] of BOUNDARIES) {
    if (c.compare(ch, first) >= 0) letter = l;
    else break;
  }
  return letter;
}

/** 整段文字的首字母：汉字取拼音首字母，字母数字原样（小写），其余去掉。「少数派 Pro」→「ssppro」。 */
export function initials(text: string): string {
  let out = "";
  for (const ch of text) {
    if (/[A-Za-z0-9]/.test(ch)) out += ch.toLowerCase();
    else out += initialOf(ch);
  }
  return out;
}
