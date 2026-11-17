// 语言文件：把界面文字导出成 JSON，翻译后导入，成为一种新的界面语言。格式见 docs/language-files.md。
//
// 导入的文字要过几道检查：不认识的键丢掉；占位符（{n} 这类）和原文对不上的那条丢掉，免得界面上漏掉数字；
// 缺的条目用 `base` 指定的内置语言（中文或英文）补上。日期、数字的写法也跟着 `base`。
// 这里只有纯函数，方便测试：npm run test:ui

export const FORMAT_VERSION = 1;

export type BaseLang = "zh" | "en";

export interface LangPack {
  /** 语言代码，如 ja、fr、zh-TW。 */
  code: string;
  /** 在语言菜单里显示的名字，如「日本語」。 */
  name: string;
  /** 缺的条目、日期和数字的写法用哪种内置语言。 */
  base: BaseLang;
  messages: Record<string, string>;
}

export interface LangFile {
  postflow_language: number;
  code: string;
  name: string;
  base: BaseLang;
  app_version?: string;
  messages: Record<string, string>;
}

export interface ImportReport {
  pack: LangPack;
  /** 用上的条目数。 */
  used: number;
  /** 界面一共多少条。 */
  total: number;
  /** 文件里有、界面里没有的键（可能是旧版本的）。 */
  unknown: string[];
  /** 占位符和原文对不上、被丢掉的键。 */
  badPlaceholders: string[];
}

/** 导入失败的原因，对应 i18n 里的 `langFile.error.*`。 */
export type ImportError = "json" | "format" | "version" | "code" | "name" | "builtin" | "empty";

export class LangFileError extends Error {
  reason: ImportError;
  constructor(reason: ImportError) {
    super(reason);
    this.reason = reason;
  }
}

const MAX_TEXT = 4000;
const CODE = /^[a-z]{2,3}(-[a-z0-9]{2,8})*$/i;

/** 文字里的占位符，排好序，用来比较。 */
export function placeholders(text: string): string[] {
  return [...new Set(text.match(/\{[a-zA-Z_][a-zA-Z0-9_]*\}/g) ?? [])].sort();
}

function samePlaceholders(a: string, b: string): boolean {
  const x = placeholders(a);
  const y = placeholders(b);
  return x.length === y.length && x.every((v, i) => v === y[i]);
}

/** 生成要导出的文件内容。`messages` 是要导出的那种语言的全部文字（导出内置语言时就是它的字典）。 */
export function exportFile(
  pack: Omit<LangPack, "messages">,
  messages: Record<string, string>,
  appVersion: string | undefined,
): string {
  const file: LangFile = {
    postflow_language: FORMAT_VERSION,
    code: pack.code,
    name: pack.name,
    base: pack.base,
    ...(appVersion ? { app_version: appVersion } : {}),
    messages,
  };
  return `${JSON.stringify(file, null, 2)}\n`;
}

/**
 * 读一个语言文件。`reference` 是界面的全部键和英文原文（用来认键、比占位符），
 * `builtins` 是内置语言的代码，导入的不能和它们重名。
 */
export function importFile(
  text: string,
  reference: Record<string, string>,
  builtins: readonly string[] = ["zh", "en"],
): ImportReport {
  let raw: unknown;
  try {
    raw = JSON.parse(text.replace(/^﻿/, ""));
  } catch {
    throw new LangFileError("json");
  }
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) throw new LangFileError("format");
  const f = raw as Partial<Record<keyof LangFile, unknown>>;
  if (typeof f.postflow_language !== "number") throw new LangFileError("format");
  if (f.postflow_language > FORMAT_VERSION) throw new LangFileError("version");
  if (!f.messages || typeof f.messages !== "object" || Array.isArray(f.messages)) throw new LangFileError("format");

  const code = typeof f.code === "string" ? f.code.trim() : "";
  if (!CODE.test(code)) throw new LangFileError("code");
  if (builtins.includes(code.toLowerCase())) throw new LangFileError("builtin");
  const name = typeof f.name === "string" ? f.name.trim() : "";
  if (!name || name.length > 40) throw new LangFileError("name");
  const base: BaseLang = f.base === "zh" ? "zh" : "en";

  const messages: Record<string, string> = {};
  const unknown: string[] = [];
  const badPlaceholders: string[] = [];
  for (const [key, value] of Object.entries(f.messages as Record<string, unknown>)) {
    const original = reference[key];
    if (original === undefined) {
      unknown.push(key);
      continue;
    }
    if (typeof value !== "string" || !value.trim() || value.length > MAX_TEXT) continue;
    if (!samePlaceholders(original, value)) {
      badPlaceholders.push(key);
      continue;
    }
    messages[key] = value;
  }
  const used = Object.keys(messages).length;
  if (used === 0) throw new LangFileError("empty");
  return {
    pack: { code, name, base, messages },
    used,
    total: Object.keys(reference).length,
    unknown,
    badPlaceholders,
  };
}

/** 语言设置里存的值：`pack:ja` 表示导入的日语。 */
export function packPref(code: string): `pack:${string}` {
  return `pack:${code}`;
}

export function packCodeOf(pref: string): string | null {
  return pref.startsWith("pack:") ? pref.slice(5) : null;
}
