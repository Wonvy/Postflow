// 阅读区状态栏里的字数和预计阅读时间。中日韩文字一个字算一个，其他语言按空格分词。
// 这里只有纯函数，方便测试：npm run test:ui

const CJK = /[぀-ヿ㐀-䶿一-鿿豈-﫿가-힯]/g;
/** 每分钟读多少：中日韩文字按字，其他按词。 */
const CJK_PER_MIN = 400;
const WORDS_PER_MIN = 220;

export function wordCount(text: string): { cjk: number; words: number } {
  const cjk = text.match(CJK)?.length ?? 0;
  const words = text.replace(CJK, " ").match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu)?.length ?? 0;
  return { cjk, words };
}

/** 显示的「字数」：中日韩文字加上其他语言的词。 */
export function totalWords(count: { cjk: number; words: number }): number {
  return count.cjk + count.words;
}

/** 预计阅读分钟数，至少 1 分钟；没有文字时为 0。 */
export function readingMinutes(count: { cjk: number; words: number }): number {
  const minutes = count.cjk / CJK_PER_MIN + count.words / WORDS_PER_MIN;
  return minutes === 0 ? 0 : Math.max(1, Math.round(minutes));
}
