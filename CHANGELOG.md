# Changelog

What changed in each Postflow release, newest first. Every version has an English and a Chinese list; the update
window in the app shows the one for your language. Add new entries under **Unreleased**; `npm run release` turns
that heading into the version number and date.

## Unreleased

### English
- Discover → Podcasts gains 56 popular Chinese shows, picked from Apple Podcasts' China charts with 声链 (shenglianfm.com) as a guide, and podcasts are now sorted into news, tech, business, education, health, stories, arts, sports, entertainment and society; pick one at the top of the list. Every feed was checked to work in Postflow.

### 中文
- 「发现 → 播客」新增 56 个热门中文播客，参考「声链」（shenglianfm.com）的分类，按 Apple 播客中国区各分类榜单挑选；播客按新闻、科技、商业、教育、健康、故事、艺术、体育、娱乐、社会文化分类，在列表顶上选。每个源都确认过能在 Postflow 里正常订阅。

## 0.1.29 — 2026-11-18

### English
- Podcasts: search for a show by name under Add feed → Podcasts, or pick one from the new Podcasts category in Discover. Episodes play in the reader and keep playing in a small player under the list: back 15 s, forward 30 s, 0.8×–3× speed, and each episode remembers where you stopped. Finished episodes are marked read, and "Play the next episode when done" keeps going. Video attachments play in the reader.
- Feeds are now sorted into articles, videos and podcasts. Once you have more than one type, a row of icons at the top of Feeds shows one type at a time, with a dot when a type has unread items. Change a feed's type in its Edit dialog if the automatic choice is wrong.
- A tidier Feeds sidebar: the views sit under a Views header that you can collapse, and right-clicking it chooses which views to show. Feeds shows only Today and Starred by default; with nothing selected you see all articles, and clicking the selected feed, folder or view again (or the Feeds tab) goes back there. Mail keeps all its views.
- Articles no longer offer Archive or Snooze while those views are hidden in Feeds: the buttons and menu items are gone, E and swiping left mark an article read instead, and H does nothing on articles. Show Archived or Snoozed again (right-click Views) and they come back. Mail is unchanged.
- Podcast episodes in the list show their length (or how much is left) with a play button; click it to play without opening the episode.
- Video feeds open as cards and podcasts as a list by default; each keeps its own view, separate from articles.
- Player shortcuts: P plays or pauses (the open episode if nothing is playing), Shift ← / Shift → go back 15 s or forward 30 s. A sleep timer in the speed menu pauses after 15, 30 or 60 minutes, or at the end of the episode.
- Number keys switch the type in Feeds, in the order of the icon row (1 is everything).
- Settings → General can choose which views Mail and Feeds show, the same as right-clicking the sidebar's Views header.
- Releases: a version that finishes building late no longer becomes the "latest" release over a newer one, so the in-app update always offers the newest version.
- Feeds shows its views (Today and Starred by default) side by side in one row, without a Views header.
- Optional: mark feed articles read as they scroll off the top of the list (Settings → General). Not in the Unread view, and mail is never affected.
- Per feed "Always fetch the full text" in Edit feed, for sites whose feed has only a summary.
- The same article from several feeds (same link, or the same long title) shows once; expand it to see each feed's copy, and reading it marks every copy read.
- Vimeo links get a player in the reader, like YouTube and Bilibili.
- Newsletters: right-click a mail and choose "Read with feeds" to read that sender's mail in Feeds instead of Mail. Each sender gets a row under Newsletters (and a Newsletters icon in the type row); Mail's views and counts no longer include them. Right-click the sender to move it back.
- Podcast chapters and transcripts (Podcasting 2.0): the play bar lists the chapters and shows the transcript; click a chapter or a line to play from there, and the current one is highlighted.
- Download podcast episodes for offline listening from the play bar. Downloads stay on this device and play without the network; Settings → General sets how many to keep (the oldest go first).

