import type { ReplyContext } from "./compose";
import { rpc } from "./transport";
import type { InstanceCheck, RsshubRoute } from "./rsshub";
import type { Rule } from "./rules";

/** junk 和 trash 是邮箱里的垃圾邮件和已删除邮件。 */
export type View =
  | "inbox"
  | "today"
  | "unread"
  | "starred"
  | "archived"
  | "junk"
  | "trash"
  | "drafts"
  | "sent"
  /** 邮箱里的某个其他文件夹，和 `source_id`、`mailbox` 一起用。 */
  | "folder"
  /** 稍后提醒着、还没到时间的。 */
  | "snoozed"
  /** 提醒时间刚到、还没读的：收件箱顶部单独列出来。 */
  | "reminders"
  /** 稍后读：自己存下来的网页（只在订阅那边）。 */
  | "later"
  /** 发出去、设了「没回复就提醒」、还在等回复的信。 */
  | "waiting";

export interface Status {
  configured: boolean;
  version: string;
  data_dir: string | null;
  device_id: string | null;
  device_name: string | null;
  suggested_data_dir: string;
  suggested_device_name: string;
  error: string | null;
}

/** 搜到的一个播客节目。 */
export interface PodcastResult {
  title: string;
  author: string;
  feed_url: string;
  artwork: string | null;
  genre: string | null;
  episodes: number | null;
}

/** 下载离线听的单集。 */
export interface EpisodeList {
  downloaded: { id: string; file: string; size: number; at: number }[];
  /** 正在下的：已下载的字节数、总大小、失败的原因。 */
  downloading: Record<string, { received: number; total: number | null; error: string | null }>;
}

/** 订阅源的类型：文章、视频、播客。 */
export type FeedKind = "article" | "video" | "podcast";

/** 订阅那边按类型看：订阅源的几类，加上放到订阅里看的订阅邮件（Newsletter）。 */
export type ContentKind = FeedKind | "newsletter";

/** 放到订阅里看的一个订阅邮件发件人。 */
export interface Newsletter {
  /** 小写的发件人地址。 */
  address: string;
  /** 最近一封信里的名字，没有时为空。 */
  name: string;
  unread: number;
  total: number;
}

/** 目录里的一个语音识别模型。 */
export interface SpeechModel {
  id: string;
  name: string;
  kind: string;
  /** 能识别的语言代码；`multi` 表示很多种。 */
  languages: string[];
  /** 推荐给这些界面语言的用户先装。 */
  recommended: string[];
  license: string;
  license_url: string;
  homepage: string;
  desc: Record<string, string>;
  /** 要下载多少字节。 */
  size: number;
  installed: boolean;
  /** 这个版本的 Postflow 能不能加载它。 */
  supported: boolean;
  /** 目录里已经撤掉了（本机还装着）。 */
  retired: boolean;
  /** 正在下载的话，已经下了多少。 */
  received: number | null;
}

export interface SpeechOverview {
  models: SpeechModel[];
  selected: string | null;
  /** 模型放在哪儿。 */
  dir: string;
}

/** 一句字幕，时间是秒。 */
export interface SubtitleCue {
  start: number;
  end: number;
  text: string;
}

export interface SubtitleSnapshot {
  id: string;
  state: "idle" | "downloading" | "loading" | "running" | "failed";
  /** 用哪个模型转的。 */
  model: string;
  duration: number | null;
  /** 已经转过的时间段。 */
  covered: [number, number][];
  complete: boolean;
  cues: SubtitleCue[];
  /** 等这一期下载到本机时的进度。 */
  received: number;
  total: number | null;
  error: string | null;
  message: string | null;
}

/** 条目附带的音频或视频。 */
export interface Media {
  url: string;
  /** MIME 类型，比如 audio/mpeg。 */
  mime: string;
  /** 时长，秒。 */
  duration?: number;
  size?: number;
  /** 章节文件（Podcasting 2.0 JSON）的地址。 */
  chapters?: string;
  /** 文字稿的地址和格式。 */
  transcript?: { url: string; mime: string };
}

export interface Feed {
  id: string;
  url: string;
  title: string;
  site_url: string | null;
  icon_url: string | null;
  /** 类型：用户选的，没选时按内容自动判断。 */
  kind: FeedKind;
  /** 用户选的类型；null 表示自动。 */
  kind_set: FeedKind | null;
  /** 打开这个源的文章时自动取全文。 */
  full_text: boolean;
  folder: string;
  interval_min: number | null;
  added_at: number;
  unread: number;
  last_fetch_at: number | null;
  error: string | null;
}

export interface Counts {
  /** 稍后读里的未读数。 */
  later: number;
  /** 设了「没回复就提醒」、还在等回复的信。 */
  waiting?: number;
  unread: number;
  today: number;
  starred: number;
  unread_mail: number;
  today_mail: number;
  unread_article: number;
  today_article: number;
}

