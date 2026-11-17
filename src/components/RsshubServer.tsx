// RSSHub 实例的设置：地址、访问密钥、测试连接、部署指南。设置的「订阅」页直接放；添加订阅时
// 只在底部显示一行当前实例（RsshubStatus），点开再改。
// 地址和密钥存在同步的设置里，改一次所有设备都用新的。

import { useQueryClient } from "@tanstack/react-query";
import { BookOpen, CircleCheck, CircleX, PlugZap, Server, TriangleAlert } from "lucide-react";
import { useState } from "react";
// 部署指南随应用打包：在应用里直接看，打不开 GitHub 的网络也能用
import GUIDE_EN from "../../docs/rsshub.md?raw";
import GUIDE_ZH from "../../docs/rsshub.zh-CN.md?raw";
import { api } from "../lib/api";
import { errorText, useLang, useT } from "../lib/i18n";
import { keys, useSettings } from "../lib/queries";
import {
  DEFAULT_INSTANCE,
  guideUrl,
  type InstanceCheck,
  KEY_SETTING,
  normalizeInstance,
  SETTING_KEY,
} from "../lib/rsshub";
import { GuideModal } from "./GuideModal";
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

export function RsshubServer() {
  const t = useT();
  const [guide, setGuide] = useState(false);
  const { instance, key, save } = useRsshubSettings();
  const [instanceDraft, setInstanceDraft] = useState<string | null>(null);
  const [keyDraft, setKeyDraft] = useState<string | null>(null);
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
    <div className="flex flex-col gap-1 rounded-md border border-line px-3 py-2">
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
 * 添加订阅底部的一行：现在用的是哪个 RSSHub 实例。公共实例 rsshub.app 对阅读器有限制，标成警告色。
 * 点开是完整的实例设置。
 */
export function RsshubStatus() {
  const t = useT();
  const { instance } = useRsshubSettings();
  const [open, setOpen] = useState(false);
  const isPublic = instance === DEFAULT_INSTANCE;
  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        title={isPublic ? t("rsshub.publicHint") : t("rsshub.changeInstance")}
        className={cx(
          "flex min-w-0 items-center gap-1.5 rounded px-1.5 py-1 text-[12px] hover:bg-hover",
          isPublic ? "text-warning" : "text-fg-3 hover:text-fg-2",
        )}
      >
        {isPublic ? <TriangleAlert size={12} className="shrink-0" /> : <Server size={12} className="shrink-0" />}
        <span className="truncate font-mono">{hostOf(instance)}</span>
      </button>
      <Modal open={open} onClose={() => setOpen(false)} title={t("rsshub.instance")} width={520}>
        <div className="p-4">
          <RsshubServer />
        </div>
      </Modal>
    </>
  );
}

/** 部署指南（docs/rsshub*.md）。 */
function RsshubGuide({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT();
  const lang = useLang();
  return (
    <GuideModal
      open={open}
      onClose={onClose}
      title={t("rsshubServer.guide")}
      text={lang === "zh" ? GUIDE_ZH : GUIDE_EN}
      onlineUrl={guideUrl(lang)}
    />
  );
}
