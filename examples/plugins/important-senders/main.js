// Example ShibaPost plugin: important senders. See docs/plugins.md for the full API.
//
// - Right-click mail → "Mark sender as important" / "Not important any more".
// - New unread mail in the inbox from an important sender is starred automatically.
// - The list toolbar gets a button that shows who's on the list.
//
// Shows: right-click menu items with `when`, per-plugin storage, reacting to
// "items-changed" with a debounce, and changing items with api.items.set.

const ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="8" r="4"/><path d="M2 21a7 7 0 0 1 11.2-5.6"/><path d="m18 13 1.2 2.6 2.8.3-2.1 1.9.6 2.8-2.5-1.5-2.5 1.5.6-2.8-2.1-1.9 2.8-.3z"/></svg>`;

/** How many handled message IDs to remember, so a message you un-star stays un-starred. */
const REMEMBER = 500;
/** Look at this many of the newest inbox messages each time. */
const LOOK_AT = 50;
/** Only mail from the last two days counts as new. */
const RECENT_MS = 2 * 24 * 60 * 60 * 1000;

export default {
  onload(api) {
    const zh = () => api.lang() === "zh";
    const text = (cn, en) => (zh() ? cn : en);

    const senders = () => new Set(api.storage.get("senders") ?? []);
    const saveSenders = (set) => api.storage.set("senders", [...set].sort());
    const addressOf = (item) => (item.sender_address ?? "").trim().toLowerCase();

    api.ui.addItemMenuItem({
      id: "add",
      label: text("设为重要发件人", "Mark sender as important"),
      when: (items) =>
        items.every((i) => i.kind === "mail" && addressOf(i)) && items.some((i) => !senders().has(addressOf(i))),
      onClick(items) {
        const set = senders();
        for (const item of items) set.add(addressOf(item));
        saveSenders(set);
        api.ui.toast(text(`已加入：${[...new Set(items.map(addressOf))].join("、")}`, `Added ${items.length} sender(s)`));
        void starNew();
      },
    });

    api.ui.addItemMenuItem({
      id: "remove",
      label: text("不再是重要发件人", "Not important any more"),
      when: (items) => items.some((i) => senders().has(addressOf(i))),
      onClick(items) {
        const set = senders();
        for (const item of items) set.delete(addressOf(item));
        saveSenders(set);
        api.ui.toast(text("已移出重要发件人", "Removed from important senders"));
      },
    });

    api.ui.addToolbarButton({
      id: "list",
      toolbar: "list",
      label: text("重要发件人", "Important senders"),
      icon: ICON,
      onClick() {
        const list = [...senders()];
        api.ui.toast(
          list.length === 0
            ? text("还没有重要发件人。在邮件上点右键添加。", "No important senders yet. Right-click a message to add one.")
            : text(`重要发件人：${list.join("、")}`, `Important senders: ${list.join(", ")}`),
        );
      },
    });

    /** Star recent unread inbox mail from important senders that we haven't handled yet. */
    async function starNew() {
      const vip = senders();
      if (vip.size === 0) return;
      const handled = new Set(api.storage.get("handled") ?? []);
      const since = Date.now() - RECENT_MS;
      const { items } = await api.items.list({ view: "inbox", kind: "mail", since }, LOOK_AT);
      const fresh = items.filter((i) => !handled.has(i.id) && i.published_at >= since);
      const toStar = fresh.filter((i) => !i.read && !i.starred && vip.has(addressOf(i)));
      for (const item of fresh) handled.add(item.id);
      api.storage.set("handled", [...handled].slice(-REMEMBER));
      if (toStar.length === 0) return;
      await api.items.set(
        toStar.map((i) => i.id),
        { starred: true },
      );
      api.ui.toast(
        text(`给 ${toStar.length} 封重要发件人的邮件加了星标`, `Starred ${toStar.length} message(s) from important senders`),
      );
    }

    // Items change often (and in bursts); wait until things settle.
    let timer;
    api.events.on("items-changed", () => {
      clearTimeout(timer);
      timer = setTimeout(() => void starNew().catch((err) => console.error("[important-senders]", err)), 1500);
    });
    api.onUnload(() => clearTimeout(timer));

    void starNew();
  },
};