export type Security = "tls" | "starttls" | "none";

export interface Account {
  id: string;
  email: string;
  name: string;
  imap_host: string;
  imap_port: number;
  imap_security: Security;
  username: string;
  /** 自己填的发信服务器；为空表示自动。 */
  smtp_host: string | null;
  smtp_port: number | null;
  smtp_security: Security | null;
  /** 发信时收件人看到的名字。`name` 只是侧栏里的邮箱名。 */
  sender_name: string;
  /** 签名，写信时自动加在正文后面。 */
  signature: string;
  added_at: number;
  unread: number;
  last_sync_at: number | null;
  error: string | null;
  error_code: string | null;
}

export interface LabelCount {
  label: string;
  unread: number;
  total: number;
  /** 其中邮件的条数，文章的是两者之差。 */
  unread_mail: number;
  total_mail: number;
}

export interface Nav {
  counts: Counts;
  feeds: Feed[];
  accounts: Account[];
  /** 收件箱里每个分类的条数。 */
  labels: LabelCount[];
  /** 放到订阅里看的订阅邮件发件人。 */
  newsletters?: Newsletter[];
  /** 「稍后读」内置订阅源的 ID（存下的网页的 source_id）。 */
  later_id?: string;
}

/** 附件总览里的一个附件。 */
export interface AttachmentRow {
  item_id: string;
  index: number;
  name: string;
  mime: string;
  size: number;
  title: string;
  from_name: string | null;
  from_address: string | null;
  published_at: number;
  account_id: string;
  /** 正文已经下载了。没下载的（大邮件）保存时先下载正文。 */
  downloaded: boolean;
}

/** 文章里的一段划线，可以带一条笔记。 */
export interface Highlight {
  id: string;
  item: string;
  quote: string;
  prefix: string;
  suffix: string;
  note: string;
  created_at: number;
}

/** 「全部笔记」里的一段，带着它所在的文章。 */
export interface HighlightWithItem extends Highlight {
  title: string;
  url: string | null;
  source_title: string;
}

/** 存进稍后读的结果。`warning`：网页打不开或认不出正文，条目照样存了，打开时再试。 */
export interface LaterSaved {
  id: string;
  title: string;
  existed: boolean;
  warning?: string;
}

export type ItemKind = "article" | "mail";

export interface Item {
  id: string;
  kind: ItemKind;
  /** 订阅源 ID 或邮箱账户 ID。 */
  source_id: string;
  source_title: string;
  source_icon: string | null;
  url: string | null;
  title: string;
  author: string | null;
  summary: string;
  published_at: number;
  read: boolean;
  starred: boolean;
  archived: boolean;
  /** 邮箱「草稿」里的一封草稿：打开时接着写。 */
  draft?: boolean;
  /** 「已发送」里自己发出去的信。 */
  sent?: boolean;
  sender_address: string | null;
  has_attachments: boolean;
  /** 卡片视图用的封面图，只有文章有。 */
  image: string | null;
  /** 分类。 */
  label: string | null;
  /** 稍后提醒的时间（毫秒）。没设过是 null；已经过了的也还在。 */
  snoozed_until?: number | null;
  /** 搜索时，正文里命中搜索词的那一小段。 */
  snippet?: string | null;
  /** 播客、视频的音频或视频附件。 */
  media?: Media | null;
}

export interface Mailbox {
  name: string | null;
  address: string | null;
}

export interface Attachment {
  index: number;
  name: string;
  mime: string;
  size: number;
}

export interface MailDetail {
  /** html：完整文档，放进沙箱 iframe；text：安全的 HTML 片段。 */
  body_kind: "html" | "text";
  body: string;
  has_remote_content: boolean;
  remote_allowed: boolean;
  sender_trusted: boolean;
  from: Mailbox;
  to: Mailbox[];
  cc: Mailbox[];
  reply_to: Mailbox[];
  attachments: Attachment[];
  /** 邮件列表的退订方式；不是邮件列表时为空。 */
  unsubscribe?: { url: string | null; mailto: string | null; one_click: boolean } | null;
  /** 会议邀请（邮件里带着 iCalendar 时）。 */
  invite?: Invite | null;
  /** 我对这个邀请的回复：ACCEPTED、TENTATIVE、DECLINED。 */
  rsvp?: string | null;
  /** 已发送的信设了「没回复就提醒」：提醒的时间（毫秒）。 */
  follow_up?: number | null;
}

/** 邮件里的会议邀请。时间是 UTC 毫秒；算不出时区时只有 `start_local`。 */
export interface Invite {
  /** REQUEST（邀请、更新）、CANCEL（取消）、REPLY（别人的回复）。 */
  method: string;
  uid: string;
  sequence: number;
  summary: string;
  location: string | null;
  start: number | null;
  end: number | null;
  start_local: string | null;
  all_day: boolean;
  organizer: Mailbox | null;
  attendees: { name: string | null; address: string; status: string }[];
  url: string | null;
  /** 周期会议；`occurrence`：周期会议里单独改过的某一次。 */
  recurring: boolean;
  occurrence: boolean;
}

