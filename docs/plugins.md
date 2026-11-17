# Writing Postflow plugins

[中文版](plugins.zh-CN.md)

Plugins add buttons, menu items and behavior to Postflow. A plugin is a folder with a `manifest.json` and a
JavaScript module. It runs inside Postflow's interface and talks to the app through an `api` object.

- [Quick start](#quick-start)
- [Examples](#examples)
- [Folder layout and manifest](#folder-layout-and-manifest)
- [Lifecycle](#lifecycle)
- [API reference](#api-reference)
- [Core methods you can call](#core-methods-you-can-call)
- [Data types](#data-types)
- [Debugging](#debugging)
- [Security and good manners](#security-and-good-manners)
- [Versioning](#versioning)

## Quick start

1. In Postflow, open **Settings → Plugins → Open plugins folder**. (In the web version, the folder is
   `plugins` inside the app's local data folder, shown at the bottom of that page.)
2. Create a folder named after your plugin's ID, for example `hello`, with two files:

   `manifest.json`

   ```json
   {
     "id": "hello",
     "name": "Hello",
     "version": "1.0.0",
     "description": "Says hello",
     "author": "You",
     "minApiVersion": 1
   }
   ```

   `main.js`

   ```js
   export default {
     onload(api) {
       api.ui.addToolbarButton({
         id: "hello",
         toolbar: "reader",
         label: "Say hello",
         onClick: ({ item }) => api.ui.toast(item ? `Hello, ${item.title}` : "Hello!"),
       });
     },
   };
   ```

3. Back in **Settings → Plugins**, choose **Rescan**, then turn the plugin on.
   A puzzle-piece button appears in the reader toolbar.

## Examples

Four complete plugins live in [`examples/plugins`](../examples/plugins). They also ship with Postflow: the first time
Postflow starts on a device, it puts them in the plugins folder, turned off. Turn one on in **Settings → Plugins**.
If you delete one, reinstall it from **Example plugins** on the same page. You can also copy a
plugin's folder into your plugins folder, choose **Rescan** and turn it on.

| Plugin | What it does | What it shows |
| --- | --- | --- |
| [`copy-code`](../examples/plugins/copy-code) | Copies the verification code from the open message; "Copy title and link" in the list's right-click menu; a notice when new mail carries a code. | A reader toolbar button with an SVG icon, `api.items.get`, a right-click item with `when`, the `"new-mail"` event. |
| [`important-senders`](../examples/plugins/important-senders) | Right-click mail to mark its sender as important; new unread mail from those senders is starred automatically. | Per-plugin storage, `"items-changed"` with a debounce, `api.items.list` with a filter, `api.items.set`, `api.onUnload`. |
| [`daily-digest`](../examples/plugins/daily-digest) | Copies today's unread items as Markdown, grouped by feed or mailbox; copies the open item as a Markdown note. | Buttons in both toolbars, `api.items.list` with `view: "today"`, the `"item-open"` event, storage that resets daily. |
| [`textrank-zh`](../examples/plugins/textrank-zh) | Above Chinese articles and mail, shows keywords and the three sentences that best sum it up, computed offline with TextRank. A reader button turns it on and off. | `api.ui.addReaderSection`, `api.ui.addStyle`, adding and removing a section to toggle it, bundling a library ([textrank4zh-ts](https://github.com/ChasLui/textrank4zh-ts), MIT) into one file. |

## Folder layout and manifest

```
plugins/
  hello/
    manifest.json
    main.js
```

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Lowercase letters, digits and `-`. Must match the folder name. |
| `name` | yes | Shown in Settings. |
| `version` | no | Your plugin's version, shown in Settings. |
| `description` | no | One line, shown in Settings. |
| `author` | no | Shown in Settings. |
| `url` | no | Where people can find the plugin. |
| `main` | no | The entry file, relative to the folder. Defaults to `main.js`. |
| `minApiVersion` | no | The lowest plugin API version the plugin needs. Defaults to `1`. |

`main.js` is loaded as an **ES module** and can be up to 4 MB. It can't `import` other files from the
plugin folder, so bundle everything into one file if you use a build step (esbuild, Rollup, Vite library
mode, all with `format: "esm"`). Libraries have to be bundled in too: the desktop app doesn't let pages load
scripts from other websites, so `import()` from a URL fails there. [`textrank-zh`](../examples/plugins/textrank-zh)
shows how: its `src/main.js` imports a library and a JSON file, and `npm run build:plugins` bundles them into
one `main.js` with [Rolldown](https://rolldown.rs/).

## Lifecycle

`main.js` must `export default` one of these:

```js
// An object with onload, and optionally onunload
export default {
  async onload(api) { /* set things up */ },
  onunload() { /* optional extra cleanup */ },
};

// Or a function; it may return a cleanup function
export default function (api) {
  const timer = setInterval(check, 60_000);
  return () => clearInterval(timer);
}
```

- A plugin is loaded when the user turns it on, and on every start of Postflow while it's on.
- It's unloaded when the user turns it off, reloads it (the ↻ button in Settings), or rescans.
- Anything registered through `api` (buttons, menu items, styles, event handlers) is removed automatically
  on unload. Clean up anything else (timers, DOM you added yourself) in `onunload`, with
  `api.onUnload(fn)`, or in the returned function.
- If `onload` throws, the error is shown under the plugin in Settings and everything it registered is removed.

## API reference

### `api.apiVersion`, `api.manifest`, `api.lang()`

- `apiVersion`: the plugin API version of this Postflow (currently `1`).
- `manifest`: your manifest, as read by Postflow.
- `lang()`: `"zh"` or `"en"`, the interface language (an imported language reports its base language).

### `api.ui`

| Method | What it does |
| --- | --- |
| `addToolbarButton({ id, toolbar, label, icon?, onClick })` | Adds a button to the `"list"` or `"reader"` toolbar. It starts at the end of the top toolbar; users can move it to the status bar or hide it in **Customize toolbar**. `icon` is an SVG string drawn with `currentColor` (24×24 viewBox); without it a puzzle piece is shown. `onClick({ item, selection })` gets the open item (or `null`) and the IDs selected in the list. Returns a function that removes the button. |
| `addItemMenuItem({ id, label, when?, onClick })` | Adds an item to the end of the list's right-click menu. `onClick(items)` gets the items the menu is for (the right-clicked one, or the whole selection). `when(items)` decides whether to show it. Returns a remover. |
| `addReaderSection({ id, when?, render })` | Adds a block above the body in the reader, for articles and mail. For every opened item, `render(item, el)` gets the full `ItemDetail` and an empty element to fill with plain DOM; it may be async and may return a cleanup function. Postflow calls it again when another item is opened or full-text mode changes, after running the cleanup and discarding the old element, so a slow render that finishes late never shows up on the wrong item. `when(item)` decides whether to show it. Returns a remover; removing and adding the section again is the way to toggle it. Use the theme's CSS variables (`--fg`, `--fg-2`, `--fg-3`, `--line`, `--accent`, `--accent-soft`, `--hover`) so it fits light and dark mode. |
| `addStyle(css)` | Adds a `<style>` to the page. Returns a remover. |
| `toast(message)` | Shows a short message at the bottom of the window. |

Errors thrown (or promises rejected) by `onClick` are shown as a toast.

### `api.items`

| Method | Returns |
| --- | --- |
| `list(filter, limit = 50)` | `{ items: Item[] }`, newest first. See [ItemFilter](#data-types); `filter.query` searches full text. |
| `get(id)` | `ItemDetail`: the item with its content (articles) or `mail` (messages). |
| `set(ids, patch)` | Changes `read`, `starred` or `archived` (`patch` has any of them). Mail is updated on the server too. |
| `label(ids, label)` | Files items in a category (`null` clears it). |
| `open(id)` | Opens an item in the reader. |
| `openedId()` | ID of the item open in the reader, or `null`. |
| `selection()` | IDs selected in the list. |

### `api.events`

`api.events.on(name, handler)` returns a function that stops listening.

| Event | Payload |
| --- | --- |
| `"new-mail"` | `{ account_id, mails: [{ id, from, subject, date }], total }`. Sent after new mail has gone through the user's rules (Settings → Rules). Only recent mail; mail caught up after days offline isn't announced. |
| `"items-changed"` | `{ scope: string[] }`. Items, feeds or settings changed (on this or another device). |
| `"item-open"` | `{ id }`, the item opened in the reader (`null` when it's closed). |
| `"core"` | Every event from Postflow's core, unfiltered. |

### `api.storage`

Per-plugin storage on this device (not synced). Values must be JSON-serializable.

`get(key)` → value or `null` · `set(key, value)` · `remove(key)`

### `api.rpc(method, params)` and `api.onUnload(fn)`

- `rpc` calls a method of Postflow's core directly and returns a promise. See below.
- `onUnload(fn)` registers extra cleanup to run when the plugin is unloaded.

## Core methods you can call

These are the methods the interface itself uses. The common ones are stable within plugin API version 1;
others may change between releases.

| Method | Params | Result |
| --- | --- | --- |
| `nav.get` | `{ today_since: number }` | Feeds, mailboxes, categories and unread counts |
| `items.list` | `{ filter, before?, limit? }` | `{ items, next }` |
| `items.get` | `{ id }` | `ItemDetail` |
| `items.set` | `{ ids, read?, starred?, archived? }` | `{}` |
| `items.label` | `{ ids, label }` | `{}` |
| `items.mark_all_read` | `{ filter }` | `{ count }` |
| `feeds.add` | `{ url, folder, title? }` | The new feed |
| `feeds.refresh` | `{ id? }` | `{ count }` |
| `settings.get` | `{}` | All synced settings |
| `rules.test` / `rules.apply` | `{ rule }` / `{ rule? }` | How many items a rule matches / applies it ([format](../crates/core/src/rules.rs)) |
| `translate.items` | `{ ids, to }` | Translations |

## Data types

```ts
interface Item {
  id: string;
  kind: "article" | "mail";
  source_id: string;          // feed or mailbox ID
  source_title: string;       // feed or mailbox name
  source_icon: string | null;
  url: string | null;
  title: string;
  author: string | null;      // sender name for mail
  sender_address: string | null;
  summary: string;
  published_at: number;       // milliseconds since 1970
  read: boolean;
  starred: boolean;
  archived: boolean;
  draft: boolean;
  sent: boolean;
  label: string | null;       // category
}

interface ItemDetail extends Item {
  content?: string;           // article HTML (sanitized)
  full_text?: string | null;  // fetched full text, if any
  mail?: {
    body_kind: "html" | "text";
    body: string;             // full HTML document, or a safe HTML fragment for plain-text mail
    from: { name: string | null; address: string | null };
    to: { name: string | null; address: string | null }[];
    cc: { name: string | null; address: string | null }[];
    attachments: { index: number; name: string; size: number; mime: string }[];
  };
}

interface ItemFilter {
  view: "inbox" | "today" | "unread" | "starred" | "archived" | "all";
  kind?: "article" | "mail";
  source_id?: string;     // one feed or mailbox
  folder?: string;        // a feed folder
  label?: string;         // a category
  since?: number;         // only items published after this time (ms)
  query?: string;         // full-text search: every word must appear
}
```

## Debugging

- **Desktop app:** run Postflow from source with `npm run app` to get developer tools (right-click with
  Shift held, then Inspect). Plugin errors are logged to the console as `[plugin <id>]`.
- **Web version:** open your browser's developer tools.
- After editing `main.js`, press ↻ next to the plugin in Settings to reload it. Rescan picks up new folders
  and manifest changes.
- If `onload` fails, the message appears under the plugin in Settings.

## Security and good manners

Plugins run with the same access as Postflow's own interface: they can read every message and article,
change them, and make network requests. That's why nothing loads until the user turns it on, per device.

- Don't send mail content anywhere without saying so clearly in your description.
- Keep work out of the main thread: no long synchronous loops.
- Don't rely on Postflow's internal CSS class names or DOM structure; they change without notice.
  Use `api.ui` instead, and keep `addStyle` rules narrow.
- Clean up after yourself (see [Lifecycle](#lifecycle)).

## Versioning

`api.apiVersion` goes up only when something changes in a way that breaks existing plugins. New methods
and events can be added within a version. Set `minApiVersion` to the version you need; Postflow refuses to
load a plugin that needs a newer API and says so in Settings.

