# Changelog

What changed in each Postflow release, newest first. Every version has an English and a Chinese list; the update
window in the app shows the one for your language. Add new entries under **Unreleased**; `npm run release` turns
that heading into the version number and date.

## Unreleased

### English

- Signatures: each mailbox can have one (in its settings). It's added below new messages and above the quote in
  replies, and switches when you change the mailbox you send from.
- Move mail into a folder: "Move to folder" in the right-click menu, or drag it onto the folder in the sidebar.
- Formatted mail: bold, italic, underline, strikethrough, links (`⌘K`), lists and quotes. The "T" button switches
  between formatted and plain text. Formatted mail includes a plain-text copy for other mail apps.
- Older mail: at the end of a mailbox, folder, Sent, Drafts, Spam or Trash list, load the next 100 older messages
  from the server.

### 中文

- 签名：每个邮箱可以设一个（在邮箱设置里）。写新邮件时加在正文后面，回复时在引文上面；换发信的邮箱时签名跟着换。
- 把邮件移进文件夹：右键菜单里的「移到文件夹」，或者直接拖到侧栏的文件夹上。
- 带格式的邮件：粗体、斜体、下划线、删除线、链接（`⌘K`）、列表和引用。「T」按钮在带格式和纯文本之间切换。
  带格式的信会附一份纯文本，给只看纯文本的邮件程序。
- 更早的邮件：邮箱、文件夹、已发送、草稿、垃圾邮件和已删除的列表拉到底，可以从服务器再取 100 封更早的。

## 0.1.11 — 2026-11-17

### English

- Write, reply, reply all and forward mail (`C`, `⌘R`, `⇧⌘R`, `⇧⌘F`; `Ctrl` on Windows and Linux). Replies quote
  the original and stay in the same conversation, recipients are suggested from people who have written to you, and
  `⌘↩` sends. Mail goes straight from your device to your provider's outgoing server, which is set up for you; a copy
  is saved to Sent on the server. Plain text only for now.
- A mailbox's settings show its outgoing (SMTP) server, which can be set by hand.
- Outlook and Microsoft 365 now ask for permission to send mail when you sign in. Mailboxes signed in earlier keep
  receiving mail; sign in again from the mailbox's settings to send from them.
- Attachments: add files with the paperclip, by dropping them on the message or by pasting an image. Forwarding
  carries the original's attachments.
- Drafts save themselves to your mailbox's Drafts folder while you write, so other devices and webmail see them,
  and closing the window keeps the draft. A new Drafts view under your mailboxes opens them to keep writing;
  sending or deleting a draft removes it from the server.
- Sent: everything you've sent, from Postflow or elsewhere, in a view under your mailboxes. People you write to are
  suggested first as recipients.
- Your own folders: the arrow next to a mailbox lists its other folders, nested; open one to sync and read it.

### 中文

- 可以写信、回复、全部回复和转发了（`C`、`⌘R`、`⇧⌘R`、`⇧⌘F`；Windows 和 Linux 上是 `Ctrl`）。回复时引用原信，
  在别的邮件程序里也归在同一个对话里；收件人会从给你来过信的人里提示；`⌘↩` 发送。信从你的设备直接交给邮箱的
  发信服务器，服务器自动选好；发出的信在服务器的「已发送」里存一份。目前只能写纯文本。
- 邮箱设置里显示发信（SMTP）服务器，也可以自己填。
- Outlook 和 Microsoft 365 登录时会多申请一项发信权限。以前登录的邮箱照常收信；要发信时在邮箱设置里重新登录一次。
- 附件：点回形针、把文件拖进写信窗口或者粘贴图片都能添加。转发时会带上原信的附件。
- 写信时自动存草稿，存进邮箱的「草稿」文件夹，其他设备和网页邮箱里也看得到；关掉窗口草稿还在。
  邮箱列表下面新增「草稿」，打开就能接着写；发出去或者删掉时，服务器上的草稿也一起删掉。
- 「已发送」：在 Postflow 里或别处发出的信都在这里，在邮箱列表下面。填收件人时，你写过信的人排在前面。
- 自己建的文件夹：点邮箱右边的小箭头，按层级列出它的其他文件夹，打开一个就同步并显示里面的邮件。

## 0.1.10 — 2026-11-17

### English

- When a feed moves, change its address in the feed's Edit dialog. Postflow checks the new address first and keeps
  the same feed, with its read and starred states and earlier items. The warning sign next to a feed means its last
  refresh failed.
- The update window shows what's new right inside the app, and checking for updates in Settings opens it straight
  away.
- Full-text search: the magnifier in the list toolbar, `/` or `⌘F`. It searches titles, senders, authors and the full
  text of articles and mail, Chinese included, in the current list or everywhere, and highlights what it found.
- New mail notifications while Postflow is in the background, and the number of unread inbox messages on the app
  icon. Both can be turned off in Settings → Appearance.
- Unsubscribe from newsletters right from the message header; lists that support it are unsubscribed in one click.
- Feeds that move with a permanent redirect switch to their new address by themselves, and a broken feed's Edit
  dialog can look for the feed on its site again.

### 中文

- 订阅源换了地址时，可以在它的「编辑」里改订阅地址。Postflow 先检查新地址能用才换，还是同一个订阅源，
  已读、星标和以前的文章都保留。订阅源旁边的黄色三角表示上次刷新失败。
- 更新窗口里直接显示更新说明；在设置里点「检查更新」，发现新版本时直接打开这个窗口。
- 全文搜索：列表工具栏的放大镜、`/` 或 `⌘F`。搜标题、发件人、作者和文章、邮件的正文，中文按词也能搜到；
  可以只搜当前列表，也可以搜全部，结果里标出命中的词。
- 应用在后台时新邮件弹出系统通知，应用图标上显示收件箱的未读邮件数。两项都可以在设置 → 外观里关掉。
- 可以在邮件的信头上直接退订电子报；支持一键退订的点一下就好。
- 用永久跳转搬了家的订阅源会自动换成新地址；打不开的订阅源可以在「编辑」里到网站上重新找。

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
