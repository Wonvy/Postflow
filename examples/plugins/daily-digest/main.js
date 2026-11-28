// Example ShibaPost plugin: daily digest. See docs/plugins.md for the full API.
//
// - List toolbar button "Copy today's digest": today's unread items, grouped by
//   feed or mailbox, copied as Markdown (handy for notes apps or a team chat).
// - Reader toolbar button "Copy as Markdown": the open item as a short note.
// - Counts how many items you opened today, using the "item-open" event.
//
// Shows: buttons in both toolbars, api.items.list with a filter, api.items.get,
// the "item-open" event, and storage that resets every day.

const DIGEST_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M4 5h16M4 10h10M4 15h16M4 20h8"/></svg>`;
const NOTE_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><path d="M14 3v6h6"/><path d="M8 13h8M8 17h5"/></svg>`;

function startOfToday() {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

function today() {
  return new Date().toISOString().slice(0, 10);
}

/** Markdown-safe text for a link label or list item. */
function md(text) {
  return String(text ?? "").replace(/([\\[\]*_`])/g, "\\$1").replace(/\s+/g, " ").trim();
}

function line(item) {
  const title = md(item.title) || "(untitled)";
  if (item.kind === "mail") return `- ${md(item.author ?? item.sender_address ?? "")}: ${title}`;
  return item.url ? `- [${title}](${item.url})` : `- ${title}`;
}

function plainText(html) {
  return new DOMParser().parseFromString(html ?? "", "text/html").body.textContent?.trim() ?? "";
}

export default {
  onload(api) {
    const zh = () => api.lang() === "zh";
    const text = (cn, en) => (zh() ? cn : en);

    // "Opened today": reset when the day changes.
    const opened = () => {
      const saved = api.storage.get("opened");
      return saved && saved.day === today() ? saved : { day: today(), ids: [] };
    };
    api.events.on("item-open", ({ id }) => {
      if (!id) return;
      const state = opened();
      if (!state.ids.includes(id)) state.ids.push(id);
      api.storage.set("opened", state);
    });

    api.ui.addToolbarButton({
      id: "digest",
      toolbar: "list",
      label: text("复制今日摘要", "Copy today's digest"),
      icon: DIGEST_ICON,
      async onClick() {
        const { items } = await api.items.list({ view: "today", since: startOfToday() }, 300);
        const unread = items.filter((i) => !i.read);
        if (unread.length === 0) {
          api.ui.toast(text("今天没有未读的内容", "Nothing unread today"));
          return;
        }
        // Group by feed or mailbox, keeping the newest-first order inside each group.
        const groups = new Map();
        for (const item of unread) {
          const key = item.source_title || text("其他", "Other");
          if (!groups.has(key)) groups.set(key, []);
          groups.get(key).push(item);
        }
        const header = text(
          `# ${today()} 摘要\n\n未读 ${unread.length} 条，今天已打开 ${opened().ids.length} 条。`,
          `# Digest for ${today()}\n\n${unread.length} unread, ${opened().ids.length} opened today.`,
        );
        const body = [...groups].map(([source, list]) => `## ${md(source)}\n\n${list.map(line).join("\n")}`);
        await navigator.clipboard.writeText([header, ...body].join("\n\n") + "\n");
        api.ui.toast(text(`已复制 ${unread.length} 条的摘要`, `Copied a digest of ${unread.length} items`));
      },
    });

    api.ui.addToolbarButton({
      id: "note",
      toolbar: "reader",
      label: text("复制为 Markdown", "Copy as Markdown"),
      icon: NOTE_ICON,
      async onClick({ item }) {
        if (!item) {
          api.ui.toast(text("先打开一条", "Open an item first"));
          return;
        }
        const detail = await api.items.get(item.id);
        const excerpt = plainText(detail.mail ? detail.mail.body : detail.summary || detail.content).slice(0, 280);
        const title = item.url ? `[${md(item.title)}](${item.url})` : md(item.title);
        const meta = [item.source_title, new Date(item.published_at).toLocaleString()].filter(Boolean).join(" · ");
        const note = `### ${title}\n\n${md(meta)}\n\n> ${md(excerpt)}${excerpt.length >= 280 ? "…" : ""}\n`;
        await navigator.clipboard.writeText(note);
        api.ui.toast(text("已复制为 Markdown", "Copied as Markdown"));
      },
    });
  },
};
