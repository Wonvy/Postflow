use std::time::Duration;

use reqwest::header::{
    CONTENT_TYPE, COOKIE, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, LOCATION,
};

use crate::parse::{ParsedFeed, parse_source};
use crate::{Error, Result};

const USER_AGENT: &str = concat!("Postflow/", env!("CARGO_PKG_VERSION"));
/// 单个响应的大小上限，防止异常的源把内存吃光。
const MAX_BODY_MB: usize = 16;
/// 最多跟几次跳转。
const MAX_REDIRECTS: usize = 8;

#[derive(Clone)]
pub struct Client {
    pub(crate) http: reqwest::Client,
    /// 不自动跟跳转的客户端：抓订阅源时自己跟，好知道源是不是永久搬了家。
    manual: reqwest::Client,
}

/// 上次抓取时服务器给的校验值，用来做条件请求。
#[derive(Debug, Clone, Default)]
pub struct Conditional {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug)]
pub enum FetchResult {
    /// 服务器回了 304，内容没变。
    NotModified,
    Fetched(Fetched),
}

#[derive(Debug)]
pub struct Fetched {
    pub feed: ParsedFeed,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// 源永久搬了家（一路都是 301 / 308 跳转）时的新地址，以后应当直接用它。
    pub moved_to: Option<String>,
}

pub(crate) struct Page {
    pub final_url: url::Url,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub body: Vec<u8>,
}

impl Client {
    pub fn new() -> Result<Self> {
        let build = |redirect| {
            reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .redirect(redirect)
                .build()
                .map_err(|e| Error::Http(e.to_string()))
        };
        Ok(Client {
            http: build(reqwest::redirect::Policy::limited(MAX_REDIRECTS))?,
            manual: build(reqwest::redirect::Policy::none())?,
        })
    }

    /// 抓取并解析一个订阅源。带上校验值时，内容没变只花一个 304 的流量。
    /// 跳转由这里自己跟：一路都是永久跳转时，在 `moved_to` 里告诉调用方新地址。
    pub async fn fetch_feed(&self, url: &str, cond: &Conditional) -> Result<FetchResult> {
        let start = url::Url::parse(url).map_err(|_| Error::InvalidUrl(url.to_string()))?;
        let mut current = start.clone();
        let mut permanent = true;
        // 校验值属于原地址；永久搬家后新地址的内容要完整取一次，这样也才会知道搬了家
        let mut cond = Some(cond);
        for _ in 0..=MAX_REDIRECTS {
            let resp = conditional(self.manual.get(current.as_str()), cond)
                .send()
                .await
                .map_err(describe)?;
            let status = resp.status();
            if status.is_redirection() && status != reqwest::StatusCode::NOT_MODIFIED {
                let next = resp
                    .headers()
                    .get(LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|loc| current.join(loc.trim()).ok())
                    .filter(|u| matches!(u.scheme(), "http" | "https"))
                    .ok_or(Error::Status(status.as_u16()))?;
                permanent &= matches!(status.as_u16(), 301 | 308);
                if permanent {
                    cond = None;
                }
                current = next;
                continue;
            }
            let Some(page) = read_page(resp).await? else {
                return Ok(FetchResult::NotModified);
            };
            let feed = parse_source(&page.body, page.final_url.as_str())?;
            let moved_to = (permanent && current != start).then(|| current.to_string());
            return Ok(FetchResult::Fetched(Fetched {
                feed,
                etag: page.etag,
                last_modified: page.last_modified,
                moved_to,
            }));
        }
        Err(Error::Http("too many redirects".into()))
    }

    /// 发起 GET。返回 `None` 表示 304。
    pub(crate) async fn get(
        &self,
        url: &str,
        cond: Option<&Conditional>,
        timeout: Option<Duration>,
    ) -> Result<Option<Page>> {
        self.get_with_cookie(url, cond, timeout, None).await
    }

    /// 把一个大文件（播客单集）下载到 `to`，边下边写，不整个放进内存。先写到旁边的临时文件，下完再改名，
    /// 中途失败不会留下半个文件。`progress` 收到已下载的字节数和总大小（服务器说了的话）。
    /// 超过 `limit` 字节、或者一分钟收不到数据就放弃。
    pub async fn download(
        &self,
        url: &str,
        to: &std::path::Path,
        limit: u64,
        progress: impl Fn(u64, Option<u64>),
    ) -> Result<u64> {
        use std::io::Write;
        const STALL: Duration = Duration::from_secs(60);
        let mut resp = self
            .http
            .get(url)
            // 大文件慢慢下，不用客户端默认的 30 秒
            .timeout(Duration::from_secs(6 * 3600))
            .send()
            .await
            .map_err(describe)?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        let total = resp.content_length();
        if total.is_some_and(|t| t > limit) {
            return Err(Error::TooLarge((limit / (1024 * 1024)) as usize));
        }
        let partial = to.with_extension("part");
        let mut file = std::fs::File::create(&partial).map_err(|e| Error::Http(e.to_string()))?;
        let mut received = 0u64;
        let result = async {
            loop {
                let chunk = tokio::time::timeout(STALL, resp.chunk())
                    .await
                    .map_err(|_| Error::Http("download stalled".into()))?
                    .map_err(describe)?;
                let Some(chunk) = chunk else { break };
                received += chunk.len() as u64;
                if received > limit {
                    return Err(Error::TooLarge((limit / (1024 * 1024)) as usize));
                }
                file.write_all(&chunk)
                    .map_err(|e| Error::Http(e.to_string()))?;
                progress(received, total);
            }
            file.flush().map_err(|e| Error::Http(e.to_string()))?;
            Ok(received)
        }
        .await;
        drop(file);
        match result {
            Ok(n) if n > 0 => {
                std::fs::rename(&partial, to).map_err(|e| Error::Http(e.to_string()))?;
                Ok(n)
            }
            Ok(_) => {
                let _ = std::fs::remove_file(&partial);
                Err(Error::Http("empty file".into()))
            }
            Err(e) => {
                let _ = std::fs::remove_file(&partial);
                Err(e)
            }
        }
    }

