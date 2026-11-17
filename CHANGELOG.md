# Changelog

What changed in each Postflow release, newest first. Every version has an English and a Chinese list; the update
window in the app shows the one for your language. Add new entries under **Unreleased**; `npm run release` turns
that heading into the version number and date.

## Unreleased

### English

- When a feed moves, change its address in the feed's Edit dialog. Postflow checks the new address first and keeps
  the same feed, with its read and starred states and earlier items. The warning sign next to a feed means its last
  refresh failed.
- The update window shows what's new right inside the app, and checking for updates in Settings opens it straight
  away.
- Full-text search: the magnifier in the list toolbar, `/` or `⌘F`. It searches titles, senders, authors and the full
  text of articles and mail, Chinese included, in the current list or everywhere, and highlights what it found.

### 中文

- 订阅源换了地址时，可以在它的「编辑」里改订阅地址。Postflow 先检查新地址能用才换，还是同一个订阅源，
  已读、星标和以前的文章都保留。订阅源旁边的黄色三角表示上次刷新失败。
- 更新窗口里直接显示更新说明；在设置里点「检查更新」，发现新版本时直接打开这个窗口。
- 全文搜索：列表工具栏的放大镜、`/` 或 `⌘F`。搜标题、发件人、作者和文章、邮件的正文，中文按词也能搜到；
  可以只搜当前列表，也可以搜全部，结果里标出命中的词。

## 0.1.9 — 2026-11-17

### English

- New app icon in a more saturated purple.
- Every item shows where it came from: the feed's icon, or the mailbox for mail. The reader's toolbar shows the
  source; click it to see everything from that feed or mailbox.
- Right-click menus on list rows and in the reader: read, star, archive, category, open or copy the link, and more.
- List style: text sizes, row spacing, padding, summary lines, and whether to show summaries, icons and images.
  "Group by date" splits the list into Today, Yesterday, This week and so on.
- Web page view (globe button or `W`): show an item's original web page in the reader, previewed as a desktop,
  iPad, iPhone or Android screen.
- Mark all as read asks before it runs.
- Adding a feed previews it as soon as you paste: icon, description, how often it posts and its latest items.
  YouTube channel addresses like `youtube.com/@name` can be subscribed to directly.
- The folder field lists recently used folders first, and a folder's menu has "Add a feed here".
- Outlook.com, Hotmail and Microsoft 365 sign in with your Microsoft account (OAuth) in the browser. This needs an
  application ID you register with Microsoft once; see the README.

### 中文

- 新的应用图标，紫色更饱和。
- 每一条都标出来源：文章是订阅源的图标，邮件是收件的邮箱。阅读区工具栏显示来源，点一下查看它的全部内容。
- 列表的行和阅读区都有右键菜单：已读、星标、归档、分类、打开或复制链接等。
- 列表样式：字号、行间距、内边距、摘要行数，以及是否显示摘要、图标和图片。「按日期分组」把列表分成
  今天、昨天、本周等几段。
- 原网页模式（地球按钮或 `W`）：在阅读区直接显示原网页，可以按桌面、iPad、iPhone、Android 的屏幕预览。
- 全部标为已读之前会先确认。
- 添加订阅时一粘贴就显示预览：图标、介绍、更新频率和最近的文章。可以直接订阅 `youtube.com/@名字`
  这样的 YouTube 频道地址。
- 选文件夹时最近用过的排在最前面，文件夹的菜单里有「在这里添加订阅」。
- Outlook.com、Hotmail 和 Microsoft 365 用微软账户在浏览器里登录（OAuth）。需要先在微软注册一个应用 ID，
  步骤见说明文档。
