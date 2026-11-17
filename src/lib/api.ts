import { rpc } from "./transport";

/** junk 和 trash 是邮箱里的垃圾邮件和已删除邮件。 */
export type View = "inbox" | "today" | "unread" | "starred" | "archived" | "junk" | "trash";

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

export interface Feed {
  id: string;
  url: string;
  title: string;
  site_url: string | null;
  icon_url: string | null;
  folder: string;
  interval_min: number | null;
  added_at: number;
  unread: number;
  last_fetch_at: number | null;
  error: string | null;
}

export interface Counts {
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
}

export interface Nav {
  counts: Counts;
  feeds: Feed[];
  accounts: Account[];
  /** 收件箱里每个分类的条数。 */
  labels: LabelCount[];
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
  sender_address: string | null;
  has_attachments: boolean;
  /** 卡片视图用的封面图，只有文章有。 */
  image: string | null;
  /** 分类。 */
  label: string | null;
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
  password: string;
  name?: string;
  imap_host?: string;
  imap_port?: number;
  imap_security?: Security;
  username?: string;
}

export interface AccountChanges {
  name?: string;
  password?: string;
  imap_host?: string;
  imap_port?: number;
  imap_security?: Security;
  username?: string;
}

export interface Cursor {
  published_at: number;
  id: string;
}

export interface ItemFilter {
  view: View;
  source_id?: string;
  kind?: ItemKind;
  folder?: string;
  since?: number;
  label?: string;
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
    patch: { title?: string; folder?: string; interval_min?: number | null },
  ) => rpc<void>("feeds.update", { id, ...patch }),
  removeFeed: (id: string) => rpc<void>("feeds.remove", { id }),
  /** 把这些订阅源按顺序放进一个文件夹，空字符串是文件夹外。 */
  reorderFeeds: (folder: string, ids: string[]) => rpc<void>("feeds.reorder", { folder, ids }),
  refresh: (id?: string) => rpc<{ count: number }>("feeds.refresh", { id }),
  importOpml: (content: string) =>
    rpc<{ added: number; total: number }>("feeds.import_opml", { content }),
  exportOpml: (path?: string) =>
    rpc<{ content?: string; path?: string; count: number }>("feeds.export_opml", { path }),

  listItems: (filter: ItemFilter, before?: Cursor, limit = 100) =>
    rpc<ItemPage>("items.list", { filter, before, limit }),
  getItem: (id: string, allowRemote = false) =>
    rpc<ItemDetail>("items.get", { id, allow_remote: allowRemote }),
  setItems: (ids: string[], patch: ItemPatch) => rpc<void>("items.set", { ids, ...patch }),
  markAllRead: (filter: ItemFilter) =>
    rpc<{ count: number }>("items.mark_all_read", { filter }),
  fullText: (id: string) => rpc<{ full_text: string }>("items.full_text", { id }),
  /** 原网页能不能嵌进 `origin` 这个页面（界面自己的地址）。 */
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
