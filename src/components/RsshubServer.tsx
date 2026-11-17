// RSSHub 实例的设置：地址、访问密钥、测试连接、部署指南。设置的「订阅」页和添加订阅的 RSSHub 表单都用它。
// 地址和密钥存在同步的设置里，改一次所有设备都用新的。

import { useQueryClient } from "@tanstack/react-query";
import { BookOpen, CircleCheck, CircleX, ExternalLink, PlugZap } from "lucide-react";
import { type ReactNode, useState } from "react";
// 部署指南随应用打包：在应用里直接看，打不开 GitHub 的网络也能用
import GUIDE_EN from "../../docs/rsshub.md?raw";
import GUIDE_ZH from "../../docs/rsshub.zh-CN.md?raw";
import { api } from "../lib/api";
import { errorText, useLang, useT } from "../lib/i18n";
import { openExternal } from "../lib/platform";
import { keys, useSettings } from "../lib/queries";
import {
  DEFAULT_INSTANCE,
  guideUrl,
  type InstanceCheck,
  KEY_SETTING,
  normalizeInstance,
  SETTING_KEY,
} from "../lib/rsshub";
import { Markdown } from "./Markdown";
import { Button, cx, Modal } from "./ui";

/** 设置里的 RSSHub 实例地址和访问密钥。 */
export function useRsshubSettings() {
  const qc = useQueryClient();
  const settings = useSettings();
  const raw = settings.data?.[SETTING_KEY];
  const rawKey = settings.data?.[KEY_SETTING];
  const instance = normalizeInstance(typeof raw === "string" ? raw : "") ?? DEFAULT_INSTANCE;
  const key = typeof rawKey === "string" ? rawKey.trim() : "";
  const save = async (patch: { instance?: string; key?: string }) => {
    if (patch.instance !== undefined) await api.setSetting(SETTING_KEY, patch.instance.trim());
    if (patch.key !== undefined) await api.setSetting(KEY_SETTING, patch.key.trim());
    await qc.invalidateQueries({ queryKey: keys.settings });
  };
  return { instance, key, save };
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/** 测试结果说成一句话。 */
function checkText(t: ReturnType<typeof useT>, check: InstanceCheck, instance: string): string {
  const host = hostOf(instance);
  switch (check.state) {
    case "ok":
      return t("rsshubServer.ok", { host, n: check.items });
    case "unreachable":
      return t("rsshubServer.unreachable", { host, detail: check.detail });
    case "challenge":
      return t("rsshubServer.challenge", { host, status: check.status });
    case "unauthorized":
      return t("rsshubServer.unauthorized", { host, status: check.status });
    case "not_rsshub":
      return t("rsshubServer.notRsshub", { host, status: check.status });
    case "error":
      return t("rsshubServer.error", { host, status: check.status });
  }
}

export function RsshubServer({ compact = false, children }: { compact?: boolean; children?: ReactNode }) {
  const t = useT();
  const [guide, setGuide] = useState(false);
  const { instance, key, save } = useRsshubSettings();
  const [instanceDraft, setInstanceDraft] = useState<string | null>(null);
  const [keyDraft, setKeyDraft] = useState<string | null>(null);
  const [showKey, setShowKey] = useState(false);
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);

  const commitInstance = async () => {
    if (instanceDraft === null) return;
    const next = normalizeInstance(instanceDraft);
    setInstanceDraft(null);
    if (next && next !== instance) {
      setResult(null);
      await save({ instance: next });
    }
  };
  const commitKey = async () => {
    if (keyDraft === null) return;
    const next = keyDraft.trim();
    setKeyDraft(null);
    if (next !== key) {
      setResult(null);
      await save({ key: next });
    }
  };

  const test = async () => {
    setChecking(true);
    setResult(null);
    try {
      const check = await api.rsshubCheck(instance, key || undefined);
      setResult({ ok: check.state === "ok", text: checkText(t, check, instance) });
    } catch (err) {
      setResult({ ok: false, text: errorText(t, err) });
    } finally {
      setChecking(false);
    }
  };

  const field =
    "min-w-0 flex-1 rounded bg-transparent px-1 py-0.5 font-mono text-[12px] text-fg-2 outline-none hover:bg-hover focus:bg-hover";

  return (
    <div className={cx("rounded-md border border-line px-3 py-2", !compact && "flex flex-col gap-1")}>
      <div className="flex items-center gap-2 text-[12px] text-fg-3">
        <span className="w-[84px] shrink-0 whitespace-nowrap">{t("rsshub.instance")}</span>
        <input
          aria-label={t("rsshub.instance")}
          value={instanceDraft ?? instance}
          onChange={(e) => setInstanceDraft(e.target.value)}
          onBlur={() => void commitInstance()}
          onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), e.currentTarget.blur())}
          spellCheck={false}
          className={field}
        />
      </div>
      {(showKey || key || !compact) && (
        <div className="flex items-center gap-2 text-[12px] text-fg-3">
          <span className="w-[84px] shrink-0 whitespace-nowrap">{t("rsshubServer.key")}</span>
          <input
            aria-label={t("rsshubServer.key")}
            type="password"
            autoComplete="off"
            value={keyDraft ?? key}
            onChange={(e) => setKeyDraft(e.target.value)}
            onBlur={() => void commitKey()}
            onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), e.currentTarget.blur())}
            placeholder={t("rsshubServer.keyPlaceholder")}
            spellCheck={false}
            className={field}
          />
        </div>
      )}
      {children}
      {instance === DEFAULT_INSTANCE && (
        <p className="mt-1 text-[11px] leading-relaxed text-fg-3">{t("rsshub.publicHint")}</p>
      )}
      {result && (
        <p
          role="status"
          className={cx("mt-1 flex items-start gap-1.5 text-[12px] leading-relaxed", result.ok ? "text-success" : "text-danger")}
        >
          {result.ok ? <CircleCheck size={13} className="mt-0.5 shrink-0" /> : <CircleX size={13} className="mt-0.5 shrink-0" />}
          <span className="min-w-0 break-words">{result.text}</span>
        </p>
      )}
      <div className="mt-1.5 flex flex-wrap items-center gap-2">
        <Button busy={checking} onClick={() => void test()}>
          {!checking && <PlugZap size={13} />}
          {t("rsshubServer.test")}
        </Button>
        {compact && !showKey && !key && (
          <button type="button" onClick={() => setShowKey(true)} className="text-[12px] text-fg-3 hover:text-fg-2">
            {t("rsshubServer.addKey")}
          </button>
        )}
        <span className="flex-1" />
        <button
          type="button"
          onClick={() => setGuide(true)}
          className="flex items-center gap-1 text-[12px] text-accent hover:underline"
        >
          <BookOpen size={12} />
          {t("rsshubServer.guide")}
        </button>
        <RsshubGuide open={guide} onClose={() => setGuide(false)} />
      </div>
    </div>
  );
}

/**
 * 部署指南（docs/rsshub*.md）。去掉只在网页上有用的语言切换和目录，
 * 底部可以去 GitHub 看最新版。
 */
function RsshubGuide({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT();
  const lang = useLang();
  const text = (lang === "zh" ? GUIDE_ZH : GUIDE_EN)
    .split("\n")
    .filter((line) => !/^\[(English|中文版)\]\(/.test(line.trim()) && !/^- \[[^\]]+\]\(#/.test(line.trim()))
    .join("\n");
  return (
    <Modal open={open} onClose={onClose} title={t("rsshubServer.guide")} width={680}>
      <div className="px-5 py-4">
        <Markdown text={text} wrapped className="select-text text-[13px] leading-relaxed text-fg-2" />
        <button
          type="button"
          onClick={() => void openExternal(guideUrl(lang))}
          className="mt-4 flex items-center gap-1 text-[12px] text-fg-3 hover:text-fg-2"
        >
          <ExternalLink size={12} />
          {t("rsshubServer.guideOnline")}
        </button>
      </div>
    </Modal>
  );
}
