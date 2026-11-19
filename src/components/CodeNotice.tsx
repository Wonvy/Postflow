// 验证码提醒：收到带验证码的邮件时右下角弹出的卡片。大号的验证码，一个复制按钮，点发件人那一行打开邮件。
// 过几分钟自己收起来（验证码也该过期了）；新的验证码来了换掉旧的。开关在设置 → 通用里。

import { Check, Copy, KeyRound, X } from "lucide-react";
import { useEffect, useState } from "react";
import { CODE_NOTICE_MS, useCodeNotice } from "../lib/codeNotice";
import { useT } from "../lib/i18n";
import { copyText } from "../lib/platform";
import { useUi } from "../lib/store";
import { useUpdater } from "../lib/updater";
import { Button, cx, IconButton } from "./ui";

export function CodeNotice() {
  const t = useT();
  const current = useCodeNotice((s) => s.current);
  const dismiss = useCodeNotice((s) => s.dismiss);
  const [copied, setCopied] = useState(false);
  // 「发现新版本」的提示也在右下角：它在的时候，验证码排在它上面
  const updateShown = useUpdater(
    (s) => s.phase === "available" && s.version !== null && s.version !== s.seen && !s.dialogOpen,
  );

  useEffect(() => {
    setCopied(false);
    if (!current) return;
    const timer = setTimeout(dismiss, CODE_NOTICE_MS);
    return () => clearTimeout(timer);
  }, [current, dismiss]);

  if (!current) return null;

  const open = () => {
    const ui = useUi.getState();
    ui.setSpace("mail");
    ui.setScope({ kind: "account", accountId: current.accountId });
    ui.open(current.id);
    dismiss();
  };
  const copy = () => {
    void copyText(current.code).then(() => setCopied(true));
  };

  return (
    <div
      role="status"
      aria-live="polite"
      className={cx(
        "anim-pop fixed right-5 z-40 flex w-[300px] gap-3 rounded-lg bg-surface p-3.5 shadow-pop",
        updateShown ? "bottom-[150px]" : "bottom-5",
      )}
    >
      <KeyRound size={18} strokeWidth={2} className="mt-px shrink-0 text-accent" />
      <div className="min-w-0 flex-1">
        <p className="text-[12px] text-fg-3">{t("codeNotice.title")}</p>
        <p className="tabular mt-0.5 select-text break-all text-[26px] font-semibold leading-tight tracking-[0.1em] text-fg">
          {current.code}
        </p>
        <button
          type="button"
          onClick={open}
          title={t("codeNotice.open")}
          className="mt-1 block max-w-full truncate text-left text-[12px] text-fg-3 underline-offset-2 hover:text-accent hover:underline"
        >
          {[current.from, current.subject].filter(Boolean).join(" · ")}
        </button>
        <div className="mt-3 flex gap-2">
          <Button variant="primary" onClick={copy}>
            {copied ? <Check size={13} /> : <Copy size={13} />}
            {copied ? t("codeNotice.copied") : t("codeNotice.copy")}
          </Button>
          <Button variant="ghost" onClick={open}>
            {t("codeNotice.open")}
          </Button>
        </div>
      </div>
      <IconButton label={t("action.close")} onClick={dismiss} className="-mr-1 -mt-1 size-6 shrink-0">
        <X size={14} strokeWidth={1.75} />
      </IconButton>
    </div>
  );
}