export interface ItemDetail extends Item {
  /** 文章的正文。邮件没有这两项，看 mail。 */
  content?: string;
  full_text?: string | null;
  mail?: MailDetail;
}

export interface Preset {
  id: string;
  name: string;
  imap_host: string;
  imap_port: number;
  imap_security: Security;
  /** password、app_password、auth_code 或 oauth。 */
  password_kind: "password" | "app_password" | "auth_code" | "oauth";
  /** 这个服务商的邮箱域名；企业邮箱为空。 */
  domains: string[];
}

export interface AccountGuess {
  imap_host: string;
  imap_port: number;
  imap_security: Security;
  username: string;
  preset: Preset | null;
}

export interface NewAccount {
  email: string;
  /** 用微软账户登录时不填，改给 `oauth`。 */
  password?: string;
  /** `oauthStart` / `oauthWait` 完成的那次登录。 */
  oauth?: string;
  name?: string;
  imap_host?: string;
  imap_port?: number;
  imap_security?: Security;
  username?: string;
}

export interface AccountChanges {
  name?: string;
  password?: string;
  /** 重新用微软账户登录。 */
  oauth?: string;
  imap_host?: string;
  imap_port?: number;
  imap_security?: Security;
  username?: string;
  /** 空字符串表示改回自动。 */
  smtp_host?: string;
  smtp_port?: number;
  smtp_security?: Security;
  sender_name?: string;
  signature?: string;
}

/** 回复时接上同一个讨论用的两个信头。 */
export interface Thread {
  in_reply_to: string | null;
  references: string[];
}

/** 交给核心的一个附件：之后发信、存草稿只传 `token`。 */
export interface AttachedFile {
  token: string;
  name: string;
  mime: string;
  size: number;
}

/** 「加载更早的邮件」从哪里取。 */
export interface OlderTarget {
  account_id?: string;
  folder: "inbox" | "sent" | "drafts" | "junk" | "trash" | "folder";
  mailbox?: string;
}

/** 用哪家的账户登录邮箱。 */
export type OAuthProvider = "microsoft" | "google";

/** 邮箱里的一个普通文件夹。 */
export interface FolderInfo {
  /** 服务器上的原名，打开时用。 */
  name: string;
  /** 给人看的完整名字，按 `delimiter` 分层。 */
  path: string;
  delimiter: string | null;
  /** 只是用来装子文件夹、自己打不开的。 */
  selectable: boolean;
}

/** 写信窗口里的一封信。发送和存草稿用同一个样子。 */
/** 定时发送里的一封。 */
export interface OutboxItem {
  id: string;
  send_at: number;
  account_id: string;
  to: string[];
  subject: string;
  attachments: number;
  /** 上次发送失败的原因。 */
  error: string | null;
}

export interface Outgoing {
  account_id: string;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  /** 纯文本正文；带格式时是 `html` 的纯文本版。 */
  text: string;
  /** 带格式的正文，纯文本时为空。 */
  html?: string | null;
  in_reply_to?: string | null;
  references?: string[];
  /** 附件的 token。 */
  attachments?: string[];
  /** 这封信上次存成的草稿的 Message-ID：发出去或者再存时删掉它。 */
  draft?: string | null;
  /** 没回复就提醒：到这个时间（毫秒）还没收到回复就提醒。 */
  follow_up?: number | null;
}

/** 一封草稿里写了什么（`mail.draft`）。 */
export interface DraftContents {
  account_id: string;
  message_id: string | null;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  text: string;
  /** 带格式的草稿，已经清理过。 */
  html: string | null;
  thread: Thread;
  attachments: AttachedFile[];
}

export interface Contact {
  name: string | null;
  address: string;
  count: number;
}

export interface SmtpSettings {
  host: string;
  port: number;
  security: Security;
  /** 自己填的，不是自动推断的。 */
  custom: boolean;
}

export interface Cursor {
  published_at: number;
  id: string;
}

export interface ItemFilter {
  /** 「all」是收件箱和归档里的全部，搜索「全部」时用。 */
  view: View | "all";
  source_id?: string;
  kind?: ItemKind;
  folder?: string;
  /** 邮箱里的哪个文件夹（服务器上的名字），和 view: "folder" 一起用。 */
  mailbox?: string;
  since?: number;
  label?: string;
  /** 全文搜索：标题、发件人、作者和正文里都要有这些词。 */
  query?: string;
  /** 当地时间比 UTC 快多少分钟（搜索里的日期用）。 */
  tz_offset?: number;
  /** 只看某一类订阅源的文章；newsletter 是放到订阅里看的订阅邮件。 */
  feed_kind?: ContentKind;
  /** 只看这个发件人的邮件。 */
  sender?: string;
}