### 中文
- 播客：在「添加订阅 → 播客」里按节目名搜索，或者在「发现」新增的「播客」分类里挑。单集在阅读区里播放，换条目时列表底下的迷你播放器接着放：后退 15 秒、前进 30 秒、0.8×–3× 倍速，每一期记住听到哪里。听完自动标为已读，打开「播完接着放下一期」可以连着听。带视频附件的条目在阅读区里直接播放。
- 订阅源分成文章、视频、播客三类。有两类以上时，订阅顶上一排图标，一次只看一类，有未读的那一类显示一个小点。自动判断不对时，可以在「编辑订阅」里改。
- 订阅的侧栏更简洁：视图放在可以收起的「视图」标题下，右键它选显示哪几个。订阅默认只显示「今天」和「星标」；什么都没选就是全部文章，再点一下选中的订阅源、文件夹或视图（或者「订阅」切换）就回到全部文章。邮件那边的视图不变。
- 订阅那边藏起了「已归档」「稍后提醒」视图时，文章不再提供归档和稍后提醒：按钮和菜单项都去掉了，E 和往左滑改成标为已读，H 对文章不起作用。右键「视图」重新打开这两个视图，它们就回来了。邮件不变。
- 列表里的播客单集显示时长（听过的显示还剩多久）和播放按钮，点一下就放，不用打开。
- 视频类默认用卡片视图，播客类默认用列表，和文章各记各的。
- 播放器快捷键：P 播放 / 暂停（没有在放时放打开的这一期），Shift ← / Shift → 后退 15 秒、前进 30 秒。倍速菜单里有睡眠定时：15、30、60 分钟后暂停，或者这一期放完就停。
- 数字键按订阅顶上那排图标的顺序切换类型（1 是全部）。
- 设置 → 通用 里可以选邮件和订阅各显示哪几个视图，和右键侧栏「视图」标题是同一个设置。
- 发版：后打完的旧版本不会再抢走「最新版」，应用里的在线更新总是给最新的版本。
- 订阅里的视图（默认「今天」「星标」）并排放在一行，不再单独成组。
- 可选：订阅文章在列表里滚出顶部就标为已读（设置 → 通用）。「未读」视图里不会，邮件不受影响。
- 「编辑订阅」里可以设「打开时自动取全文」，给源里只有摘要的网站用。
- 同一篇文章出现在好几个订阅源里（链接相同，或者标题一样且够长）只显示一次；展开能看到每个源的那一条，读了就全部算已读。
- Vimeo 的视频和 YouTube、B 站一样，在阅读区里直接播放。
- Newsletter：右键一封邮件选「放到订阅里看」，这个发件人的信就在订阅里看，不在邮件里了。每个发件人在「Newsletter」下有一行（类型那排图标里也多一个 Newsletter），邮件那边的视图和计数不再算它们。右键发件人可以放回邮件。
- 播客章节和文字稿（Podcasting 2.0）：播放条下面列出章节、显示文字稿，点一个章节或一句话就从那里放，正在放的那一项高亮。
- 播放条上可以下载单集离线听：文件只在这台设备上，放的时候不用联网；在「设置 → 通用」里设最多留几期（早下的先删）。

## 0.1.28 — 2026-11-18

### English
- Feeds without an icon now get their website's real icon: after each refresh, Postflow looks up the icon the site declares (a large touch icon when there is one, otherwise a favicon it has checked exists) and uses it from then on. Icons that feeds bring themselves are left alone.
- New option in Settings → Appearance → Reader: Pop-up. The window then has two columns, the sidebar and a full-width list; opening an item shows the reader in a large panel on top. J/K move between items, F fills the window, Esc or a click outside closes it. Beside the list stays the default; phones are unchanged.

### 中文
- 没有图标的订阅源会自动用上网站真正的收藏图标：每次刷新后找网站声明的图标（有大图标先用大图标，没有就用确认存在的 favicon），找到后就一直用它。订阅源自己带的图标不动。
- 设置 → 外观 → 阅读区 新增「弹出」：窗口里只有侧栏和占满的列表两栏，打开一条时阅读区以大面板盖在上面。J/K 换条目，F 铺满窗口，按 Esc 或点旁边关掉。默认仍是「列表右边」，手机上不变。

## 0.1.27 — 2026-11-18

### English
- No more white flash when moving between items in the web page view: the page stays hidden until it has loaded (at most 2.5 seconds), with a spinner on the theme background meanwhile.
- Focus mode for the three columns: when the pointer is over the sidebar, the list or the reader, the other columns fade. On by default; turn it off in Settings → Appearance. Only with a mouse or trackpad.
- The command palette button moved to the right of the sidebar footer, next to the phone icon; both are lighter than Settings, and the phone icon shows a small dot when the phone link is ready.

