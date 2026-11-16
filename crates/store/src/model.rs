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
}

#[derive(Debug, Clone, Serialize)]
pub struct Feed {
    pub id: String,
    pub url: String,
    pub title: String,
    pub site_url: Option<String>,
    pub icon_url: Option<String>,
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
    /// `Some(None)` 表示恢复默认刷新间隔。
    #[serde(default, deserialize_with = "double_option")]
    pub interval_min: Option<Option<i64>>,
}

fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

#[derive(Debug, Clone, Default, Deserialize)]
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
    Today,
    Unread,
    Starred,
    Archived,
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
    /// 「今天」视图的起点，由界面按本地时区算出。
    pub since: Option<i64>,
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
    /// 邮件发件人的地址，界面用来生成头像颜色。
    pub sender_address: Option<String>,
    pub has_attachments: bool,
    /// 卡片视图用的封面图。只有文章有。
    pub image: Option<String>,
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
    /// 同样两个数，只算邮件或只算文章。侧栏的选项卡用。
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
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AccountPatch {
    pub name: Option<String>,
    pub imap_host: Option<String>,
    pub imap_port: Option<u16>,
    pub imap_security: Option<String>,
    pub username: Option<String>,
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
    pub added_at: i64,
    pub unread: i64,
    pub last_sync_at: Option<i64>,
    pub error: Option<String>,
    pub error_code: Option<String>,
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
}

/// 一次收件箱同步的结果，一次性写入。
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MailSyncStats {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

impl MailSyncStats {
    pub fn changed(&self) -> bool {
        self.added + self.updated + self.removed > 0
    }
}