export interface ItemPage {
  items: Item[];
  next: Cursor | null;
}

export interface ItemPatch {
  read?: boolean;
  starred?: boolean;
  archived?: boolean;
}

export interface Candidate {
  url: string;
  title: string;
}

/** 添加订阅前的预览。 */
export interface FeedPreview {
  url: string;
  title: string;
  site_url: string | null;
  icon_url: string | null;
  description: string | null;
  entry_count: number;
  newest_ms: number | null;
  oldest_ms: number | null;
  entries: { title: string; url: string | null; published_ms: number | null; summary: string; image: string | null }[];
  kind: "youtube" | null;
}

export interface Preview {
  feed: FeedPreview;
  candidates: Candidate[];
}

export interface Device {
  id: string;
  name: string;
  current: boolean;
}

export interface ImportStats {
  files: number;
  ops: number;
  articles: number;
  skipped: number;
  unreadable: number;
}

export type Settings = Record<string, unknown>;

/** 一个订阅源的统计。阅读相关的数字来自本机的阅读记录。 */
export interface FeedStats {
  id: string;
  title: string;
  url: string;
  icon_url: string | null;
  folder: string;
  added_at: number;
  error: string | null;
  total: number;
  last_published: number | null;
  /** 统计期内发布的文章数。 */
  published: number;
  /** 每周发布几篇，最早的一周在前。 */
  weekly: number[];
  /** 最近几篇里带全文的比例。 */
  full_share: number | null;
  read: number;
  opened: number;
  finished: number;
  avg_depth: number | null;
  dwell_ms: number;
}

export interface ReadingDay {
  /** 从 1970-01-01 起的第几天（本地时间）。 */
  day: number;
  published: number;
  opened: number;
  dwell_ms: number;
}

export interface Stats {
  since: number;
  now: number;
  tracking_since: number | null;
  feeds: FeedStats[];
  days: ReadingDay[];
}

/** 一个配置好的 AI 服务。密钥不在这里，在系统钥匙串里。 */
export interface AiProvider {
  id: string;
  preset: string;
  name: string;
  base_url: string;
  model: string;
  proxy?: string | null;
}

export interface AiProviderView extends AiProvider {
  has_key: boolean;
  /** 在这台电脑上运行（Ollama、LM Studio）。 */
  local: boolean;
}

export interface AiPreset {
  id: string;
  name: string;
  base_url: string;
  model: string;
  key_url: string;
  local: boolean;
}

export interface AiProviders {
  providers: AiProviderView[];
  default: string | null;
  fast: string | null;
  allow_mail: boolean;
}

export interface AiResult {
  task: string;
  lang: string;
  provider: string;
  model: string;
  content: string;
  created_at: number;
}

export interface AiMessage {
  id: number;
  role: "user" | "assistant";
  content: string;
  created_at: number;
}

/** 用户定义的一个分类。 */
export interface Category {
  name: string;
  description: string;
}

export interface AiUsage {
  provider: string;
  requests: number;
  input: number;
  output: number;
}

/** 已经下载到本机的翻译模型。语言代码用的是模型目录里的写法，比如 `zh`、`zh_hant`。 */
export interface InstalledModel {
  pair: string;
  from: string;
  to: string;
  size: number;
}

export interface ModelProgress {
  pair: string;
  received: number;
  total: number;
}

export interface TranslateLanguages {
  /** 拿到了模型目录。离线又没存过目录时为 false，只能用已经下载的模型。 */
  catalog: boolean;
  sources: string[];
  targets: string[];
  installed: InstalledModel[];
  downloads: ModelProgress[];
}

export interface MissingModel {
  pair: string;
  from: string;
  to: string;
  size: number;
}

export interface TranslatePlan {
  from: string | null;
  to: string;
  same: boolean;
  missing: MissingModel[];
}

/** 要翻译的一段：纯文字，加上里面每个行内元素覆盖的 `[start, end)`（UTF-16 下标）。 */
export interface SourceSegment {
  text: string;
  marks: [number, number][];
}

/** 译文：每个标记是 `[start, end, 原文里第几个元素]`。 */
export interface TranslatedSegment {
  text: string;
  marks: [number, number, number][];
}