### 中文
- 原网页模式下切换条目不再白屏闪一下：新网页加载出来（最多等 2.5 秒）才显示，之前是跟着主题的底色和转圈。
- 三栏聚焦：鼠标停在侧栏、列表或阅读区上时，另外两栏变淡。默认开启，可以在设置 → 外观里关掉；只在用鼠标或触控板时起作用。
- 命令面板按钮移到侧栏底部右边、手机图标左边，两个图标都比设置淡一些；手机链接准备好时手机图标右上角显示一个小点。

## 0.1.26 — 2026-11-18

### English
- Browser access picks the right network: on computers with a VPN, virtual machines or WSL, the link and QR code now use the real Wi-Fi or Ethernet address, and you can choose another network in the settings if needed.
- A phone icon at the bottom right of the sidebar shows the QR code on hover; click it for the browser access settings (desktop app).
- Press H to snooze: pick a preset with 1–5 or choose a date and time.
- The More menu in the reader no longer repeats buttons you've put back on the toolbar.
- The welcome guide mentions the Mail/Feeds switch, ⌘K and ?.
- Easier to use with your fingers on phones: the page is a little larger, and buttons, menus, sidebar rows, list rows and input fields are bigger on touch screens. Tapping a text box on an iPhone no longer zooms the page, and the reader's top bar fits on a phone screen.
- iPad and other tablets: held upright, the list and reader sit side by side and the sidebar slides out from the menu button; turned sideways on 11-inch and larger screens, all three columns show. Articles default to 17px text and list rows are larger. Swipe a row to archive or mark it read, long-press for its menu, and drag the wider column divider with a finger.

### 中文
- 网页访问选对网络了：电脑上有 VPN、虚拟机或 WSL 时，链接和二维码用的是真正的 Wi-Fi 或有线网卡的地址；需要时也可以在设置里换一个网络。
- 侧栏右下角加了手机图标：鼠标移上去显示二维码，点一下打开网页访问设置（桌面应用）。
- 按 H 稍后提醒：按 1 到 5 选预设时间，或者自己挑日期和时间。
- 阅读区的「更多」不再重复你放回工具栏的按钮。
- 新手引导介绍了邮件和订阅的切换、⌘K 和 ?。
- 手机上更好用手指点了：页面整体放大一点，触屏上的按钮、菜单、侧栏、列表行和输入框都变大；iPhone 上点输入框不会再把页面放大，阅读区顶栏的按钮在手机屏幕上也放得下了。
- iPad 等平板：竖着拿时列表和阅读区并排，侧栏从菜单按钮滑出来；11 寸及以上横着拿时三栏都显示。文章默认 17 像素字号，列表行更大；左右滑动一行可以归档或标为已读，长按弹出菜单，栏间的分隔线手指也好拖。

## 0.1.25 — 2026-11-18

### English
- Fewer buttons: the reader's top bar keeps Reply (with a small menu for Reply all and Forward), Category, AI, Star, Archive, Snooze, Maximize (Shift-click for full screen) and a new More menu with Mark unread, Open in browser, Full text, View source and Full screen. The list toolbar no longer repeats Refresh, and the Feeds sidebar no longer repeats Stats. Toolbars you customised keep your layout.
- List rows show a … button on hover with the same menu as right-click.
- Settings has a new General page for the sidebar, mail and app options; Appearance is just theme, density, sender logos and language.
- Press ? for all keyboard shortcuts.
- The command palette finds Chinese names by their pinyin initials, like "ssp" for 少数派.

### 中文
- 按钮更少了：阅读区顶部保留回复（旁边的小菜单里有全部回复、转发）、分类、AI、星标、归档、稍后提醒、最大化（按住 Shift 点是全屏），新增的「更多」里有标为未读、在浏览器中打开、全文、查看源代码、全屏。列表顶部不再重复刷新按钮，订阅侧栏顶部不再重复统计按钮。自定义过工具栏的保持原样。
- 列表行鼠标移上去会出现「…」按钮，菜单和右键的一样。
- 设置新增「通用」页，放侧栏、邮件和应用相关的开关；「外观」只留主题、密度、发件人图标和语言。
- 按 ? 查看全部快捷键。
- 命令面板可以用拼音首字母搜中文名字，比如「ssp」找到少数派。

## 0.1.24 — 2026-11-18

