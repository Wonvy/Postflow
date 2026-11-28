// Example ShibaPost plugin. See docs/plugins.md for the full API.
//
// - Reader toolbar button "Copy code": finds the verification code in the open
//   message and copies it.
// - List right-click item "Copy title and link".
// - A notice when new mail looks like it carries a code.

const ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="8" y="8" width="13" height="13" rx="2"/><path d="M4 16V5a1 1 0 0 1 1-1h11"/><path d="M11.5 14.5h1m2 0h1m2 0h1"/></svg>`;

const NEAR_KEYWORD =
  /(验证码|校验码|动态码|verification code|security code|one-time code|code)[^\d]{0,24}(\d{4,8})/i;
const ANY_CODE = /(?<!\d)(\d{4,8})(?!\d)/;

/** The code in a piece of text: next to a keyword if possible, otherwise the first 4–8 digit number. */
function findCode(text) {
  const near = text.match(NEAR_KEYWORD);
  if (near) return near[2];
  const any = text.match(ANY_CODE);
  return any ? any[1] : null;
}

function textOf(html) {
  return new DOMParser().parseFromString(html, "text/html").body.textContent ?? "";
}

export default {
  onload(api) {
    const zh = () => api.lang() === "zh";

    api.ui.addToolbarButton({
      id: "copy",
      toolbar: "reader",
      label: zh() ? "复制验证码" : "Copy code",
      icon: ICON,
      async onClick({ item }) {
        if (!item) {
          api.ui.toast(zh() ? "先打开一封邮件" : "Open a message first");
          return;
        }
        const detail = await api.items.get(item.id);
        const body = detail.mail ? detail.mail.body : (detail.content ?? "");
        const code = findCode(`${item.title}\n${textOf(body)}`);
        if (!code) {
          api.ui.toast(zh() ? "这封邮件里没找到验证码" : "No code found in this message");
          return;
        }
        await navigator.clipboard.writeText(code);
        api.storage.set("copied", (api.storage.get("copied") ?? 0) + 1);
        api.ui.toast(zh() ? `已复制验证码 ${code}` : `Copied code ${code}`);
      },
    });

    api.ui.addItemMenuItem({
      id: "copy-link",
      label: zh() ? "复制标题和链接" : "Copy title and link",
      when: (items) => items.length === 1,
      async onClick([item]) {
        await navigator.clipboard.writeText(item.url ? `${item.title}\n${item.url}` : item.title);
        api.ui.toast(zh() ? "已复制" : "Copied");
      },
    });

    api.events.on("new-mail", (event) => {
      const withCode = event.mails.find((m) => NEAR_KEYWORD.test(m.subject));
      if (withCode) {
        api.ui.toast(zh() ? `${withCode.from} 发来了验证码` : `${withCode.from} sent a code`);
      }
    });
  },

  onunload() {
    // Everything registered through `api` is removed automatically.
  },
};
