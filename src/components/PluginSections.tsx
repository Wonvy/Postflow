// 插件加在阅读区正文上方的内容（ui.addReaderSection）。每块是一个空的容器，插件自己往里画。
// 换了条目、切换全文、插件停用时，先调插件给的清理函数，再清空容器重新画。

import { useEffect, useRef } from "react";
import type { ItemDetail } from "../lib/api";
import { type ReaderSection, usePlugins } from "../lib/plugins";

export function PluginSections({ item, mode }: { item: ItemDetail; mode?: string }) {
  const sections = usePlugins((s) => s.sections);
  const shown = sections.filter((section) => {
    try {
      return !section.when || section.when(item);
    } catch {
      return false;
    }
  });
  if (shown.length === 0) return null;
  return (
    <>
      {shown.map((section) => (
        <Section key={section.key} section={section} item={item} mode={mode} />
      ))}
    </>
  );
}

function Section({ section, item, mode }: { section: ReaderSection & { pluginId: string }; item: ItemDetail; mode?: string }) {
  const ref = useRef<HTMLDivElement>(null);
  // 正文变了（切换全文）才重新画，其他字段变（已读、星标）不重画
  const contentKey = `${item.id}:${mode ?? ""}:${item.full_text ? 1 : 0}`;
  const itemRef = useRef(item);
  itemRef.current = item;

  useEffect(() => {
    const host = ref.current;
    if (!host) return;
    // 每次画都给一个新的容器，清理时整个拿掉：插件慢慢算完时如果已经换了条目，
    // 它画进的是一个已经不在页面上的容器，不会叠到新内容上
    const el = document.createElement("div");
    host.replaceChildren(el);
    let cleanup: (() => void) | undefined;
    let alive = true;
    Promise.resolve()
      .then(() => section.render(itemRef.current, el))
      .then((result) => {
        if (typeof result !== "function") return;
        if (alive) cleanup = result;
        else result();
      })
      .catch((err) => {
        console.error(`[plugin ${section.pluginId}] reader section failed`, err);
        if (!alive) return;
        el.replaceChildren();
        const note = document.createElement("p");
        note.className = "text-[12px] text-danger";
        note.textContent = `${section.pluginId}: ${err instanceof Error ? err.message : String(err)}`;
        el.append(note);
      });
    return () => {
      alive = false;
      try {
        cleanup?.();
      } catch (err) {
        console.error(`[plugin ${section.pluginId}] reader section cleanup failed`, err);
      }
      el.remove();
    };
  }, [section, contentKey]);

  return <div ref={ref} data-plugin={section.pluginId} className="plugin-section mb-6 [&:not(:has(*:not(:empty)))]:hidden" />;
}