### English
- Unread counts next to feeds, mailboxes and categories no longer include snoozed items.
- Snoozed items that come back are pinned to the top of the mailbox, feed, folder or category they belong to, not only the inbox.
- Phones: add Postflow to the home screen to open it full screen like an app. If a phone opens it without the access key, Postflow asks you to paste the link from the desktop app instead of showing an error. On phones, swipe a row left to archive or right to mark it read or unread.
- Browser access settings explain what plain HTTP limits on phones (notifications, some copying).
- The recommended-feeds list now records the exact time it changed, so a second change on the same day still reaches everyone.

### 中文
- 侧栏里订阅源、邮箱和分类的未读数不再算上稍后提醒的条目。
- 到时间回来的稍后提醒，在它所在的邮箱、订阅源、文件夹或分类里也排在最上面，不只是收件箱。
- 手机：可以「添加到主屏幕」，像应用一样全屏打开。手机上没带访问密钥打开时，Postflow 会请你贴上桌面应用里的链接，而不是报错。手机上把一行往左滑归档，往右滑标为已读或未读。
- 网页访问的设置里说明了普通 HTTP 在手机上的限制（通知、部分复制功能）。
- 推荐订阅源列表记下的是精确的修改时间，同一天改第二次也能推送给所有人。

## 0.1.23 — 2026-11-18

### English
- List views: by default every list shares one display mode and grouping. Lock a list (the lock at the bottom of the list) to give it its own, or turn on Settings → Appearance → Remember each list's view to make every list keep its own, like folders in a file manager. Lists you had customised are locked for you.
- Card view: rows without pictures are no longer as tall as rows with them, and pictures in cards and summaries are shown whole instead of cropped.
- Web page view: drag the edges of the iPad, iPhone or Android frame to any size; each device remembers its size, with a button to reset it.
- Sidebar: the Mail and Feeds lists no longer repeat their name as a heading. The Feeds + button adds a feed directly; right-click empty space in the sidebar to add a feed, make a folder or refresh.
- Drop-down menus no longer show the old gradient look in the macOS app.

### 中文
- 列表视图：默认所有列表共用一种显示方式和折叠方式。点列表底部的锁可以让这个列表单独记住自己的；或者在「设置 → 外观」打开「每个列表记住自己的视图」，像文件夹一样每个列表各记各的。以前单独设过的列表会自动锁上。
- 卡片视图：没有图的一行不再和有图的一样高；卡片和摘要视图里的图片完整显示，不再裁剪。
- 原网页视图：iPad、iPhone、Android 的设备框可以拖动边缘随意改大小，每种设备记住自己的大小，也可以一键恢复。
- 侧栏：邮件和订阅里不再重复显示同名的标题。订阅那边的「+」直接添加订阅；在侧栏空白处右键可以添加订阅、新建文件夹、刷新。
- 下拉框在 macOS 应用里不再是带渐变的老式样子。

## 0.1.22 — 2026-11-18

### English
- Snooze: send mail or articles away until later today, this evening, tomorrow, the weekend, next week or a time you pick. They wait under Snoozed and come back unread at the top of the inbox with a notification. Snoozes sync between devices.
- Undo send (5 seconds by default, Settings → Appearance) and schedule send (the clock next to Send). Scheduled mail is listed under Scheduled in the Mail sidebar; Postflow sends it when it's running at that time, or as soon as it opens.
- Command palette: ⌘K / Ctrl+K (or the button at the bottom of the sidebar) to jump anywhere or run a command.
- Tray: closing the window keeps Postflow running in the tray (menu bar) with the unread count; quit from the tray menu. Can be turned off.
- Phones: on narrow screens the layout shows one pane at a time, with a sliding sidebar and a back button, so browser access works well on a phone.
- More Chinese feeds in the catalog for design and life. Postflow is now licensed under GPL-3.0-or-later.
- Browser access: turn on Settings → Browser access to use Postflow in a browser with the same data. Open it on this computer with one click, or allow access from your network and scan the QR code on your phone. Links carry an access key that the browser then remembers; changing the key signs out every browser. It runs inside the desktop app and stops when you quit.

