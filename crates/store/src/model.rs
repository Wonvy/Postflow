use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::hlc::Timestamp;

/// 操作日志里的一行：把若干实体的若干字段设为新值。
///
/// 格式刻意做成通用的「种类 + ID + 字段表」，旧版本读到不认识的种类或字段时
/// 照样记下来，不会丢数据，也不会报错。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Op {
    pub ts: Timestamp,
    /// 实体种类：feed、item、setting、device。
    pub k: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 一次改多个实体时用，例如「全部标为已读」。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<String>,
    pub set: Map<String, Value>,
}

impl Op {
    pub fn targets(&self) -> impl Iterator<Item = &String> {
        self.id.iter().chain(self.ids.iter())
    }
}

/// 文章存档里的一行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    pub id: String,
    pub feed: String,
    pub guid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// 发布时间，Unix 毫秒。
    pub published: i64,
    /// 抓取时间，Unix 毫秒。
    pub fetched: i64,
    /// 列表里显示的纯文本摘要。
    #[serde(default)]
    pub summary: String,
    /// 原始 HTML 正文，未经净化，展示前再净化。
    #[serde(default)]
    pub content: String,
    /// 源里明确给出的封面图（媒体缩略图、图片附件）。没有时从正文里找。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// 播客、视频的音频或视频附件。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Media>,
}

/// 条目附带的音频或视频。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Media {
    pub url: String,
    /// MIME 类型，比如 `audio/mpeg`。
    pub mime: String,
    /// 时长，秒。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    /// 文件大小，字节。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// 章节文件（Podcasting 2.0 的 JSON）地址。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapters: Option<String>,
    /// 文字稿。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<Transcript>,
}

/// 单集的文字稿：地址和格式（text/vtt、application/x-subrip、application/json、text/html、text/plain）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transcript {
    pub url: String,
    pub mime: String,
}

impl Media {
    /// 以前存下的附件少了这次源里有的章节或文字稿，要补上。
    /// 地址本身变了不算：有的源每次给的下载地址都带不同的签名，不能每抓一次就记一次。
    pub fn adds_to(&self, stored: Option<&Media>) -> bool {
        match stored {
            None => true,
            Some(old) => {
                (old.chapters.is_none() && self.chapters.is_some())
                    || (old.transcript.is_none() && self.transcript.is_some())
            }
        }
    }

    /// 存进索引、用来判断订阅源类型的 `audio` 或 `video`。
    pub fn kind(&self) -> &'static str {
        if self.mime.starts_with("video/") {
            "video"
        } else {
            "audio"
        }
    }
}

/// 放到订阅里看的一个订阅邮件发件人。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Newsletter {
    /// 发件人地址，小写。
    pub address: String,
    /// 最近一封信里发件人的名字，没有时为空。
    pub name: String,
    pub unread: i64,
    pub total: i64,
}

/// 订阅源的类型：文章、视频、播客。界面按它分成几类。
pub const FEED_KINDS: [&str; 3] = ["article", "video", "podcast"];

