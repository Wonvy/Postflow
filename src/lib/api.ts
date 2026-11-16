import { rpc } from "./transport";

export type View = "inbox" | "today" | "unread" | "starred" | "archived";

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

export interface Nav {
  counts: Counts;
  feeds: Feed[];
  accounts: Account[];
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

export const api = {
  status: () => rpc<Status>("app.status"),
  setup: (data_dir: string, device_name: string) =>
    rpc<Status>("app.setup", { data_dir, device_name }),

  nav: (today_since: number) => rpc<Nav>("nav.get", { today_since }),

  discover: (url: string) => rpc<Candidate[]>("feeds.discover", { url }),
  addFeed: (url: string, folder: string) => rpc<Feed>("feeds.add", { url, folder }),
  updateFeed: (
    id: string,
    patch: { title?: string; folder?: string; interval_min?: number | null },
  ) => rpc<void>("feeds.update", { id, ...patch }),
  removeFeed: (id: string) => rpc<void>("feeds.remove", { id }),
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

  presets: () => rpc<{ presets: Preset[] }>("mail.presets"),
  guessAccount: (email: string) => rpc<AccountGuess>("accounts.guess", { email }),
  addAccount: (account: NewAccount) => rpc<Account>("accounts.add", account),
  updateAccount: (id: string, changes: AccountChanges) =>
    rpc<Account>("accounts.update", { id, ...changes }),
  removeAccount: (id: string) => rpc<void>("accounts.remove", { id }),
  syncAccount: (id?: string) => rpc<void>("accounts.sync", { id }),
  attachment: (id: string, index: number, path?: string) =>
    rpc<{ name: string; mime?: string; data?: string; path?: string }>("mail.attachment", {
      id,
      index,
      path,
    }),
  trustSender: (address: string) => rpc<void>("mail.trust_sender", { address }),

  settings: () => rpc<Settings>("settings.get"),
  setSetting: (key: string, value: unknown) => rpc<void>("settings.set", { key, value }),

  devices: () => rpc<Device[]>("devices.list"),
  renameDevice: (name: string) => rpc<void>("devices.rename", { name }),

  rebuildIndex: () => rpc<ImportStats>("index.rebuild"),
};