### 中文
- 稍后提醒：邮件或文章可以先拿走，到今晚、明早、周末、下周或者自己挑的时间再回来。等待时在「稍后提醒」里，到时间回到收件箱顶部、标为未读并弹提醒。提醒时间会在设备间同步。
- 撤销发送（默认 5 秒，设置 → 外观）和定时发送（「发送」旁边的时钟）。等着发的在邮件侧栏的「定时发送」里；到时间时 Postflow 开着就发，关着的话下次打开时补发。
- 命令面板：⌘K / Ctrl+K（或侧栏底部的按钮），前往任意位置或执行命令。
- 托盘常驻：关闭窗口后 Postflow 留在托盘（菜单栏）里继续收信，显示未读数；从托盘菜单退出。可以关掉。
- 手机：屏幕窄时一次只显示一栏，侧栏滑出、阅读区有返回按钮，网页访问在手机上也好用。
- 推荐订阅源补了中文的设计和生活类。Postflow 采用 GPL-3.0-or-later 许可证。
- 网页访问：在「设置 → 网页访问」里打开后，可以在浏览器里用 Postflow，数据和应用里的一样。这台电脑上一点就能打开；允许局域网访问后，手机扫二维码也能用。链接里带访问密钥，浏览器之后会记住；更换密钥后所有浏览器都要重新打开。它由桌面应用提供，关掉应用就停了。

## 0.1.21 — 2026-11-18

### English
- Discover in Add feed shows feeds as a grid of icons by default; switch to the list with the buttons next to the language menu. The choice is kept on this device.
- The recommended feeds update online: the list and its icons are one file, and the app checks GitHub (through jsDelivr) for a newer version at most once a day, falling back to the list it came with. Turn it off or check now in Settings → Feeds.
- Offline translation recognises plain French, German and similar sentences again; since 0.1.18 they were sometimes left untranslated.

### 中文
- 「添加订阅 → 发现」默认用图标网格显示，可以用语言菜单旁的按钮切换成列表，这台设备会记住。
- 推荐订阅源可以在线更新：列表和图标都在一个文件里，应用每天最多一次从 GitHub（经 jsDelivr）检查新版本，检查不到就用自带的。可以在「设置 → 订阅」里关掉或立即检查。
- 离线翻译又能认出普通的法语、德语等句子了；0.1.18 以来它们有时不会被翻译。

## 0.1.20 — 2026-11-18

### English
- Add feed opens on a new Discover page: about 150 official feeds from well-known sites, with icons, in ten categories (News, Tech, Programming, Design, Science, Business, Gaming, Culture, Life, Blogs). Feeds in your interface language come first, and you can show only Chinese or only English ones. Search by name or topic, subscribe with one click, or subscribe to a whole category; each category goes into its own folder unless you turn that off. Pasting an address still works from the same search box.
- The other two pages are Address (paste a site or feed address, or pick YouTube, GitHub and other popular sites) and Advanced (RSSHub, and a link to OPML import).
- The welcome guide's feed step lets you pick topics and subscribe to a few recommended feeds for each in one go.
- The catalog only lists feeds the sites publish themselves. Icons ship with the app, so browsing it doesn't contact any site.

### 中文
- 「添加订阅」打开时先显示「发现」页：内置约 150 个知名网站的官方订阅源，带图标，分十类（新闻、科技、编程开发、设计、科学、商业财经、游戏、文化、生活、博客）。界面语言的源排在前面，也可以只看中文或只看英文。可以按名字或主题搜索，一键订阅，或者整个分类一起订阅；默认每个分类放进同名文件夹，可以关掉。在同一个搜索框里粘贴网址照样能订阅。
- 另外两页是「网址」（粘贴网站或订阅地址，或者从 YouTube、GitHub 等常用网站里选）和「高级」（RSSHub，以及导入 OPML 的入口）。
- 新手引导的订阅那一步可以选几个感兴趣的分类，一次订阅每类几个推荐源。
- 目录只收网站自己提供的订阅源。图标随应用一起发布，浏览目录时不会访问任何网站。

## 0.1.19 — 2026-11-18

### English
- Mail and Feeds are now two separate spaces. Switch between them at the top of the sidebar; each has its own Inbox / All articles, Today, Unread, Starred and Archive, its own buttons (compose, sync mail and add account on the Mail side; stats, refresh feeds and add feed or folder on the Feeds side) and remembers the list you were on. The mail / articles filter and the stacked sidebar layout are gone.
- Categories show on both sides, counting only that side's items; a category with nothing on one side is hidden there.
- Feeds are optional: turn off Settings → Appearance → Show Feeds, or right-click the switch, to see only mail.
- The Inbox unread count now counts mail only, matching what the Inbox shows.
- A side with nothing added yet offers to add a mail account or a feed, and each side's refresh button spins only while that side is syncing.

