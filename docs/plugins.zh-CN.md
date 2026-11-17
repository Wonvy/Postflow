# 开发 Postflow 插件

[English](plugins.md)

插件可以给 Postflow 加按钮、菜单项和功能。一个插件就是一个文件夹，里面有一个 `manifest.json` 和一个
JavaScript 模块。插件在 Postflow 的界面里运行，通过 `api` 对象和应用打交道。

- [快速上手](#快速上手)
- [例子](#例子)
- [文件夹结构和 manifest](#文件夹结构和-manifest)
- [生命周期](#生命周期)
- [接口参考](#接口参考)
- [可以调用的核心方法](#可以调用的核心方法)
- [数据类型](#数据类型)
- [调试](#调试)
- [安全和规矩](#安全和规矩)
- [版本](#版本)

## 快速上手

1. 在 Postflow 里打开「设置 → 插件 → 打开插件目录」。（网页版里，插件目录是应用本机数据目录下的 `plugins`，
   路径显示在那一页的最下面。）
2. 新建一个以插件 ID 命名的文件夹，比如 `hello`，放进两个文件：

   `manifest.json`

   ```json
   {
     "id": "hello",
     "name": "Hello",
     "version": "1.0.0",
     "description": "打个招呼",
     "author": "你",
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
         label: "打招呼",
         onClick: ({ item }) => api.ui.toast(item ? `你好，${item.title}` : "你好！"),
       });
     },
   };
   ```

3. 回到「设置 → 插件」，点「重新扫描」，再启用这个插件。阅读区的工具栏上会出现一个拼图图标的按钮。

## 例子

[`examples/plugins`](../examples/plugins) 里有四个完整的插件。它们也随 Postflow 一起打包：每台设备第一次启动 Postflow 时
会把它们放进插件目录，默认不启用，在「设置 → 插件」里打开开关就能用。删掉的可以在同一页的「示例插件」里重新安装。也可以把插件文件夹复制到你的插件目录，点「重新扫描」，再启用它。

| 插件 | 做什么 | 演示了哪些接口 |
| --- | --- | --- |
| [`copy-code`](../examples/plugins/copy-code) | 复制打开的邮件里的验证码；列表右键菜单里加「复制标题和链接」；收到含验证码的新邮件时提示。 | 带 SVG 图标的阅读区按钮、`api.items.get`、带 `when` 的右键菜单项、`"new-mail"` 事件。 |
| [`important-senders`](../examples/plugins/important-senders) | 在邮件上点右键把发件人设为重要发件人；之后这些发件人的未读新邮件自动加星标。 | 插件自己的存储、带防抖的 `"items-changed"`、带筛选条件的 `api.items.list`、`api.items.set`、`api.onUnload`。 |
| [`daily-digest`](../examples/plugins/daily-digest) | 把今天的未读条目按订阅源或邮箱分组，复制成 Markdown；把打开的条目复制成一条 Markdown 笔记。 | 两个工具栏上的按钮、`view: "today"` 的 `api.items.list`、`"item-open"` 事件、每天清零的存储。 |
| [`textrank-zh`](../examples/plugins/textrank-zh) | 在中文文章和邮件的正文上方显示关键词，以及最能概括全文的三句话，用 TextRank 离线计算。阅读区有个按钮可以打开、关闭。 | `api.ui.addReaderSection`、`api.ui.addStyle`、用添加和移除来开关一块内容、把第三方库（[textrank4zh-ts](https://github.com/ChasLui/textrank4zh-ts)，MIT）打包成一个文件。 |

## 文件夹结构和 manifest

```
plugins/
  hello/
    manifest.json
    main.js
```

| 字段 | 必填 | 含义 |
| --- | --- | --- |
| `id` | 是 | 小写字母、数字和 `-`，必须和文件夹同名。 |
| `name` | 是 | 显示在设置里的名字。 |
| `version` | 否 | 插件自己的版本号，显示在设置里。 |
| `description` | 否 | 一句话介绍，显示在设置里。 |
| `author` | 否 | 作者，显示在设置里。 |
| `url` | 否 | 插件的主页或下载地址。 |
| `main` | 否 | 入口文件，相对于插件文件夹，默认是 `main.js`。 |
| `minApiVersion` | 否 | 插件需要的最低插件接口版本，默认是 `1`。 |

`main.js` 按 **ES 模块** 加载，最大 4 MB。它不能 `import` 插件文件夹里的其他文件，所以如果用了构建工具，
请把所有代码打包成一个文件（esbuild、Rollup、Vite 的库模式都可以，格式选 `format: "esm"`）。
第三方库也要一起打包进去：桌面应用不允许页面从别的网站加载脚本，所以在那里用 `import()` 从网址加载会失败。
[`textrank-zh`](../examples/plugins/textrank-zh) 是一个例子：它的 `src/main.js` 引入了一个库和一个 JSON 文件，
`npm run build:plugins` 用 [Rolldown](https://rolldown.rs/) 把它们打包成一个 `main.js`。

## 生命周期

`main.js` 必须 `export default` 下面两种之一：

```js
// 一个带 onload 的对象，onunload 可选
export default {
  async onload(api) { /* 初始化 */ },
  onunload() { /* 可选的额外清理 */ },
};

// 或者一个函数，可以返回一个清理函数
export default function (api) {
  const timer = setInterval(check, 60_000);
  return () => clearInterval(timer);
}
```

- 用户启用插件时加载；插件开着的时候，每次启动 Postflow 都会加载。
- 用户停用插件、重新加载它（设置里的 ↻ 按钮）或者重新扫描时卸载。
- 通过 `api` 登记的东西（按钮、菜单项、样式、事件监听）卸载时自动撤掉。别的东西（定时器、自己加的 DOM）
  要自己清理：写在 `onunload` 里、用 `api.onUnload(fn)` 登记，或者放在返回的清理函数里。
- `onload` 抛出错误时，错误信息显示在设置里这个插件的下面，它已经登记的东西全部撤掉。

## 接口参考

### `api.apiVersion`、`api.manifest`、`api.lang()`

- `apiVersion`：这个 Postflow 的插件接口版本（现在是 `1`）。
- `manifest`：Postflow 读到的你的 manifest。
- `lang()`：界面语言，`"zh"` 或 `"en"`（导入的语言返回它的基础语言）。

### `api.ui`

| 方法 | 作用 |
| --- | --- |
| `addToolbarButton({ id, toolbar, label, icon?, onClick })` | 在 `"list"`（列表）或 `"reader"`（阅读区）工具栏加一个按钮。默认放在顶部工具栏的最后，用户可以在「自定义工具栏」里把它挪到状态栏或隐藏。`icon` 是用 `currentColor` 画的 SVG 字符串（viewBox 24×24），不给就显示拼图图标。`onClick({ item, selection })` 收到阅读区打开的条目（没有就是 `null`）和列表里选中的条目 ID。返回一个移除这个按钮的函数。 |
| `addItemMenuItem({ id, label, when?, onClick })` | 在列表右键菜单的最后加一项。`onClick(items)` 收到这个菜单针对的条目（右键点的那一条，或者整个选中的范围）。`when(items)` 决定显不显示。返回一个移除函数。 |
| `addReaderSection({ id, when?, render })` | 在阅读区正文上方加一块内容，文章和邮件都有。每打开一个条目，`render(item, el)` 会收到完整的 `ItemDetail` 和一个空元素，用普通 DOM 往里画；可以是 async，也可以返回一个清理函数。打开别的条目、切换全文模式时，Postflow 先调清理函数、丢掉旧的元素，再重新调用，所以算得慢、晚到的结果不会出现在别的条目上。`when(item)` 决定显不显示。返回一个移除函数；要开关这块内容，就移除后再重新添加。样式请用主题的 CSS 变量（`--fg`、`--fg-2`、`--fg-3`、`--line`、`--accent`、`--accent-soft`、`--hover`），浅色和深色模式下都合适。 |
| `addStyle(css)` | 往页面里加一段 `<style>`。返回一个移除函数。 |
| `toast(message)` | 在窗口底部显示一句很快消失的提示。 |

`onClick` 抛出的错误（或者被拒绝的 Promise）会以提示的形式显示出来。

### `api.items`

| 方法 | 返回 |
| --- | --- |
| `list(filter, limit = 50)` | `{ items: Item[] }`，新的在前。筛选条件见 [ItemFilter](#数据类型)；`filter.query` 是全文搜索。 |
| `get(id)` | `ItemDetail`：带正文（文章）或 `mail`（邮件）的条目。 |
| `set(ids, patch)` | 改 `read`（已读）、`starred`（星标）或 `archived`（归档），`patch` 里给其中任意几项。邮件会同步改到服务器上。 |
| `label(ids, label)` | 把条目分到一个类别（`null` 是取消分类）。 |
| `open(id)` | 在阅读区打开一个条目。 |
| `openedId()` | 阅读区打开的条目 ID，没有就是 `null`。 |
| `selection()` | 列表里选中的条目 ID。 |

### `api.events`

`api.events.on(name, handler)` 返回一个取消监听的函数。

| 事件 | 内容 |
| --- | --- |
| `"new-mail"` | `{ account_id, mails: [{ id, from, subject, date }], total }`。新邮件经过用户的自动规则（设置 → 自动规则）处理之后发出。只包括最近的邮件；离线几天后补收的旧邮件不会触发。 |
| `"items-changed"` | `{ scope: string[] }`。条目、订阅或设置有了变化（在这台设备上，或者从其他设备同步过来）。 |
| `"item-open"` | `{ id }`，阅读区打开的条目（关掉时是 `null`）。 |
| `"core"` | Postflow 核心发出的所有事件，不做筛选。 |

### `api.storage`

插件自己的存储，只在这台设备上，不同步。值必须能转成 JSON。

`get(key)` → 值或 `null` · `set(key, value)` · `remove(key)`

### `api.rpc(method, params)` 和 `api.onUnload(fn)`

- `rpc` 直接调用 Postflow 核心的方法，返回一个 Promise。见下一节。
- `onUnload(fn)` 登记一个插件卸载时要做的额外清理。

## 可以调用的核心方法

这些是界面本身在用的方法。常用的这些在插件接口版本 1 里保持稳定；其他方法可能随版本变化。

| 方法 | 参数 | 结果 |
| --- | --- | --- |
| `nav.get` | `{ today_since: number }` | 订阅源、邮箱、类别和未读数 |
| `items.list` | `{ filter, before?, limit? }` | `{ items, next }` |
| `items.get` | `{ id }` | `ItemDetail` |
| `items.set` | `{ ids, read?, starred?, archived? }` | `{}` |
| `items.label` | `{ ids, label }` | `{}` |
| `items.mark_all_read` | `{ filter }` | `{ count }` |
| `feeds.add` | `{ url, folder, title? }` | 新加的订阅源 |
| `feeds.refresh` | `{ id? }` | `{ count }` |
| `settings.get` | `{}` | 所有同步的设置 |
| `rules.test` / `rules.apply` | `{ rule }` / `{ rule? }` | 一条规则会命中多少条 / 应用规则（[格式](../crates/core/src/rules.rs)） |
| `translate.items` | `{ ids, to }` | 译文 |

## 数据类型

```ts
interface Item {
  id: string;
  kind: "article" | "mail";
  source_id: string;          // 订阅源或邮箱的 ID
  source_title: string;       // 订阅源或邮箱的名字
  source_icon: string | null;
  url: string | null;
  title: string;
  author: string | null;      // 邮件是发件人的名字
  sender_address: string | null;
  summary: string;
  published_at: number;       // 1970 年以来的毫秒数
  read: boolean;
  starred: boolean;
  archived: boolean;
  draft: boolean;
  sent: boolean;
  label: string | null;       // 类别
}

interface ItemDetail extends Item {
  content?: string;           // 文章的 HTML（已经过安全处理）
  full_text?: string | null;  // 抓取到的全文，如果有
  mail?: {
    body_kind: "html" | "text";
    body: string;             // 完整的 HTML 文档；纯文本邮件是一段安全的 HTML 片段
    from: { name: string | null; address: string | null };
    to: { name: string | null; address: string | null }[];
    cc: { name: string | null; address: string | null }[];
    attachments: { index: number; name: string; size: number; mime: string }[];
  };
}

interface ItemFilter {
  view: "inbox" | "today" | "unread" | "starred" | "archived" | "all";
  kind?: "article" | "mail";
  source_id?: string;     // 某一个订阅源或邮箱
  folder?: string;        // 某一个订阅文件夹
  label?: string;         // 某一个类别
  since?: number;         // 只要这个时间之后发布的（毫秒）
  query?: string;         // 全文搜索：每个词都要出现
}
```

## 调试

- **桌面应用：** 用 `npm run app` 从源码运行 Postflow，就有开发者工具（按住 Shift 点右键，再选「检查」）。
  插件的错误会以 `[plugin <id>]` 开头打印在控制台里。
- **网页版：** 打开浏览器的开发者工具。
- 改完 `main.js` 后，点设置里插件旁边的 ↻ 重新加载。新加的插件文件夹和改过的 manifest 要点「重新扫描」。
- `onload` 出错时，错误信息显示在设置里这个插件的下面。

## 安全和规矩

插件和 Postflow 自己的界面有同样的权限：能读每一封邮件和每一篇文章、能修改它们，也能发网络请求。
所以插件在每台设备上都要用户手动启用才会加载。

- 不要把邮件内容发到任何地方，除非在介绍里写清楚。
- 别占着主线程：不要写长时间的同步循环。
- 不要依赖 Postflow 内部的 CSS 类名和 DOM 结构，它们会随时改变。请用 `api.ui`，`addStyle` 的规则写得越窄越好。
- 自己的东西自己清理（见[生命周期](#生命周期)）。

## 版本

只有在改动会让现有插件出问题时，`api.apiVersion` 才会加一。同一个版本里可以增加新的方法和事件。
把 `minApiVersion` 设成你需要的版本；插件需要的接口版本比当前 Postflow 新时，Postflow 不会加载它，并在设置里说明原因。
