// 查看源代码：邮件的原始邮件（头和正文），或者文章在订阅源里原本的 XML。从条目的右键菜单打开。

import { useQuery } from "@tanstack/react-query";
import { Check, Copy } from "lucide-react";
import { useState } from "react";
import { api } from "../lib/api";
import { errorText, useT } from "../lib/i18n";
import { copyText } from "../lib/platform";
import { useUi } from "../lib/store";
import { Button, Modal, Spinner } from "./ui";

export function SourceDialog() {
  const t = useT();
  const dialog = useUi((s) => s.dialog);
  const setDialog = useUi((s) => s.setDialog);
  const open = dialog?.type === "source";
  const itemId = open ? dialog.itemId : null;
  const title = open ? dialog.title : "";
  const [copied, setCopied] = useState(false);

  const source = useQuery({
    queryKey: ["item-source", itemId],
    queryFn: () => api.itemSource(itemId ?? ""),
    enabled: itemId !== null,
    // 文章的 XML 要重新抓源，每次打开都取最新的
    staleTime: 0,
    gcTime: 0,
    retry: false,
  });
  const data = source.data;

  const copy = () => {
    if (!data) return;
    void copyText(data.source).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  return (
    <Modal open={open} onClose={() => setDialog(null)} title={title || t("source.xml")} width={820}>
      <div className="flex h-[min(620px,calc(76vh-44px))] flex-col gap-2 p-4">
        {source.isPending && (
          <p className="flex items-center gap-2 text-[13px] text-fg-3">
            <Spinner size={13} />
            {t("source.loading")}
          </p>
        )}
        {source.isError && (
          <p role="alert" className="select-text text-[13px] text-danger">
            {errorText(t, source.error)}
          </p>
        )}
        {data && (
          <>
            <div className="flex items-start gap-3">
              <p className="min-w-0 flex-1 select-text break-all text-[12px] leading-relaxed text-fg-3">
                {data.kind === "xml" && (data.whole ? t("source.wholeFeed") : t("source.fetched", { url: data.url ?? "" }))}
              </p>
              <Button onClick={copy} className="shrink-0">
                {copied ? <Check size={13} /> : <Copy size={13} />}
                {copied ? t("source.copied") : t("source.copy")}
              </Button>
            </div>
            <pre className="min-h-0 flex-1 select-text overflow-auto whitespace-pre-wrap break-all rounded-md bg-active p-3 font-mono text-[12px] leading-relaxed text-fg-2">
              {data.source}
            </pre>
          </>
        )}
      </div>
    </Modal>
  );
}
