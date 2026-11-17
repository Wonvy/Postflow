// 随应用打包的说明文档（docs/*.md），在应用里直接看：打不开 GitHub 的网络也能用。
// 去掉只在网页上有用的语言切换和目录，底部可以去 GitHub 看最新版。

import { ExternalLink } from "lucide-react";
import { useT } from "../lib/i18n";
import { openExternal } from "../lib/platform";
import { Markdown } from "./Markdown";
import { Modal } from "./ui";

export function GuideModal({
  open,
  onClose,
  title,
  text,
  onlineUrl,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  /** 文档的 Markdown 原文。 */
  text: string;
  /** GitHub 上的最新版。 */
  onlineUrl: string;
}) {
  const t = useT();
  const shown = text
    .split("\n")
    .filter((line) => !/^\[(English|中文版)\]\(/.test(line.trim()) && !/^- \[[^\]]+\]\(#/.test(line.trim()))
    .join("\n");
  return (
    <Modal open={open} onClose={onClose} title={title} width={680}>
      <div className="px-5 py-4">
        <Markdown text={shown} wrapped className="select-text text-[13px] leading-relaxed text-fg-2" />
        <button
          type="button"
          onClick={() => void openExternal(onlineUrl)}
          className="mt-4 flex items-center gap-1 text-[12px] text-fg-3 hover:text-fg-2"
        >
          <ExternalLink size={12} />
          {t("guide.online")}
        </button>
      </div>
    </Modal>
  );
}
