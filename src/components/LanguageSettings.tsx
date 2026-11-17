// 外观设置里的界面语言：选语言，以及导出、导入语言文件（格式见 docs/language-files.md）。

import { Download, Trash2, Upload } from "lucide-react";
import { type ChangeEvent, useRef, useState } from "react";
import { builtinMessages, type MessageKey, useT } from "../lib/i18n";
import { exportFile, type ImportError, importFile, type LangPack, LangFileError, packCodeOf, packPref } from "../lib/langPack";
import { useLangPacks } from "../lib/langPacks";
import { saveTextFile } from "../lib/platform";
import { useStatus } from "../lib/queries";
import { type LangPref, useUi } from "../lib/store";
import { SettingRow } from "./SettingsParts";
import { toast } from "./Toast";
import { Button, cx, Menu, MenuItem, MenuSeparator, Select } from "./ui";

const TOTAL = Object.keys(builtinMessages("en")).length;

export function LanguageSettings() {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const setLang = useUi((s) => s.setLang);
  const packs = useLangPacks((s) => s.packs);
  const addPack = useLangPacks((s) => s.add);
  const removePack = useLangPacks((s) => s.remove);
  const version = useStatus().data?.version;
  const input = useRef<HTMLInputElement>(null);
  // 存结果而不是现成的句子：导入后界面换了语言，这段说明也跟着换
  const [report, setReport] = useState<
    | { ok: true; name: string; used: number; total: number; unknown: number; bad: string[] }
    | { ok: false; reason: ImportError }
    | null
  >(null);

  // 选中的导入语言被删掉了：选择框里显示「跟随系统」，和实际用的一致
  const selected: LangPref = packCodeOf(lang) && !packs.some((p) => packPref(p.code) === lang) ? "system" : lang;

  const save = async (meta: Omit<LangPack, "messages">, messages: Record<string, string>) => {
    try {
      const saved = await saveTextFile(`postflow-language-${meta.code}.json`, exportFile(meta, messages, version), {
        name: "JSON",
        extensions: ["json"],
      });
      if (saved) toast(t("langFile.exported"));
    } catch (err) {
      toast(String(err));
    }
  };

  const exportBuiltin = (base: "zh" | "en") =>
    // 内置语言导出成模板：代码和名字先占个位，翻译的人改成自己的
    void save({ code: "xx", name: t("langFile.newName"), base }, builtinMessages(base));

  const exportPack = (pack: LangPack) =>
    // 没翻译的条目用原文补上，翻译的人能看到还差哪些
    void save(pack, { ...builtinMessages(pack.base), ...pack.messages });

  const onFile = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.currentTarget.files?.[0];
    e.currentTarget.value = "";
    if (!file) return;
    setReport(null);
    try {
      const result = importFile(await file.text(), builtinMessages("en"));
      addPack(result.pack);
      setLang(packPref(result.pack.code));
      setReport({
        ok: true,
        name: result.pack.name,
        used: result.used,
        total: result.total,
        unknown: result.unknown.length,
        bad: result.badPlaceholders,
      });
    } catch (err) {
      setReport({ ok: false, reason: err instanceof LangFileError ? err.reason : "format" });
    }
  };

  const reportText = !report
    ? null
    : !report.ok
      ? t(`langFile.error.${report.reason}` as MessageKey)
      : [
          t("langFile.imported", { name: report.name, used: report.used, total: report.total }),
          report.unknown > 0 ? t("langFile.unknown", { n: report.unknown }) : "",
          report.bad.length > 0
            ? t("langFile.badPlaceholders", { n: report.bad.length, keys: report.bad.slice(0, 5).join(", ") })
            : "",
        ]
          .filter(Boolean)
          .join(" ");

  return (
    <>
      <SettingRow label={t("settings.language")}>
        <Select className="w-44" value={selected} onChange={(e) => setLang(e.target.value as LangPref)}>
          <option value="system">{t("settings.langSystem")}</option>
          <option value="zh">中文</option>
          <option value="en">English</option>
          {packs.map((p) => (
            <option key={p.code} value={packPref(p.code)}>
              {p.name}
            </option>
          ))}
        </Select>
      </SettingRow>
      <SettingRow label={t("langFile.title")} hint={t("langFile.hint")}>
        <div className="flex gap-2">
          <Menu
            align="end"
            trigger={
              <Button>
                <Download size={14} />
                {t("langFile.export")}
              </Button>
            }
          >
            <MenuItem onSelect={() => exportBuiltin("zh")}>{t("langFile.exportFrom", { name: "中文" })}</MenuItem>
            <MenuItem onSelect={() => exportBuiltin("en")}>{t("langFile.exportFrom", { name: "English" })}</MenuItem>
            {packs.length > 0 && <MenuSeparator />}
            {packs.map((p) => (
              <MenuItem key={p.code} onSelect={() => exportPack(p)}>
                {t("langFile.exportFrom", { name: p.name })}
              </MenuItem>
            ))}
          </Menu>
          <Button onClick={() => input.current?.click()}>
            <Upload size={14} />
            {t("langFile.import")}
          </Button>
          <input ref={input} type="file" accept=".json,application/json" className="hidden" onChange={(e) => void onFile(e)} />
        </div>
      </SettingRow>
      {reportText && (
        <p className={cx("-mt-1 pb-2 text-[12px] leading-relaxed", report?.ok ? "text-fg-2" : "text-danger")}>
          {reportText}
        </p>
      )}
      {packs.length > 0 && (
        <ul className="flex flex-col gap-1 pb-2">
          {packs.map((p) => {
            const used = Object.keys(p.messages).length;
            return (
              <li key={p.code} className="flex items-center gap-3 rounded-md px-2 py-1.5 text-[13px] hover:bg-hover">
                <span className="min-w-0 flex-1 truncate text-fg">
                  {p.name} <span className="text-fg-3">· {p.code}</span>
                </span>
                <span className="tabular text-[12px] text-fg-3">
                  {t("langFile.coverage", { used, total: TOTAL, percent: Math.round((used / TOTAL) * 100) })}
                </span>
                <button
                  type="button"
                  aria-label={t("langFile.remove", { name: p.name })}
                  title={t("langFile.remove", { name: p.name })}
                  onClick={() => {
                    removePack(p.code);
                    if (lang === packPref(p.code)) setLang("system");
                  }}
                  className="inline-flex size-6 items-center justify-center rounded text-fg-3 hover:bg-active hover:text-danger"
                >
                  <Trash2 size={13} />
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </>
  );
}
