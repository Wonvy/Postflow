// 朗读的本机声音：声音的名字怎么读、一种语言用哪个模型哪种声音。只有纯函数，方便测试：npm run test:ui
//
// 声音的名字按 Kokoro 的习惯：第一个字母是语言（a 美式英语、b 英式英语、z 中文，其余的语言这个版本念不了），
// 第二个是 f 女声、m 男声，后面是名字或编号：af_heart、zf_001、zm_yunxi。

/** 本机声音念得了的语言。 */
export type VoiceLang = "zh" | "en";

export function voiceLang(name: string): VoiceLang | null {
  if (/^z[fm]_/.test(name)) return "zh";
  if (/^[ab][fm]_/.test(name)) return "en";
  return null;
}

export function voiceGender(name: string): "female" | "male" | null {
  const g = name.charAt(1);
  return g === "f" ? "female" : g === "m" ? "male" : null;
}

/** 英文声音的口音。 */
export function voiceAccent(name: string): "us" | "uk" | null {
  const a = name.charAt(0);
  return a === "a" ? "us" : a === "b" ? "uk" : null;
}

/** Kokoro v1.0 里几种中文声音的汉字名字。 */
const CHINESE_NAMES: Record<string, string> = {
  xiaobei: "晓北",
  xiaoni: "晓妮",
  xiaoxiao: "晓晓",
  xiaoyi: "晓伊",
  yunjian: "云健",
  yunxi: "云希",
  yunxia: "云夏",
  yunyang: "云扬",
};

/** 声音名字里下划线后面的部分：编号（zf_001 → 1）或者名字（af_heart → Heart，中文界面下 zf_xiaoxiao → 晓晓）。 */
export function voiceLabel(name: string, ui: string): { n: number } | { name: string } {
  const rest = name.slice(name.indexOf("_") + 1);
  if (/^\d+$/.test(rest)) return { n: Number(rest) };
  if (ui === "zh" && CHINESE_NAMES[rest]) return { name: CHINESE_NAMES[rest] };
  return { name: rest.charAt(0).toUpperCase() + rest.slice(1) };
}

/** 一种语言能用的声音，女声在前，各自保持模型里的顺序。 */
export function voicesFor(speakers: readonly string[], lang: VoiceLang): string[] {
  const mine = speakers.filter((s) => voiceLang(s) === lang);
  return [...mine.filter((s) => voiceGender(s) === "female"), ...mine.filter((s) => voiceGender(s) !== "female")];
}

/** 用哪种声音：选过而且模型里还有的 → 选的；否则模型推荐的（`voice_zh`、`voice_en`）；否则这种语言的第一种。 */
export function pickVoice(
  speakers: readonly string[],
  options: Readonly<Record<string, string>> | undefined,
  lang: VoiceLang,
  chosen?: string,
): string | null {
  const usable = voicesFor(speakers, lang);
  if (chosen && usable.includes(chosen)) return chosen;
  const preset = options?.[`voice_${lang}`];
  if (preset && usable.includes(preset)) return preset;
  return usable[0] ?? null;
}

export interface VoiceModelInfo {
  id: string;
  installed: boolean;
  supported: boolean;
  languages: string[];
  recommended: string[];
  speakers?: string[];
}

/**
 * 用哪个本机模型念这种语言，`null` 是用系统语音。
 * - `engine` 是 "system"：总用系统语音。
 * - 是某个模型的 ID：它装着、念得了这种语言就用它，否则用系统语音。
 * - "auto"：装着的模型里推荐给这种语言的，没有的话装着的第一个念得了的。
 */
export function pickModel(models: readonly VoiceModelInfo[], lang: string, engine: string): string | null {
  if (lang !== "zh" && lang !== "en") return null;
  const usable = models.filter(
    (m) => m.installed && m.supported && m.languages.includes(lang) && voicesFor(m.speakers ?? [], lang).length > 0,
  );
  if (engine === "system") return null;
  if (engine !== "auto") return usable.some((m) => m.id === engine) ? engine : null;
  return (usable.find((m) => m.recommended.includes(lang)) ?? usable[0])?.id ?? null;
}