### 中文
- 邮件和订阅分成两个独立的空间，在侧栏顶部切换。两边各有自己的收件箱（订阅这边叫「全部文章」）、今天、未读、星标、已归档和按钮：邮件这边是写信、同步邮件、添加邮箱，订阅这边是统计、刷新订阅、添加订阅或文件夹；切回来时停在上次看的列表。去掉了「只看邮件 / 只看订阅」筛选和侧栏的上下排列模式。
- 分类在两边都显示，数字只算这一边的条目；某一边没有内容的分类在那边不显示。
- 订阅可以不看：在「设置 → 外观」关掉「显示订阅」，或者右键切换条，侧栏就只有邮件。
- 收件箱旁的未读数只算邮件，和收件箱里显示的一致。
- 某一边还没有来源时，列表里直接提示添加邮箱或订阅；两边的刷新按钮只在自己那一边同步时转圈。

## 0.1.18 — 2026-11-18

### English
- Mail and feeds: the inbox stays on mail in the Mail tab, while the Feeds tab shows subscriptions. Mail can be grouped by mailbox, and hovering an account shows its full address.
- Lists: switch between list, summary and cards directly from the bottom bar. Grouping moved into List style, which now has area-padding and spacing sliders, wheel-adjustable values, and simpler checkmark options.
- Translation: language detection is less likely to request unrelated offline models for short or mixed-language items; missing-model prompts refresh when the list or translation settings change.
- Layout: keep the main navigation and categories fixed while the sidebar list scrolls, avoid clipping cards in paged lists, and optionally show summaries when hovering over single-line rows.

### 中文
- 邮件和订阅：「邮箱」选项卡的收件箱只显示邮件，「订阅」选项卡显示订阅内容；邮件可按邮箱分组，悬停邮箱时可查看完整地址。
- 列表：底栏可直接切换列表、摘要和卡片视图。分组设置并入「列表样式」，新增列表区域边距和间距滑块，数值可悬停滚轮调整，开关改为简洁的勾选样式。
- 翻译：改进短文本和中英混合内容的语言判断，减少误提示下载无关离线模型；切换列表或翻译设置后会刷新缺失模型提示。
- 布局：侧栏顶部导航和分类固定显示、下方列表独立滚动；修复分页时卡片被裁剪，并可设置单行列表悬停显示摘要。

## 0.1.17 — 2026-11-17

### English
- Sidebar: Categories now sit above Mail. Mail and Feeds can be shown as two tabs side by side, one at a time:
  right-click the Mail or Feeds heading and choose Show Mail and Feeds as tabs; right-click the tabs to switch back.
  Section headings are closer together.
- Plugins: the four example plugins ship with Postflow. The first time Postflow starts on a device (including after
  this update), it puts them in the plugins folder, turned off; turn them on in Settings → Plugins. Plugins and
  which ones are on are still per device and don't sync. If you delete an example, Settings → Plugins offers to
  install it again. The developer guide opens inside Postflow, so it
  works on networks where GitHub doesn't load.
- Adding an RSSHub feed: the instance settings box is gone from the form. The instance address now sits in the
  dialog's bottom bar (in the warning colour for the public rsshub.app); click it to change the address or key,
  test the connection or read the setup guide.

### 中文
- 侧栏：「分类」移到了「邮箱」上面。「邮箱」和「订阅」可以做成左右两个选项卡，一次显示一个：在这两组的标题上点右键，
  选「邮箱和订阅用选项卡切换」；在选项卡上点右键可以改回上下排列。分组之间的间距也缩小了。
- 插件：四个示例插件随 Postflow 一起打包。每台设备第一次启动 Postflow 时（包括这次更新后）会把它们放进插件目录，
  默认不启用，在「设置 → 插件」里打开即可。插件和启用哪些仍然只对这台设备有效，不会同步。删掉的示例可以在
  「设置 → 插件」里重新安装。
  开发文档在应用里直接打开，打不开 GitHub 的网络也能看。
- 添加 RSSHub 订阅：表单里不再放实例设置框，实例地址显示在对话框底栏（公共实例 rsshub.app 显示为警告色），
  点它可以改地址和密钥、测试连接、看部署指南。

## 0.1.16 — 2026-11-17