    /// 同 `get`，可以带一个 Cookie 头。有的网站（YouTube）不带同意 Cookie 时先给一个确认页。
    pub(crate) async fn get_with_cookie(
        &self,
        url: &str,
        cond: Option<&Conditional>,
        timeout: Option<Duration>,
        cookie: Option<&str>,
    ) -> Result<Option<Page>> {
        let mut req = conditional(self.http.get(url), cond);
        if let Some(cookie) = cookie {
            req = req.header(COOKIE, cookie);
        }
        if let Some(timeout) = timeout {
            req = req.timeout(timeout);
        }
        let resp = req.send().await.map_err(describe)?;
        read_page(resp).await
    }
}

/// 带上条件请求的校验值。
fn conditional(
    mut req: reqwest::RequestBuilder,
    cond: Option<&Conditional>,
) -> reqwest::RequestBuilder {
    if let Some(cond) = cond {
        if let Some(etag) = &cond.etag {
            req = req.header(IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = &cond.last_modified {
            req = req.header(IF_MODIFIED_SINCE, last_modified);
        }
    }
    req
}

/// 读完响应：304 返回 `None`，别的不成功的状态码报错，正文有大小上限。
async fn read_page(mut resp: reqwest::Response) -> Result<Option<Page>> {
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(Error::Status(status.as_u16()));
    }

    let header = |name| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let content_type = header(CONTENT_TYPE);
    let etag = header(ETAG);
    let last_modified = header(LAST_MODIFIED);
    let final_url = resp.url().clone();

    let limit = MAX_BODY_MB * 1024 * 1024;
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(describe)? {
        if body.len() + chunk.len() > limit {
            return Err(Error::TooLarge(MAX_BODY_MB));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Some(Page {
        final_url,
        content_type,
        etag,
        last_modified,
        body,
    }))
}

/// 把 reqwest 的错误压成一句人能看懂的话，带上最底层的原因。
pub(crate) fn describe(err: reqwest::Error) -> Error {
    let kind = if err.is_timeout() {
        "timed out"
    } else if err.is_connect() {
        "could not connect"
    } else if err.is_redirect() {
        "too many redirects"
    } else {
        "request failed"
    };
    let mut source: &dyn std::error::Error = &err;
    while let Some(next) = source.source() {
        source = next;
    }
    let detail = source.to_string();
    if std::ptr::addr_eq(source, &err) || detail.is_empty() {
        Error::Http(kind.to_string())
    } else {
        Error::Http(format!("{kind}: {detail}"))
    }
}

/// 按响应头或 `<meta charset>` 声明的编码把网页解码成字符串。
pub(crate) fn decode_html(page: &Page) -> String {
    let from_header = page
        .content_type
        .as_deref()
        .and_then(|ct| {
            ct.to_ascii_lowercase()
                .split("charset=")
                .nth(1)
                .map(str::to_string)
        })
        .map(|c| {
            c.trim_matches(|ch| ch == '"' || ch == '\'' || ch == ' ')
                .to_string()
        });
    let label = from_header.or_else(|| sniff_meta_charset(&page.body));
    let encoding = label
        .and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    encoding.decode(&page.body).0.into_owned()
}

fn sniff_meta_charset(body: &[u8]) -> Option<String> {
    let head = String::from_utf8_lossy(&body[..body.len().min(4096)]).to_ascii_lowercase();
    let at = head.find("charset=")? + "charset=".len();
    let rest = head[at..].trim_start_matches(['"', '\'', ' ']);
    let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))?;
    (end > 0).then(|| rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(content_type: Option<&str>, body: &[u8]) -> Page {
        Page {
            final_url: url::Url::parse("https://example.com/").unwrap(),
            content_type: content_type.map(str::to_string),
            etag: None,
            last_modified: None,
            body: body.to_vec(),
        }
    }

    #[test]
    fn decodes_by_header_then_meta_then_utf8() {
        // 「中文」的 GBK 编码
        let gbk = [0xd6, 0xd0, 0xce, 0xc4];
        assert_eq!(
            decode_html(&page(Some("text/html; charset=GBK"), &gbk)),
            "中文"
        );

        let mut with_meta = b"<html><head><meta charset=\"gb2312\"></head><body>".to_vec();
        with_meta.extend_from_slice(&gbk);
        assert!(decode_html(&page(Some("text/html"), &with_meta)).ends_with("中文"));

        assert_eq!(decode_html(&page(None, "中文".as_bytes())), "中文");
    }
}