#[derive(Debug, Clone, Serialize)]
pub struct Feed {
    pub id: String,
    pub url: String,
    pub title: String,
    pub site_url: Option<String>,
    pub icon_url: Option<String>,
    /// 类型（`FEED_KINDS` 之一）：用户在「编辑订阅」里选的，没选时按内容自动判断。
    pub kind: String,
    /// 用户选的类型；`None` 表示自动。
    pub kind_set: Option<String>,
    /// 打开这个源的文章时自动取全文（源里只有摘要的网站用）。
    pub full_text: bool,
    pub folder: String,
    pub interval_min: Option<i64>,
    pub added_at: i64,
    pub unread: i64,
    pub last_fetch_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NewFeed {
    pub url: String,
    pub title: String,
    pub site_url: Option<String>,
    pub icon_url: Option<String>,
    pub folder: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FeedPatch {
    pub title: Option<String>,
    pub folder: Option<String>,
    /// 换订阅地址。订阅源的 ID 不变，已读、星标和以前的文章都还在。调用方先确认新地址能用。
    pub url: Option<String>,
    /// 换了地址后，新源给的网站地址和图标。由核心填，界面不能直接改。
    #[serde(skip)]
    pub site_url: Option<String>,
    #[serde(skip)]
    pub icon_url: Option<String>,
    /// `Some(None)` 表示恢复默认刷新间隔。
    #[serde(default, deserialize_with = "double_option")]
    pub interval_min: Option<Option<i64>>,
    /// 类型（`FEED_KINDS` 之一）。`Some(None)` 表示恢复自动判断。
    #[serde(default, deserialize_with = "double_option")]
    pub kind: Option<Option<String>>,
    /// 打开文章时自动取全文。
    pub full_text: Option<bool>,
}

fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct ItemPatch {
    pub read: Option<bool>,
    pub starred: Option<bool>,
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    #[default]
    Inbox,
    /// 收件箱和归档里的全部，搜索「全部」时用。不含垃圾邮件和已删除。
    All,
    Today,
    Unread,
    Starred,
    Archived,
    /// 邮箱里的垃圾邮件。
    Junk,
    /// 邮箱里已删除的邮件。
    Trash,
    /// 邮箱里的草稿。
    Drafts,
    /// 邮箱里已发送的邮件。
    Sent,
    /// 邮箱里的某个其他文件夹：和 `source_id`、`mailbox` 一起用。
    Folder,
    /// 稍后提醒着、还没到时间的。
    Snoozed,
    /// 提醒时间刚到（三天内）、还没读的：收件箱顶部单独列出来。
    Reminders,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Article,
    Mail,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Article => "article",
            ItemKind::Mail => "mail",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ItemFilter {
    #[serde(default)]
    pub view: View,
    /// 只看某个订阅源或某个邮箱账户。
    pub source_id: Option<String>,
    /// 只看文章或只看邮件。
    pub kind: Option<ItemKind>,
    pub folder: Option<String>,
    /// 邮箱里的哪个文件夹（服务器上的名字），和 `View::Folder` 一起用。
    pub mailbox: Option<String>,
    /// 「今天」视图的起点，由界面按本地时区算出。
    pub since: Option<i64>,
    /// 只看某个分类。
    pub label: Option<String>,
    /// 全文搜索：标题、发件人、作者和正文里都要有这些词。
    pub query: Option<String>,
    /// 只看某一类订阅源（`FEED_KINDS` 之一）的文章；`newsletter` 是放到订阅里看的订阅邮件。
    pub feed_kind: Option<String>,
    /// 只看这个发件人的邮件（地址，不分大小写）。订阅里的 Newsletter 用。
    pub sender: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Cursor {
    pub published_at: i64,
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemSummary {
    pub id: String,
    /// article 或 mail。
    pub kind: String,
    /// 订阅源 ID 或邮箱账户 ID。
    pub source_id: String,
    pub source_title: String,
    pub source_icon: Option<String>,
    pub url: Option<String>,
    pub title: String,
    pub author: Option<String>,
    pub summary: String,
    pub published_at: i64,
    pub read: bool,
    pub starred: bool,
    pub archived: bool,
    /// 邮箱「草稿」里的一封草稿：打开时接着写，不能归档。
    pub draft: bool,
    /// 「已发送」里自己发出去的信：不能归档。
    pub sent: bool,
    /// 邮件发件人的地址，界面用来生成头像颜色。
    pub sender_address: Option<String>,
    pub has_attachments: bool,
    /// 卡片视图用的封面图。只有文章有。
    pub image: Option<String>,
    /// 分类。
    pub label: Option<String>,
    /// 稍后提醒的时间（毫秒）。没设过、取消了是 None；已经过了的也还在，界面据此判断。
    pub snoozed_until: Option<i64>,
    /// 搜索时，正文里命中搜索词的那一小段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// 播客、视频的音频或视频附件。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<Media>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemDetail {
    #[serde(flatten)]
    pub item: ItemSummary,
    /// 原始 HTML，调用方负责净化。
    pub content: String,
    pub full_text: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    /// 收件箱里的未读数。
    pub unread: i64,
    /// 今天发布的未读数。
    pub today: i64,
    pub starred: i64,
    /// 同样两个数，只算邮件或只算文章：侧栏邮件和订阅两边各显示各的。
    pub unread_mail: i64,
    pub today_mail: i64,
    pub unread_article: i64,
    pub today_article: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub current: bool,
}

/// 抓取调度需要的信息，只存在本机索引里，不同步。
#[derive(Debug, Clone)]
pub struct FetchTarget {
    pub feed_id: String,
    pub url: String,
    pub interval_min: Option<i64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub error_count: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportStats {
    pub files: usize,
    pub ops: usize,
    pub articles: usize,
    /// 解析失败被跳过的行数。
    pub skipped: usize,
    /// 读取失败的文件数。
    pub unreadable: usize,
}

impl ImportStats {
    pub fn changed(&self) -> bool {
        self.ops > 0 || self.articles > 0
    }
}

// -------------------------------------------------------------------- 邮件

/// 一个邮箱账户的设置。会同步到其他设备，所以绝不能包含密码。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AccountConfig {
    pub email: String,
    #[serde(default)]
    pub name: String,
    pub imap_host: String,
    pub imap_port: u16,
    /// tls、starttls 或 none。
    pub imap_security: String,
    pub username: String,
    /// 发信服务器。为空时按服务商预设或收信服务器推断。
    #[serde(default)]
    pub smtp_host: Option<String>,
    #[serde(default)]
    pub smtp_port: Option<u16>,
    #[serde(default)]
    pub smtp_security: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AccountPatch {
    pub name: Option<String>,
    pub imap_host: Option<String>,
    pub imap_port: Option<u16>,
    pub imap_security: Option<String>,
    pub username: Option<String>,
    /// 空字符串表示改回自动。
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_security: Option<String>,
    /// 发信时收件人看到的名字。
    pub sender_name: Option<String>,
    /// 签名，写信时自动加在正文后面。
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Account {
    pub id: String,
    pub email: String,
    pub name: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: String,
    pub username: String,
    /// 自己填的发信服务器；为空表示自动。
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_security: Option<String>,
    /// 发信时收件人看到的名字。`name` 只是这个邮箱在侧栏里的名字（比如「工作」），不拿来发信。
    pub sender_name: String,
    /// 签名，写信时自动加在正文后面。会同步到其他设备。
    pub signature: String,
    pub added_at: i64,
    pub unread: i64,
    pub last_sync_at: Option<i64>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

/// 写信时给收件人一栏提示的联系人：给你来过信的人。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Contact {
    pub name: Option<String>,
    pub address: String,
    /// 收到过多少封他的信，越多越靠前。
    pub count: i64,
}

/// 一封邮件在服务器上的位置，写回服务器时用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailRef {
    pub id: String,
    pub account_id: String,
    pub mailbox: String,
    /// 移到归档之后不知道新的 UID，这时为空，要按 Message-ID 去找。
    pub uid: Option<u32>,
    pub message_id: Option<String>,
    pub size: u32,
}

/// 本机已有的一封收件箱邮件，同步时和服务器对比用。
#[derive(Debug, Clone)]
pub struct KnownMail {
    pub id: String,
    pub uid: u32,
    pub read: bool,
    pub starred: bool,
    pub archived: bool,
    pub touched_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct NewMail {
    pub id: String,
    pub uid: u32,
    pub message_id: Option<String>,
    pub subject: String,
    pub from_name: Option<String>,
    pub from_address: Option<String>,
    pub snippet: String,
    pub date: i64,
    pub read: bool,
    pub starred: bool,
    pub size: u32,
    pub has_attachments: bool,
    pub list_unsubscribe: Option<String>,
    /// 完整原文。大邮件同步时只取了信头，这时为空，打开时再取。
    pub raw: Option<Vec<u8>>,
    /// 已发送的信写给了谁（名字、地址），这些人也算联系人。
    pub recipients: Vec<(Option<String>, String)>,
}

/// 同步的是邮箱里的哪个文件夹。
///
/// 邮件在哪个文件夹记在 `items.archived` 列里：0 收件箱，1 归档，2 垃圾邮件，3 已删除，
/// 4 草稿，5 已发送，6 其他文件夹（具体是哪个看 `mail_meta.mailbox`）。
/// 这样所有按 `archived = 0` 筛收件箱的地方（列表、未读数、今天）自然不含后面这些。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailFolder {
    Inbox,
    Junk,
    Trash,
    Drafts,
    Sent,
    /// 用户自己建的文件夹，打开过才同步。
    Other,
}

impl MailFolder {
    /// 存在 `items.archived` 里的值。
    pub fn code(self) -> i64 {
        match self {
            MailFolder::Inbox => 0,
            MailFolder::Junk => 2,
            MailFolder::Trash => 3,
            MailFolder::Drafts => 4,
            MailFolder::Sent => 5,
            MailFolder::Other => 6,
        }
    }
}

/// 一次同步一个文件夹的结果，一次性写入。
#[derive(Debug, Clone, Default)]
pub struct MailSync {
    pub uidvalidity: u32,
    /// 这次同步开始的时间。在这之后本机改过的邮件，不用服务器上的旧状态覆盖。
    pub started_at: i64,
    pub new: Vec<NewMail>,
    /// UID、已读、星标。
    pub flags: Vec<(u32, bool, bool)>,
    /// 服务器上已经没有了的 UID。
    pub vanished: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MailSyncStats {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    /// 这次第一次出现在收件箱里的未读邮件（不算从别处移回来的），新邮件通知用。
    pub fresh: Vec<FreshMail>,
}

/// 新邮件通知里显示的几样。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FreshMail {
    pub id: String,
    pub from: String,
    pub subject: String,
    pub date: i64,
}

impl MailSyncStats {
    pub fn changed(&self) -> bool {
        self.added + self.updated + self.removed > 0
    }
}
