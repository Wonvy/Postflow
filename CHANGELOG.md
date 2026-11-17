# Changelog

What changed in each Postflow release, newest first. Every version has an English and a Chinese list; the update
window in the app shows the one for your language. Add new entries under **Unreleased**; `npm run release` turns
that heading into the version number and date.

## Unreleased

### English
- Sent mail shows up in Sent a few seconds after sending, instead of waiting for the next time new mail arrives.
  Folders created or renamed on the server while Postflow is running (such as a Sent folder made when the first
  message is sent) are found without restarting. Sent has its own hint when it's empty.
- Plugins can add their own section above the body in the reader (api.ui.addReaderSection). New example plugin
  textrank-zh: keywords and a three-sentence summary for Chinese articles and mail, computed offline with
  TextRank (textrank4zh-ts, MIT); a reader button turns it on and off. The plugin guide now explains bundling
  libraries, which a plugin can't load from the web in the desktop app.
- Automatic AI classification is gone. With some models it could keep sending the same items again every minute when
  the answer couldn't be read, costing money for nothing. New items are filed by your rules (Settings → Rules);
  "Classify now" still asks the AI once when you click it, and logs the shape of an answer it can't read.
- Mail: when Postflow can't find a mailbox's Sent, Drafts, Spam or Trash folder, verbose logs list the folders the
  server offers, to help with providers that name them differently.
- Mail stays connected: Postflow checks in with the server every 2 minutes instead of waiting 5 in silence, so
  servers and network gateways that close quiet connections (Sohu after a minute, 163 and 126, QQ and Gmail behind
  some networks) no longer drop it. If a server still hangs up, Postflow learns how long it allows and checks in
  sooner. These expected reconnects are no longer logged as warnings or shown as mailbox errors.
- The sidebar's tabs layout (All / Mail / Feeds tabs, its toggle button, the setting, and keys 1–3) is gone; the
  sidebar always shows every section. To see only mail or only feeds, use the filter at the top of the list.
- Maximize the reader: the button in the reader toolbar (or `F`) hides the sidebar and list so an article, a video
  page in web-page view or a long message fills the window. Full screen (button or `Shift+F`) does that and makes
  the whole window full screen, without the window buttons on Windows. `Esc` steps back out. Making the window full
  screen the system's way (like macOS's green button) leaves the layout as it is.

### 中文
- 发出的邮件几秒钟后就会出现在「已发送」里，不用再等下一次收到新邮件。Postflow 运行期间服务器上新建或改名的文件夹
  （比如第一次发信时才建出来的「已发送」）不用重启也能找到。「已发送」为空时有了自己的提示。
- 插件可以在阅读区正文上方加自己的一块内容（api.ui.addReaderSection）。新增示例插件 textrank-zh：用 TextRank 离线
  为中文文章和邮件提取关键词和三句摘要（textrank4zh-ts，MIT），阅读区有按钮可以开关。插件开发文档补充了如何把第三方库打包进插件
  （桌面应用里插件不能从网上加载库）。
- 去掉了 AI 自动分类：有些模型的回答读不出来时，它会每分钟把同一批内容再发一次，白白花钱。新内容改由你的规则分类
  （设置 → 自动规则）；「立即分类」仍然可以点一下让 AI 分一次，读不出回答时日志里会记下回答的样子。
- 邮件：找不到邮箱的「已发送」「草稿」「垃圾邮件」「已删除」文件夹时，详细日志里会列出服务器提供的文件夹，方便排查叫法不同的邮箱。
- 邮件连接更稳：Postflow 每 2 分钟跟服务器说一句话，不再安静等 5 分钟，所以会清理安静连接的服务器和网关
  （搜狐一分钟左右，163、126，以及部分网络下的 QQ、Gmail）不会再把连接断掉。服务器还是会断的话，Postflow 会记住它能等多久，
  以后提前说话。这类正常的重连不再记成警告，也不会让邮箱显示出错。