### English
- Getting started guide: after a fresh install, a short guide walks through language and theme, adding a mailbox,
  adding feeds (or importing OPML), the layout and main shortcuts, and where to find rules, translation, AI and
  plugins. Every step can be skipped, and it closes for good once skipped or finished. Open it again from
  Settings → About. People upgrading with mail or feeds already set up don't see it.
- RSSHub: the guide to running your own instance now opens inside Postflow, so it works on networks where GitHub
  doesn't load; commands in it have a copy button. When a route fails with HTTP 503 even though the connection test
  passes, Postflow now says the route needs setup on the RSSHub server (YouTube routes need YOUTUBE_KEY) and, for
  YouTube, points to Follow directly, which needs no RSSHub.
- Videos play in the reader: players that feeds embed (Bilibili, as RSSHub's Bilibili feeds do, YouTube, Vimeo,
  Tencent Video, Youku, Xigua, and Spotify, SoundCloud, Apple Music and NetEase Cloud Music for audio) are kept,
  full width and sandboxed. Other embedded frames are still removed. Bilibili's player no longer starts on its own.
  Items that link to a YouTube video (YouTube's own channel and playlist feeds) or a Bilibili video page get the
  player too, in place of the thumbnail.
- Turning off web-page view (or opening an item without a link) now empties the page, so a video in it stops
  instead of playing on in the background.
- View source: right-click an item for "View XML source" (that item as it appears in its feed, fetched again) or,
  for mail, "View message source" (the raw message), with a button to copy it.
- Add feed: the preview shows thumbnails for feeds that only put pictures in the text (such as RSSHub's Bilibili
  feeds); the name fills in with the feed's title until you change it; you can pick the feed's icon (the feed's
  own, the site's, or any image address); the name, folder and Add button stay at the bottom while long forms
  scroll. Going back from an RSSHub route keeps your search and its results.
- Right-click the empty part of the list for refresh, mark all as read and search, the layout (list, summary, cards,
  one or two lines), folding, browsing by scroll or pages, mail or feeds only, the show/hide switches, list
  translation and Customize toolbar.

### 中文
- 新手引导：第一次安装后，一步步带你选语言和主题、添加邮箱、添加订阅（或导入 OPML）、认识界面和常用快捷键，
  并告诉你自动规则、翻译、AI 和插件在哪里。每一步都可以跳过，跳过或看完后不再自动弹出，设置 → 关于里可以再打开。
  已经有邮箱或订阅的老用户升级后不会看到。
- RSSHub：「怎么部署自己的 RSSHub」改为在 Postflow 里直接打开，打不开 GitHub 的网络也能看，里面的命令可以一键拷贝。
  测试连接通过、但某个路由返回 HTTP 503 时，会说明这个路由要在 RSSHub 服务器上另外配置（YouTube 路由要设置 YOUTUBE_KEY），
  YouTube 还会提示用不需要 RSSHub 的「直接订阅」。
- 阅读区能直接播放视频：订阅源里嵌的播放器（RSSHub 的 B 站订阅就带着，还有 YouTube、Vimeo、腾讯视频、优酷、西瓜视频，
  以及 Spotify、SoundCloud、Apple Music、网易云音乐的音频播放器）会保留下来，占满正文宽度，放在沙箱里。别的内嵌框架仍然去掉。
  B 站播放器不再一打开就自动播放。
  链接是 YouTube 视频（YouTube 自己的频道、播放列表订阅）或 B 站视频页的条目，也会在正文上方放上播放器，代替原来的缩略图。
- 关掉原网页模式（或者打开没有链接的条目）时，网页会清空，里面的视频随之停下，不会在后台接着放。
- 查看源代码：在条目上右键选「查看 XML 源代码」（这一篇在订阅源里的原文，重新读取），邮件是「查看邮件源代码」（原始邮件），可以一键拷贝。
- 添加订阅：只在正文里放图的订阅源（比如 RSSHub 的 B 站订阅）预览里也有缩略图；名称自动填上订阅源的名字，改过就不再覆盖；
  可以选图标（订阅源的、网站的，或者填一个图片地址）；名称、文件夹和「添加」按钮固定在底部，表单再长也看得到。
  从 RSSHub 的路由返回时，搜索词和结果都还在。
- 中间列表的空白处可以右键：刷新、全部标为已读、搜索，显示方式（列表、摘要、卡片，一行或两行）、折叠、滚动或分页、
  只看邮件或订阅、各项显示开关、列表翻译，以及自定义工具栏。

## 0.1.15 — 2026-11-17

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