export const api = {
  status: () => rpc<Status>("app.status"),
  setup: (data_dir: string, device_name: string) =>
    rpc<Status>("app.setup", { data_dir, device_name }),

  nav: (today_since: number) => rpc<Nav>("nav.get", { today_since }),

  discover: (url: string) => rpc<Candidate[]>("feeds.discover", { url }),
  preview: (url: string) => rpc<Preview>("feeds.preview", { url }),
  addFeed: (url: string, folder: string, extra: { title?: string; icon_url?: string | null } = {}) =>
    rpc<Feed>("feeds.add", { url, folder, ...extra }),
  updateFeed: (
    id: string,
    patch: {
      title?: string;
      folder?: string;
      interval_min?: number | null;
      url?: string;
      /** null 表示按内容自动判断。 */
      kind?: FeedKind | null;
      full_text?: boolean;
    },
  ) => rpc<void>("feeds.update", { id, ...patch }),
  removeFeed: (id: string) => rpc<void>("feeds.remove", { id }),
  /** 把这些订阅源按顺序放进一个文件夹，空字符串是文件夹外。 */
  reorderFeeds: (folder: string, ids: string[]) => rpc<void>("feeds.reorder", { folder, ids }),
  /** 一篇文章的划线和笔记。 */
  highlights: (item: string) => rpc<{ highlights: Highlight[] }>("highlights.list", { item }),
  /** 全部划线和笔记，带着文章标题，导出用。 */
  allHighlights: () => rpc<{ highlights: HighlightWithItem[] }>("highlights.list", {}),
  addHighlight: (h: Omit<Highlight, "created_at">) => rpc<Highlight>("highlights.add", h),
  setHighlightNote: (id: string, note: string) => rpc<void>("highlights.note", { id, note }),
  removeHighlight: (id: string) => rpc<void>("highlights.remove", { id }),
  /** 取消一封已发送邮件的「没回复就提醒」。 */
  cancelFollowUp: (id: string) => rpc<void>("mail.follow_up_cancel", { id }),
  /** 回复会议邀请：ACCEPTED、TENTATIVE、DECLINED。 */
  rsvp: (id: string, status: "ACCEPTED" | "TENTATIVE" | "DECLINED") => rpc<{ status: string }>("mail.rsvp", { id, status }),
  /** 邀请的 .ics：`save` 时存到本机临时目录给出路径（桌面应用），否则给出文字。 */
  inviteFile: (id: string, save: boolean) => rpc<{ path?: string; ics?: string }>("mail.invite_file", { id, save }),
  /** 发件人卡片：和这个地址的往来。 */
  senderProfile: (address: string) =>
    rpc<{
      address: string;
      name: string | null;
      received: number;
      unread: number;
      sent: number;
      first_at: number | null;
      last_at: number | null;
      recent: Item[];
    }>("mail.sender", { address }),
  /** 附件总览：最近邮件里的附件，按文件名、标题、发件人搜。 */
  mailAttachments: (query: string) =>
    rpc<{ files: AttachmentRow[]; not_downloaded: Item[] }>("mail.attachments", { query: query || null }),
  /** 把一个网页存进稍后读（核心抓下标题和正文）。 */
  laterSave: (url: string) => rpc<LaterSaved>("later.save", { url }),
  /** 导入 Pocket、Instapaper 的导出文件或浏览器书签（文件的文字内容）。 */
  laterImport: (text: string) => rpc<{ found: number; added: number }>("later.import", { text }),
  /** 下载在线的推荐订阅源目录（地址固定，见核心的 `catalog.fetch`）。 */
  catalogFetch: () => rpc<{ text: string; url: string }>("catalog.fetch"),
  episodesList: () => rpc<EpisodeList>("episodes.list"),
  /** 在后台下载这一期，只保留最近下载的 `keep` 期。 */
  downloadEpisode: (id: string, keep: number) => rpc<void>("episodes.download", { id, keep }),
  removeEpisode: (id: string) => rpc<void>("episodes.remove", { id }),
  /** 取播客的章节、文字稿文件（由核心去取）。 */
  mediaText: (url: string) => rpc<{ text: string }>("media.text", { url }),
  /** 语音识别模型：目录里有哪些、装了哪些、现在用哪个。 */
  speechOverview: () => rpc<SpeechOverview>("speech.overview"),
  /** 在后台下载一个识别模型，进度通过 speech_model 事件来。 */
  speechDownload: (id: string) => rpc<void>("speech.download", { id }),
  speechRemove: (id: string) => rpc<SpeechOverview>("speech.remove", { id }),
  speechSelect: (id: string) => rpc<SpeechOverview>("speech.select", { id }),
  /** 从本机的一个文件夹装模型（文件照样校验）。 */
  speechImport: (id: string, path: string) => rpc<SpeechOverview>("speech.import", { id, path }),
  /** 这一期在本机转写出来的字幕（没有就是空的）。 */
  subtitlesGet: (id: string) => rpc<SubtitleSnapshot>("subtitles.get", { id }),
  /**
   * 开始（或者接着）转写这一期，优先从 `position` 秒往后；已经在转的话只是告诉它现在放到哪儿了。
   * 音频还没下载到本机会先下载，本机一共留 `keep` 期。
   */
  subtitlesStart: (id: string, position: number, keep: number) =>
    rpc<SubtitleSnapshot>("subtitles.start", { id, position, keep }),
  subtitlesStop: (id: string) => rpc<SubtitleSnapshot>("subtitles.stop", { id }),
  /** 删掉这一期的字幕（换个模型重新转之前）。 */
  subtitlesRemove: (id: string) => rpc<SubtitleSnapshot>("subtitles.remove", { id }),
  subtitlesClear: () => rpc<void>("subtitles.clear"),
  /** 按节目名搜播客（苹果播客目录）。`country` 是两个字母的地区代码。 */
  searchPodcasts: (term: string, country: string) =>
    rpc<PodcastResult[]>("podcasts.search", { term, country }),
  /** 刷新订阅源；不给 `id` 时全部刷新，邮箱也一起收，除非 `feedsOnly`。 */
  refresh: (id?: string, feedsOnly = false) =>
    rpc<{ count: number }>("feeds.refresh", feedsOnly ? { id, feeds_only: true } : { id }),
  importOpml: (content: string) =>
    rpc<{ added: number; total: number }>("feeds.import_opml", { content }),
  exportOpml: (path?: string) =>
    rpc<{ content?: string; path?: string; count: number }>("feeds.export_opml", { path }),

  listItems: (filter: ItemFilter, before?: Cursor, limit = 100) =>
    rpc<ItemPage>("items.list", { filter, before, limit }),
  getItem: (id: string, allowRemote = false) =>
    rpc<ItemDetail>("items.get", { id, allow_remote: allowRemote }),
  setItems: (ids: string[], patch: ItemPatch) => rpc<void>("items.set", { ids, ...patch }),
  /** 稍后提醒到 `until`（毫秒）；`null` 取消。 */
  snooze: (ids: string[], until: number | null) => rpc<void>("items.snooze", { ids, until }),
  markAllRead: (filter: ItemFilter) =>
    rpc<{ count: number }>("items.mark_all_read", { filter }),
  fullText: (id: string) => rpc<{ full_text: string }>("items.full_text", { id }),
  /** 查看源代码：邮件的原始邮件，或者文章在订阅源里的 XML（`whole`：这篇已经不在源里，给的是整个源）。 */
  itemSource: (id: string) =>
    rpc<{ kind: "mail" | "xml"; source: string; whole: boolean; url?: string }>("items.source", { id }),
  /** 原网页能不能嵌进 `origin` 这个页面（界面自己的地址）。 */
  rsshubSearch: (query: string, zh: boolean) =>
    rpc<{ routes: RsshubRoute[]; total: number }>("rsshub.search", { query, zh }),
  rsshubRadar: (url: string, zh: boolean) =>
    rpc<{ matches: { route: RsshubRoute; values: Record<string, string> }[] }>("rsshub.radar", { url, zh }),
  /** 试一条规则：最新的条目里有多少会命中。不改动任何东西。 */
  rulesTest: (rule: Rule) =>
    rpc<{ matched: number; checked: number; samples: { id: string; title: string; source: string; kind: ItemKind }[] }>(
      "rules.test",
      { rule },
    ),
  /** 把一条规则（不给就是全部启用的规则）应用到已有的条目。 */
  rulesApply: (rule?: Rule) => rpc<{ matched: number }>("rules.apply", rule ? { rule } : {}),
  rsshubCheck: (instance: string, key?: string) => rpc<InstanceCheck>("rsshub.check", { instance, key }),
  storageUsage: () => rpc<StorageUsage>("storage.usage"),
  /** 清理本机缓存：抓取的全文、30 天前的邮件原文、发件人图标。清完会顺手压缩索引。 */
  clearCache: (what: "fulltext" | "mail" | "icons") => rpc<{ count: number }>("storage.clear", { what }),
  compactIndex: () => rpc<Record<string, never>>("storage.compact"),
  /** 发件人所在网站的图标地址，没有时为 null。核心会把结果存在本机。 */
  senderIcon: (domain: string) => rpc<{ url: string | null }>("mail.senderIcon", { domain }),
  webCheck: (url: string, origin: string) =>
    rpc<{ embeddable: boolean; reason: string | null; final_url: string }>("web.check", { url, origin }),

  presets: () => rpc<{ presets: Preset[] }>("mail.presets"),
  guessAccount: (email: string) => rpc<AccountGuess>("accounts.guess", { email }),
  addAccount: (account: NewAccount) => rpc<Account>("accounts.add", account),
  updateAccount: (id: string, changes: AccountChanges) =>
    rpc<Account>("accounts.update", { id, ...changes }),
  removeAccount: (id: string) => rpc<void>("accounts.remove", { id }),
  reorderAccounts: (ids: string[]) => rpc<void>("accounts.reorder", { ids }),
  /** 收信。`auto` 为真表示是应用自己发起的，不是用户点的刷新。 */
  syncAccount: (id?: string, auto = false) => rpc<void>("accounts.sync", { id, auto }),
  attachment: (id: string, index: number, path?: string) =>
    rpc<{ name: string; mime?: string; data?: string; path?: string }>("mail.attachment", {
      id,
      index,
      path,
    }),
  trustSender: (address: string) => rpc<void>("mail.trust_sender", { address }),
  /** 一键退订（RFC 8058）。 */
  unsubscribe: (id: string) => rpc<void>("mail.unsubscribe", { id }),
  /** 回复、转发一封信要用到的原信内容。转发时原信的附件也放进待发的附件里。 */
  composeContext: (id: string, forward = false) =>
    rpc<ReplyContext>("mail.compose_context", { id, forward }),
  send: (mail: Outgoing) => rpc<{ message_id: string }>("mail.send", mail),
  /** 定时发送：存在本机，到 `sendAt`（毫秒）时由核心发出。 */
  schedule: (mail: Outgoing, sendAt: number) => rpc<{ id: string }>("mail.schedule", { ...mail, send_at: sendAt }),
  /** 定时发送里还没发的。 */
  outbox: () => rpc<{ items: OutboxItem[] }>("mail.outbox").then((r) => r.items),
  /** 定时发送的这一封现在就发。 */
  outboxSend: (id: string) => rpc<void>("mail.outbox_send", { id }),
  /** 取消定时发送，存回邮箱的草稿里。 */
  unschedule: (id: string) => rpc<void>("mail.unschedule", { id }),
  /** 存成草稿，返回新草稿的 Message-ID 和它在列表里的 ID。 */
  saveDraft: (mail: Outgoing) => rpc<{ message_id: string; item_id: string }>("mail.save_draft", mail),
  deleteDraft: (accountId: string, messageId: string) =>
    rpc<void>("mail.delete_draft", { account_id: accountId, message_id: messageId }),
  /** 删掉列表里的一封草稿。 */
  deleteDraftItem: (id: string) => rpc<void>("mail.delete_draft", { id }),
  /** 接着写一封草稿。 */
  draft: (id: string) => rpc<DraftContents>("mail.draft", { id }),
  /** 加一个附件，`data` 是 Base64。 */
  attach: (name: string, mime: string, data: string) => rpc<AttachedFile>("mail.attach", { name, mime, data }),
  /** 写信窗口关了：附件不用再留着。 */
  attachDrop: (tokens: string[]) => rpc<void>("mail.attach_drop", { tokens }),
  /** 给你来过信的人，写信时提示收件人。 */
  contacts: () => rpc<{ contacts: Contact[] }>("mail.contacts"),
  /** 从服务器取更早的邮件。不给邮箱就是全部邮箱。 */
  loadOlder: (target: OlderTarget) => rpc<{ added: number; more: boolean }>("mail.load_older", target),
  /** 把邮件移到同一个邮箱的另一个文件夹（服务器上的名字，收件箱是 INBOX）。 */
  moveMail: (ids: string[], mailbox: string) => rpc<{ moved: number }>("mail.move", { ids, mailbox }),
  /** 邮箱里的普通文件夹。 */
  folders: (accountId: string) => rpc<{ folders: FolderInfo[] }>("mail.folders", { account_id: accountId }),
  /** 打开一个普通文件夹：马上同步它，之后每次收信也带上它。 */
  openFolder: (accountId: string, mailbox: string) =>
    rpc<void>("mail.open_folder", { account_id: accountId, mailbox }),
  /** 账户现在用的发信服务器。 */
  smtpSettings: (id: string) => rpc<SmtpSettings>("accounts.smtp", { id }),
  /** 能不能用微软账户登录（要有 Azure 应用 ID）。 */
  oauthConfig: () => rpc<{ microsoft: boolean; google: boolean }>("accounts.oauth_config"),
  /** 这个邮箱在这台设备上怎么登录。 */
  signInMethod: (id: string) =>
    rpc<{ method: "password" | "microsoft" | "google" | "none" }>("accounts.sign_in_method", { id }),
  /** 开始用微软账户登录，返回要在浏览器里打开的地址。 */
  oauthStart: (email: string, provider: OAuthProvider = "microsoft") =>
    rpc<{ flow_id: string; url: string }>("accounts.oauth_start", { email, provider }),
  /** 等用户在浏览器里登录完。 */
  oauthWait: (flowId: string) => rpc<void>("accounts.oauth_wait", { flow_id: flowId }),
  oauthCancel: (flowId: string) => rpc<void>("accounts.oauth_cancel", { flow_id: flowId }),

  settings: () => rpc<Settings>("settings.get"),
  setSetting: (key: string, value: unknown) => rpc<void>("settings.set", { key, value }),

  recordReading: (p: { item_id: string; dwell_ms: number; depth: number; finished: boolean }) =>
    rpc<{ saved: boolean }>("stats.reading", p),
  feedStats: (days: number) =>
    rpc<Stats>("stats.feeds", { days, tz_offset_min: -new Date().getTimezoneOffset() }),
  clearReading: () => rpc<void>("stats.clear"),

  devices: () => rpc<Device[]>("devices.list"),
  renameDevice: (name: string) => rpc<void>("devices.rename", { name }),

  rebuildIndex: () => rpc<ImportStats>("index.rebuild"),

  /** 最近的日志。`available` 为 false 时这个运行环境没有写日志。 */
  logs: (lines = 600) =>
    rpc<{ available: boolean; dir: string | null; verbose: boolean; text: string }>("app.logs", { lines }),
  log: (level: "error" | "warn" | "info", message: string) => rpc<void>("app.log", { level, message }),
  setLogging: (verbose: boolean) => rpc<{ ok: boolean }>("app.logging", { verbose }),

  aiPresets: () => rpc<{ presets: AiPreset[] }>("ai.presets"),
  aiProviders: () => rpc<AiProviders>("ai.providers"),
  saveAiProvider: (provider: Partial<AiProvider> & { base_url: string }, api_key?: string) =>
    rpc<AiProvider>("ai.providers.save", { provider, api_key }),
  removeAiProvider: (id: string) => rpc<void>("ai.providers.remove", { id }),
  testAiProvider: (provider: Partial<AiProvider> & { base_url: string }, api_key?: string) =>
    rpc<{ ok: boolean; reply: string }>("ai.providers.test", { provider, api_key }),
  aiModels: (provider: Partial<AiProvider> & { base_url: string }, api_key?: string) =>
    rpc<{ models: string[] }>("ai.models", { provider, api_key }),
  aiResults: (id: string, lang: string) =>
    rpc<{ summary: AiResult | null; keywords: AiResult | null }>("ai.results", { id, lang }),
  aiRun: (id: string, task: "summary" | "keywords", lang: string, force = false) =>
    rpc<{ cached?: AiResult; request_id?: string }>("ai.run", { id, task, lang, force }),
  aiAbstracts: (ids: string[], lang: string) =>
    rpc<{ abstracts: Record<string, string> }>("ai.abstracts", { ids, lang }),
  aiChat: (id: string, message: string, lang: string) =>
    rpc<{ request_id: string; message: AiMessage }>("ai.chat", { id, message, lang }),
  aiChatHistory: (id: string) => rpc<{ messages: AiMessage[] }>("ai.chat.history", { id }),
  aiChatClear: (id: string) => rpc<void>("ai.chat.clear", { id }),
  aiCancel: (request_id: string) => rpc<void>("ai.cancel", { request_id }),
  classify: (ids?: string[]) => rpc<{ labels: Record<string, string | null> }>("ai.classify", { ids }),
  /** 手动定分类，`null` 是「哪一类都不是」。 */
  setLabel: (ids: string[], label: string | null) => rpc<void>("items.label", { ids, label }),
  aiUsage: () => rpc<{ today: AiUsage[]; last30: AiUsage[] }>("ai.usage"),

  aiTranslate: (id: string | undefined, from: string, to: string, segments: SourceSegment[]) =>
    rpc<{ segments: TranslatedSegment[] }>("ai.translate", { id, from, to, segments }),
  detectLanguage: (text: string) => rpc<{ from: string | null }>("translate.detect", { text }),
  /** 列表里条目的标题和摘要的译文。已经是目标语言的在 `same` 里；离线模型缺了的在 `missing` 里。 */
  translateItems: (ids: string[], to: string, engine: "offline" | "ai") =>
    rpc<{
      items: Record<string, { from: string; title: string; summary: string }>;
      same: string[];
      missing: MissingModel[];
    }>("translate.items", { ids, to, engine }),

  translateLanguages: () => rpc<TranslateLanguages>("translate.languages"),
  translatePlan: (text: string, to: string, from?: string) =>
    rpc<TranslatePlan>("translate.plan", { text, to, from }),
  downloadModels: (pairs: string[]) => rpc<void>("translate.download", { pairs }),
  removeModel: (pair: string) => rpc<void>("translate.remove", { pair }),
  translateText: (from: string, to: string, segments: SourceSegment[]) =>
    rpc<{ segments: TranslatedSegment[] }>("translate.text", { from, to, segments }),
};

export interface StoragePart {
  key: "articles" | "oplog" | "index" | "translation" | "speech" | "episodes" | "logs" | "icons" | "rsshub" | "other";
  bytes: number;
}

export interface StorageUsage {
  data_dir: string;
  data: { total: number; parts: StoragePart[] };
  local_dir: string;
  local: { total: number; parts: StoragePart[] };
  index: {
    articles: number;
    fulltext: number;
    mail: number;
    mail_clearable: number;
    sources: { source_id: string; kind: ItemKind; items: number; bytes: number }[];
  };
  keep_mail_days: number;
}