- 去掉了侧栏的「选项卡」模式（全部 / 邮件 / 订阅选项卡、切换按钮、设置项和 1–3 快捷键），侧栏总是显示所有分组。
  只想看邮件或只看订阅，用列表顶部的筛选。
- 阅读区可以最大化：阅读区工具栏上的按钮（或按 `F`）把侧栏和列表藏起来，文章、原网页里的视频、长邮件占满整个窗口。
  全屏（按钮或 `Shift+F`）在此基础上让整个窗口全屏，Windows 上也不显示窗口按钮。按 `Esc` 逐级退出。
  用系统的方式全屏（比如 macOS 的绿色按钮）时布局不变。

## 0.1.14 — 2026-11-17

### English

- Toolbars you can arrange: right-click the list or reader toolbar and choose "Customize toolbar…" to reorder
  buttons and put each one in the top toolbar, in the new status bar, or hide it. Keyboard shortcuts keep working.
- Status bars under the list (number of items, unread, selected, refreshing) and the reader (date, word count,
  reading time and how far you've read). Display switches like list style, folding and translation now sit there
  by default, so the top toolbars are shorter.
- Paged list: under List style → Browse by → Pages, each turn of the mouse wheel moves one page, and every page
  starts on a whole row. The status bar shows the page number with previous and next buttons.
- The list can hide source names as well as source icons, and the List style and Fold menus open when you hover.
- Rules (Settings → Rules): when new mail or articles arrive, file them in a category, mark them read, star or
  archive them, based on the title, sender, recipients, body or source (contains, starts with, regular expression
  and more). A ready-made rule handles verification-code mail. "Try it" shows what a rule would match, and
  rules can be applied to items you already have. Rules sync to your other devices.
- Plugins (Settings → Plugins): put a plugin folder in the plugins folder and turn it on. Plugins can add toolbar
  buttons (which you can place like any other button), right-click menu items and styles, react to new mail, and
  read and change items. The developer guide is docs/plugins.md, and examples/plugins/copy-code is a complete
  example. Plugins only load after you turn them on, on each device.
- Two more example plugins: important-senders (stars new mail from senders you mark as important) and
  daily-digest (copies today's unread items, or the open one, as Markdown). The plugin developer guide is now
  also available in Chinese (docs/plugins.zh-CN.md), and Settings → Plugins opens the one for your language.
- A new look for the Windows installer: brand-purple welcome and finish pages, a header with the Postflow logo,
  its own setup icon, and Chinese as well as English (picked from your Windows language).
- Windows: updating works when Postflow is installed in a folder that needs administrator rights (such as
  D:\Program Files). Postflow notices, downloads and checks the update, and asks for permission through the
  Windows prompt instead of failing.
- RSSHub in Add feed: when an RSSHub instance refuses (the public rsshub.app turns feed readers away with
  HTTP 403), the message now says so and what to change, instead of "The server returned an error". YouTube
  routes offer to follow the channel directly, without RSSHub. Route and setting descriptions, which RSSHub mostly
  writes in English, are translated into your language with the offline translation model (downloaded on request).
- RSSHub settings in Settings → Feeds (and in Add feed): instance address, access key (ACCESS_KEY, added to
  feed addresses as ?key=), and a Test connection button that says whether the instance works, has a bot check
  like rsshub.app, needs a key, or isn't RSSHub at all. Addresses without http:// use http for local and LAN
  instances. New guide for running your own RSSHub on a computer, NAS or cloud server: docs/rsshub.md.
- Add feed shows popular sites with official feeds as icons: YouTube, GitHub (releases, commits, tags or a user),
  Reddit, Bluesky, Mastodon, Medium, Substack, Stack Overflow, Hacker News, V2EX and arXiv. Click one, type a name
  (or paste the page address), and the preview appears; no RSSHub needed.

### 中文

- 工具栏可以自己排：在列表或阅读区的工具栏上点右键，选「自定义工具栏…」，可以调整按钮顺序，每个按钮放在顶部工具栏、
  新的状态栏，或者隐藏。快捷键不受影响。
- 列表和阅读区底部有了状态栏：列表显示条数、未读、已选和刷新状态；阅读区显示时间、字数、预计阅读时间和读到了哪里。
  列表样式、折叠、翻译这些显示开关默认挪到了状态栏，顶部工具栏清爽了。
- 分页视图：「列表样式 → 浏览方式 → 分页」，滚轮滚一下翻一页，每页都从一整行开始；状态栏显示第几页和上一页、下一页按钮。
- 列表除了来源图标，也可以隐藏来源名称；「列表样式」和「折叠」菜单鼠标移上去就会打开。
- 自动规则（设置 → 自动规则）：新邮件和新文章进来时，按标题、发件人、收件人、正文或来源（包含、开头是、正则表达式等）
  自动分类、标为已读、加星标或归档。自带一条「验证码邮件」规则；「试一下」能看到会命中哪些；也可以应用到已有的条目。规则会同步到其他设备。
- 插件（设置 → 插件）：把插件文件夹放进插件目录并启用。插件可以加工具栏按钮（和其他按钮一样可以排）、右键菜单项和样式，
  响应新邮件，读取和修改条目。开发文档见 docs/plugins.md，完整的例子在 examples/plugins/copy-code。插件要在每台设备上手动启用才会加载。
- 新增两个插件例子：important-senders（给标记为重要的发件人的新邮件自动加星标）和 daily-digest（把今天的未读条目或打开的条目复制成 Markdown）。
  插件开发文档有了完整的中文版（docs/plugins.zh-CN.md），「设置 → 插件」里的开发文档按钮会按界面语言打开对应的版本。
- Windows 安装程序换了新样子：品牌紫色的欢迎页和完成页、带 Postflow 图标的页眉、单独的安装程序图标，
  并且有了中文界面（按 Windows 的语言自动选择）。
- Windows：Postflow 装在需要管理员权限的文件夹（比如 D:\Program Files）时也能在线更新了。Postflow 会发现这种情况，
  下载并校验更新后，通过系统的授权窗口请求权限，而不是直接安装失败。
- 添加订阅里的 RSSHub：RSSHub 实例拒绝访问时（公共实例 rsshub.app 会对阅读器返回 HTTP 403），提示会说明原因和该怎么改，
  不再只是「对方服务器返回了错误」。YouTube 的路由会提供「直接订阅」，不经过 RSSHub。RSSHub 的路由和参数说明大多是英文，
  现在会用离线翻译模型翻成你的语言（第一次需要点一下下载模型）。
- 「设置 → 订阅」（以及添加订阅里）有了 RSSHub 设置：实例地址、访问密钥（ACCESS_KEY，会以 ?key= 加在订阅地址后面），
  以及「测试连接」，能告诉你实例能不能用、是不是像 rsshub.app 那样开了防机器人验证、是否需要密钥、是不是根本不是 RSSHub。
  没写 http:// 的本机和局域网地址会用 http。新增部署指南，介绍在电脑、NAS、云服务器上部署自己的 RSSHub：docs/rsshub.zh-CN.md。
- 添加订阅里多了「常用网站」图标：YouTube、GitHub（发布版本、提交记录、标签或用户动态）、Reddit、Bluesky、Mastodon、
  Medium、Substack、Stack Overflow、Hacker News、V2EX、arXiv。点一个，填个名字（或粘贴网页地址）就能预览订阅，不需要 RSSHub。

## 0.1.13 — 2026-11-17

### English

- Reading settings use sliders: text size, line spacing, letter spacing (new), paragraph spacing (new) and width.
  Double-click a slider to reset it.
- Sender avatars show the logo of the sender's website. Personal addresses (Gmail, QQ Mail and the like) keep the
  initial. Turn it off under Settings → Appearance → Sender logos.
- When a new version is out, a notice appears once at the bottom right, and an update icon stays at the top of the
  sidebar until you update.
- Right-click menus everywhere in the sidebar: "Mark all as read" on every list, folder and category, and menus on
  the Mail, Categories and Feeds headings. The system's own right-click menu (Reload, Inspect) is gone; text fields
  and selected text keep theirs.
- RSSHub in Add feed: search thousands of sites that have no feed of their own, pick one, and fill in its
  settings as fields, with menus where RSSHub lists the choices. Pasting a page like a GitHub repo or a bilibili
  profile also suggests what RSSHub can follow there. The RSSHub address is set right in the dialog and syncs to
  your other devices; the public rsshub.app limits feed readers, so your own instance is recommended.
- Windows: the system title bar is gone; minimize, maximize and close are drawn by the app at the top right.
- Settings → Storage shows how much space the data folder and this device use, which feeds and mailboxes take the
  most, and clears local caches: mail older than 30 days (it downloads again when opened), fetched full text and
  sender logos. It can also compact the local index.
- Language files: export the interface text, translate it, and import it to use Postflow in your own language
  (Settings → Appearance). The format is in docs/language-files.md.

### 中文

- 阅读设置改用滑块：字号、行距、字间距（新增）、段间距（新增）和宽度。双击滑块回到默认。
- 发件人头像显示发件人网站的图标；个人邮箱（Gmail、QQ 邮箱等）仍显示首字母。可以在「设置 → 外观 → 发件人显示网站图标」关掉。
- 有新版本时右下角提示一次，侧栏顶上留一个更新图标，直到更新为止。
- 侧栏处处有右键菜单：每个列表、文件夹和分类都能「全部标为已读」，「邮箱」「分类」「订阅」标题上也有菜单。
  系统自带的右键菜单（重新载入、检查元素）不再出现；输入框和选中的文字仍保留系统菜单。
- 添加订阅里可以用 RSSHub：搜索几千个没有 RSS 的网站，选一项后按表单填参数，RSSHub 给了可选值的显示成下拉框。
  粘贴 GitHub 仓库、bilibili 空间这类网页时，也会提示 RSSHub 能订阅的内容。RSSHub 实例地址直接在对话框里改，会同步到其他设备；
  公共实例 rsshub.app 对阅读器有限制，建议用自建实例。
- Windows：去掉系统标题栏，右上角的最小化、最大化和关闭由应用自己画。
- 「设置 → 存储空间」：数据目录和本机各占多少、哪些订阅和邮箱占得最多；可以清理本机缓存（30 天前的邮件原文，打开时重新下载；
  抓取的全文；发件人图标），以及压缩本机索引。
- 语言文件：导出界面文字，翻译后导入，就能用自己的语言（设置 → 外观）。格式见 docs/language-files.md。

## 0.1.12 — 2026-11-17

### English

- Signatures: each mailbox can have one (in its settings). It's added below new messages and above the quote in
  replies, and switches when you change the mailbox you send from.
- Move mail into a folder: "Move to folder" in the right-click menu, or drag it onto the folder in the sidebar.
- Formatted mail: bold, italic, underline, strikethrough, links (`⌘K`), lists and quotes. The "T" button switches
  between formatted and plain text. Formatted mail includes a plain-text copy for other mail apps.
- Gmail and Google Workspace can sign in with your Google account instead of an app password. Like Microsoft, this
  needs an OAuth client you register with Google once; see the README.
- Older mail: at the end of a mailbox, folder, Sent, Drafts, Spam or Trash list, load the next 100 older messages
  from the server.

### 中文

- 签名：每个邮箱可以设一个（在邮箱设置里）。写新邮件时加在正文后面，回复时在引文上面；换发信的邮箱时签名跟着换。
- 把邮件移进文件夹：右键菜单里的「移到文件夹」，或者直接拖到侧栏的文件夹上。
- 带格式的邮件：粗体、斜体、下划线、删除线、链接（`⌘K`）、列表和引用。「T」按钮在带格式和纯文本之间切换。
  带格式的信会附一份纯文本，给只看纯文本的邮件程序。
- Gmail 和 Google Workspace 可以用 Google 账户登录，不用应用专用密码。和微软一样，需要先在 Google 注册一个
  OAuth 客户端，步骤见说明文档。
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
