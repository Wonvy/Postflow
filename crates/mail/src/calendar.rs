//! 会议邀请：邮件里的 iCalendar（text/calendar 部分或 .ics 附件），以及接受、暂定、拒绝时
//! 发回给组织者的回复（iTIP REPLY，RFC 5546）。
//!
//! 只认邀请需要的那几项：主题、时间、地点、组织者、参会人、会议链接。时区按邀请里自带的
//! VTIMEZONE 换算（标准时间和夏令时各一个偏移，按月份判断用哪个，切换那几天可能差一小时）；
//! 没有 VTIMEZONE 的按 UTC 或者「当地时间」处理。

use mail_parser::{MessageParser, MimeHeaders, PartType};
use serde::Serialize;

use crate::message::Mailbox;

/// 参会人。`status` 是 iCalendar 的 PARTSTAT：NEEDS-ACTION、ACCEPTED、TENTATIVE、DECLINED。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Attendee {
    pub name: Option<String>,
    pub address: String,
    pub status: String,
}

/// 邮件里的一个会议邀请（或者取消、别人的回复）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Invite {
    /// REQUEST（邀请、更新）、CANCEL（取消）、REPLY（别人的回复）……
    pub method: String,
    pub uid: String,
    pub sequence: i64,
    pub summary: String,
    pub location: Option<String>,
    /// 开始、结束（UTC 毫秒）。算不出来时为空，看 `start_local`。
    pub start: Option<i64>,
    pub end: Option<i64>,
    /// 没有时区信息的「当地时间」，形如 `2026-11-20T10:00`。
    pub start_local: Option<String>,
    pub all_day: bool,
    pub organizer: Option<Mailbox>,
    pub attendees: Vec<Attendee>,
    /// 线上会议的链接（Teams、Zoom、Google Meet……）。
    pub url: Option<String>,
    /// 周期会议（每周、每月……）。
    pub recurring: bool,
    /// 这是周期会议里单独改过的某一次（RECURRENCE-ID）。
    pub occurrence: bool,
    /// 原始的 iCalendar 文字：「添加到日历」时原样交给日历应用。
    #[serde(skip)]
    pub ics: String,
}

/// 邮件里的会议邀请。没有时返回 `None`。
pub fn invite(raw: &[u8]) -> Option<Invite> {
    let msg = MessageParser::default().parse(raw)?;
    for part in &msg.parts {
        let calendar = part.content_type().is_some_and(|ct| {
            let main = ct.ctype().eq_ignore_ascii_case("text")
                || ct.ctype().eq_ignore_ascii_case("application");
            let sub = ct.subtype().unwrap_or_default().to_ascii_lowercase();
            main && (sub == "calendar" || sub == "ics")
        }) || part
            .attachment_name()
            .is_some_and(|n| n.to_ascii_lowercase().ends_with(".ics"));
        if !calendar {
            continue;
        }
        let text = match &part.body {
            PartType::Text(t) => t.to_string(),
            PartType::Binary(b) | PartType::InlineBinary(b) => {
                String::from_utf8_lossy(b).into_owned()
            }
            _ => continue,
        };
        if let Some(found) = parse_ics(&text) {
            return Some(found);
        }
    }
    None
}

/// 一行内容：名字、参数、值。
struct Line {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Line {
    fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
}

/// 拆开折行（以空格或制表符开头的行接在上一行后面），切成一行一行。
fn lines(text: &str) -> Vec<Line> {
    let mut joined: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = raw.strip_prefix([' ', '\t'])
            && let Some(last) = joined.last_mut()
        {
            last.push_str(rest);
        } else if !raw.is_empty() {
            joined.push(raw.to_string());
        }
    }
    joined.iter().filter_map(|l| split_line(l)).collect()
}

fn split_line(line: &str) -> Option<Line> {
    // 名字和参数在第一个不在引号里的冒号之前
    let mut quoted = false;
    let colon = line.char_indices().find_map(|(i, c)| match c {
        '"' => {
            quoted = !quoted;
            None
        }
        ':' if !quoted => Some(i),
        _ => None,
    })?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in head.chars() {
        match c {
            '"' => quoted = !quoted,
            ';' if !quoted => parts.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    parts.push(cur);
    let mut parts = parts.into_iter();
    let name = parts.next()?.to_ascii_uppercase();
    let params = parts
        .filter_map(|p| {
            p.split_once('=')
                .map(|(k, v)| (k.to_ascii_uppercase(), v.to_string()))
        })
        .collect();
    Some(Line {
        name,
        params,
        value: value.to_string(),
    })
}

/// 文字值里的转义：`\n`、`\,`、`\;`、`\\`。
fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn mailto(v: &str) -> String {
    let v = v.trim();
    v.strip_prefix("mailto:")
        .or_else(|| v.strip_prefix("MAILTO:"))
        .unwrap_or(v)
        .to_string()
}

/// 时区里的一段（标准时间或夏令时）：从什么时候开始、偏移多少。
#[derive(Default, Clone, Copy, Debug, PartialEq)]
struct Rule {
    /// 这段时间里比 UTC 快多少秒。
    offset: i64,
    /// 每年从哪个月开始。
    month: u32,
    /// 那个月的第几个星期几（`(2, 0)` 是第二个星期日，`(-1, 0)` 是最后一个星期日）。没有时用 `day`。
    by_day: Option<(i32, u32)>,
    day: u32,
    /// 在那一天的几点切换（当地时间，秒）。
    at: i64,
}

impl Rule {
    /// 这一年里切换的那一刻（当地时间，从 1970 年起的秒数）。
    fn start(&self, year: i64) -> i64 {
        let m = i64::from(self.month);
        let first = days_from_civil(year, m, 1);
        let next = if m == 12 {
            days_from_civil(year + 1, 1, 1)
        } else {
            days_from_civil(year, m + 1, 1)
        };
        let weekday = |days: i64| (days + 4).rem_euclid(7); // 1970-01-01 是星期四，星期日记作 0
        let day = match self.by_day {
            Some((n, wd)) if n > 0 => {
                first + (i64::from(wd) - weekday(first)).rem_euclid(7) + i64::from(n - 1) * 7
            }
            Some((n, wd)) => {
                let last = next - 1;
                last - (weekday(last) - i64::from(wd)).rem_euclid(7) - i64::from(-n - 1) * 7
            }
            None => first + i64::from(self.day.max(1)) - 1,
        };
        day * 86_400 + self.at
    }
}

/// 一个时区：标准时间和夏令时各一段规则。
#[derive(Default, Clone)]
struct Zone {
    standard: Option<Rule>,
    daylight: Option<Rule>,
}

impl Zone {
    /// 当地时间 `local`（从 1970 年起的秒数，年份 `year`）用哪个偏移。按邀请里写的规则
    /// （比如三月的第二个星期日两点）算出这一年夏令时的起止，切换当天也对。
    fn offset(&self, year: i64, local: i64) -> i64 {
        match (self.standard, self.daylight) {
            (Some(std), Some(dst)) => {
                let (from, to) = (dst.start(year), std.start(year));
                let in_dst = if from < to {
                    local >= from && local < to
                } else {
                    local >= from || local < to
                };
                if in_dst { dst.offset } else { std.offset }
            }
            (Some(r), None) | (None, Some(r)) => r.offset,
            (None, None) => 0,
        }
    }
}

/// `BYDAY=2SU`、`-1SU` 这样的写法。
fn by_day(v: &str) -> Option<(i32, u32)> {
    let v = v.split(',').next()?.trim();
    // 星期是最后两个字母；不是 ASCII 的（坏掉的邀请）直接不认，免得切在一个字的中间
    let cut = v.len().checked_sub(2)?;
    if !v.is_char_boundary(cut) {
        return None;
    }
    let (num, name) = v.split_at(cut);
    let wd = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"]
        .iter()
        .position(|d| name.eq_ignore_ascii_case(d))? as u32;
    let n = if num.is_empty() {
        1
    } else {
        num.trim_start_matches('+').parse().ok()?
    };
    Some((n, wd))
}

/// `+0800`、`-0530` 变成秒。
fn utc_offset(v: &str) -> Option<i64> {
    let v = v.trim();
    let (sign, rest) = match v.as_bytes().first()? {
        b'+' => (1, &v[1..]),
        b'-' => (-1, &v[1..]),
        _ => (1, v),
    };
    let h: i64 = rest.get(0..2)?.parse().ok()?;
    let m: i64 = rest.get(2..4).unwrap_or("00").parse().ok()?;
    Some(sign * (h * 3600 + m * 60))
}

/// 公历日期到 1970-01-01 的天数（Howard Hinnant 的算法）。
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// 拆开 `20261120T100000Z` 或 `20261120`。
fn date_parts(v: &str) -> Option<(i64, i64, i64, i64, i64, i64, bool)> {
    let v = v.trim();
    let num = |r: std::ops::Range<usize>| v.get(r).and_then(|s| s.parse::<i64>().ok());
    let (y, mo, d) = (num(0..4)?, num(4..6)?, num(6..8)?);
    if v.len() == 8 {
        return Some((y, mo, d, 0, 0, 0, false));
    }
    if v.as_bytes().get(8) != Some(&b'T') {
        return None;
    }
    let (h, mi, s) = (num(9..11)?, num(11..13)?, num(13..15).unwrap_or(0));
    Some((y, mo, d, h, mi, s, v.ends_with('Z') || v.ends_with('z')))
}

/// 一个时间：UTC 毫秒（算得出来时）和不带时区的当地时间。
fn when(
    line: &Line,
    zones: &std::collections::HashMap<String, Zone>,
) -> (Option<i64>, Option<String>, bool) {
    let all_day = line
        .param("VALUE")
        .is_some_and(|v| v.eq_ignore_ascii_case("DATE"))
        || line.value.trim().len() == 8;
    let Some((y, mo, d, h, mi, s, utc)) = date_parts(&line.value) else {
        return (None, None, all_day);
    };
    let local_secs = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s;
    let floating = format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}");
    if all_day {
        return (None, Some(format!("{y:04}-{mo:02}-{d:02}")), true);
    }
    if utc {
        return (Some(local_secs * 1000), None, false);
    }
    match line.param("TZID").map(|t| t.trim_matches('"')) {
        Some(tz) if zones.contains_key(tz) => {
            let offset = zones[tz].offset(y, local_secs);
            (Some((local_secs - offset) * 1000), None, false)
        }
        Some(tz) if tz.eq_ignore_ascii_case("UTC") || tz.eq_ignore_ascii_case("Etc/UTC") => {
            (Some(local_secs * 1000), None, false)
        }
        _ => (None, Some(floating), false),
    }
}

/// 读一段 iCalendar，取出第一个会议。
pub fn parse_ics(text: &str) -> Option<Invite> {
    let all = lines(text);
    if !all
        .iter()
        .any(|l| l.name == "BEGIN" && l.value.eq_ignore_ascii_case("VEVENT"))
    {
        return None;
    }
    // 先把时区读出来
    let mut zones = std::collections::HashMap::new();
    let mut zone_id = None::<String>;
    let mut zone = Zone::default();
    let mut part = None::<String>;
    let mut offset = None::<i64>;
    let mut rule = Rule::default();
    for l in &all {
        match (l.name.as_str(), l.value.to_ascii_uppercase().as_str()) {
            ("BEGIN", "VTIMEZONE") => {
                zone = Zone::default();
                zone_id = None;
            }
            ("TZID", _) if part.is_none() => zone_id = Some(l.value.trim().to_string()),
            ("BEGIN", p @ ("STANDARD" | "DAYLIGHT")) => {
                part = Some(p.to_string());
                offset = None;
                rule = Rule::default();
            }
            ("TZOFFSETTO", _) => offset = utc_offset(&l.value),
            ("DTSTART", _) if part.is_some() => {
                if let Some((_, mo, d, h, mi, s, _)) = date_parts(&l.value) {
                    if rule.month == 0 {
                        rule.month = mo as u32;
                    }
                    rule.day = d as u32;
                    rule.at = h * 3600 + mi * 60 + s;
                }
            }
            ("RRULE", _) if part.is_some() => {
                for kv in l.value.split(';') {
                    if let Some(m) = kv
                        .strip_prefix("BYMONTH=")
                        .and_then(|m| m.split(',').next()?.parse().ok())
                    {
                        rule.month = m;
                    } else if let Some(d) = kv.strip_prefix("BYDAY=") {
                        rule.by_day = by_day(d);
                    }
                }
            }
            ("END", p @ ("STANDARD" | "DAYLIGHT")) => {
                if let Some(o) = offset {
                    let entry = Some(Rule {
                        offset: o,
                        month: rule.month.max(1),
                        ..rule
                    });
                    if p == "STANDARD" {
                        zone.standard = entry;
                    } else {
                        zone.daylight = entry;
                    }
                }
                part = None;
            }
            ("END", "VTIMEZONE") => {
                if let Some(id) = zone_id.take() {
                    zones.insert(id, zone.clone());
                }
            }
            _ => {}
        }
    }

    let method = all
        .iter()
        .take_while(|l| !(l.name == "BEGIN" && l.value.eq_ignore_ascii_case("VEVENT")))
        .find(|l| l.name == "METHOD")
        .map(|l| l.value.trim().to_ascii_uppercase())
        .unwrap_or_else(|| "PUBLISH".into());
    let mut inv = Invite {
        method,
        uid: String::new(),
        sequence: 0,
        summary: String::new(),
        location: None,
        start: None,
        end: None,
        start_local: None,
        all_day: false,
        organizer: None,
        attendees: Vec::new(),
        url: None,
        recurring: false,
        occurrence: false,
        ics: text.to_string(),
    };
    let mut in_event = false;
    let mut depth = 0;
    for l in &all {
        let upper = l.value.trim().to_ascii_uppercase();
        if l.name == "BEGIN" && upper == "VEVENT" && !in_event {
            in_event = true;
            continue;
        }
        if !in_event {
            continue;
        }
        // VEVENT 里的 VALARM 之类不管
        if l.name == "BEGIN" {
            depth += 1;
            continue;
        }
        if l.name == "END" {
            if depth == 0 && upper == "VEVENT" {
                break;
            }
            depth -= 1;
            continue;
        }
        if depth > 0 {
            continue;
        }
        match l.name.as_str() {
            "UID" => inv.uid = l.value.trim().to_string(),
            "SEQUENCE" => inv.sequence = l.value.trim().parse().unwrap_or(0),
            "SUMMARY" => inv.summary = unescape(&l.value).trim().to_string(),
            "LOCATION" => {
                inv.location = Some(unescape(&l.value).trim().to_string()).filter(|s| !s.is_empty())
            }
            "DTSTART" => {
                let (utc, local, all_day) = when(l, &zones);
                inv.start = utc;
                inv.start_local = local;
                inv.all_day = all_day;
            }
            "DTEND" => inv.end = when(l, &zones).0,
            "RRULE" => inv.recurring = true,
            "RECURRENCE-ID" => inv.occurrence = true,
            "ORGANIZER" => {
                inv.organizer = Some(Mailbox {
                    name: l.param("CN").map(|n| n.trim_matches('"').to_string()),
                    address: Some(mailto(&l.value)),
                });
            }
            "ATTENDEE" => inv.attendees.push(Attendee {
                name: l.param("CN").map(|n| n.trim_matches('"').to_string()),
                address: mailto(&l.value),
                status: l
                    .param("PARTSTAT")
                    .unwrap_or("NEEDS-ACTION")
                    .to_ascii_uppercase(),
            }),
            "URL" | "X-MICROSOFT-SKYPETEAMSMEETINGURL" | "X-GOOGLE-CONFERENCE" => {
                let v = l.value.trim();
                if inv.url.is_none() && (v.starts_with("https://") || v.starts_with("http://")) {
                    inv.url = Some(v.to_string());
                }
            }
            "DESCRIPTION" if inv.url.is_none() => inv.url = meeting_link(&unescape(&l.value)),
            _ => {}
        }
    }
    (!inv.uid.is_empty() || !inv.summary.is_empty()).then_some(inv)
}

/// 说明里的线上会议链接。
fn meeting_link(text: &str) -> Option<String> {
    const HOSTS: &[&str] = &[
        "teams.microsoft.com",
        "zoom.us",
        "meet.google.com",
        "webex.com",
        "meeting.tencent.com",
        "feishu.cn",
        "dingtalk.com",
    ];
    text.split(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '"')
        .filter(|w| w.starts_with("https://"))
        .find(|w| HOSTS.iter().any(|h| w.contains(h)))
        .map(|w| w.trim_end_matches([')', '.', ',']).to_string())
}

/// 第一个 VEVENT 自己的那几行（不含里面的 VALARM，也不含外面的 VTIMEZONE）。
fn event_lines(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let (mut inside, mut depth) = (false, 0);
    for l in lines(text) {
        let upper = l.value.trim().to_ascii_uppercase();
        match (inside, l.name.as_str()) {
            (false, "BEGIN") if upper == "VEVENT" => inside = true,
            (false, _) => {}
            (true, "BEGIN") => depth += 1,
            (true, "END") if depth == 0 => break,
            (true, "END") => depth -= 1,
            (true, _) if depth == 0 => out.push(l),
            (true, _) => {}
        }
    }
    out
}

/// 回复的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Partstat {
    Accepted,
    Tentative,
    Declined,
}

impl Partstat {
    pub fn as_str(self) -> &'static str {
        match self {
            Partstat::Accepted => "ACCEPTED",
            Partstat::Tentative => "TENTATIVE",
            Partstat::Declined => "DECLINED",
        }
    }
}

/// 折行：每行不超过 75 个字节，接着的行以空格开头。
fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut count = 0;
    for c in line.chars() {
        let len = c.len_utf8();
        if count + len > 75 {
            out.push_str("\r\n ");
            count = 1;
        }
        out.push(c);
        count += len;
    }
    out.push_str("\r\n");
    out
}

fn escape(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

/// 回给组织者的 iCalendar（METHOD:REPLY）。`now` 是 UTC 秒，写进 DTSTAMP。
pub fn reply_ics(invite: &Invite, me: &Mailbox, status: Partstat, now: i64) -> String {
    let stamp = {
        let days = now.div_euclid(86_400);
        let secs = now.rem_euclid(86_400);
        // 天数换回日期
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        format!(
            "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
            secs / 3600,
            secs % 3600 / 60,
            secs % 60
        )
    };
    let source = event_lines(&invite.ics);
    // 开始、结束时间原样照抄（连同时区参数），对方据此认出是哪一次
    let copy = |name: &str| {
        source
            .iter()
            .find(|l| l.name == name)
            .map(|l| {
                let params: String = l.params.iter().map(|(k, v)| format!(";{k}={v}")).collect();
                fold(&format!("{name}{params}:{}", l.value))
            })
            .unwrap_or_default()
    };
    let address = me.address.clone().unwrap_or_default();
    let cn = me
        .name
        .as_deref()
        .filter(|n| !n.is_empty())
        .map(|n| format!(";CN=\"{}\"", n.replace('"', "")))
        .unwrap_or_default();
    let organizer = invite
        .organizer
        .as_ref()
        .map(|o| {
            let cn = o
                .name
                .as_deref()
                .map(|n| format!(";CN=\"{}\"", n.replace('"', "")))
                .unwrap_or_default();
            fold(&format!(
                "ORGANIZER{cn}:mailto:{}",
                o.address.clone().unwrap_or_default()
            ))
        })
        .unwrap_or_default();
    let mut out = String::new();
    out.push_str(
        "BEGIN:VCALENDAR\r\nPRODID:-//Postflow//Postflow//EN\r\nVERSION:2.0\r\nMETHOD:REPLY\r\n",
    );
    out.push_str("BEGIN:VEVENT\r\n");
    out.push_str(&fold(&format!("UID:{}", invite.uid)));
    out.push_str(&format!("SEQUENCE:{}\r\n", invite.sequence));
    out.push_str(&format!("DTSTAMP:{stamp}\r\n"));
    out.push_str(&copy("DTSTART"));
    out.push_str(&copy("DTEND"));
    out.push_str(&copy("RECURRENCE-ID"));
    out.push_str(&organizer);
    out.push_str(&fold(&format!(
        "ATTENDEE;PARTSTAT={}{cn}:mailto:{address}",
        status.as_str()
    )));
    out.push_str(&fold(&format!("SUMMARY:{}", escape(&invite.summary))));
    out.push_str("END:VEVENT\r\nEND:VCALENDAR\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTLOOK: &str = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nPRODID:Microsoft Exchange Server 2010\r\nVERSION:2.0\r\n\
BEGIN:VTIMEZONE\r\nTZID:China Standard Time\r\nBEGIN:STANDARD\r\nDTSTART:16010101T000000\r\nTZOFFSETFROM:+0800\r\nTZOFFSETTO:+0800\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
BEGIN:VTIMEZONE\r\nTZID:Pacific Standard Time\r\nBEGIN:STANDARD\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:-0700\r\nTZOFFSETTO:-0800\r\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=11\r\nEND:STANDARD\r\n\
BEGIN:DAYLIGHT\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:-0800\r\nTZOFFSETTO:-0700\r\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=2SU;BYMONTH=3\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\n\
BEGIN:VEVENT\r\nORGANIZER;CN=\"Lee, Ann\":mailto:ann@example.com\r\n\
ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;RSVP=TRUE;CN=Me:mailto:me@example.com\r\n\
ATTENDEE;PARTSTAT=ACCEPTED;CN=Bo:mailto:bo@example.com\r\n\
DESCRIPTION;LANGUAGE=en-US:Agenda\\n\\nJoin: https://teams.microsoft.com/l/meetup-join/abc \\n\r\n\
UID:040000008200E00074C5B7101A82E008\r\nSUMMARY;LANGUAGE=en-US:Q4 review\\, budget\r\n\
DTSTART;TZID=China Standard Time:20261120T100000\r\nDTEND;TZID=China Standard Time:20261120T110000\r\n\
SEQUENCE:2\r\nLOCATION:Room 3\r\nBEGIN:VALARM\r\nDESCRIPTION:REMINDER\r\nTRIGGER;RELATED=START:-PT15M\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn reads_an_outlook_invite() {
        let inv = parse_ics(OUTLOOK).unwrap();
        assert_eq!(inv.method, "REQUEST");
        assert_eq!(inv.summary, "Q4 review, budget");
        assert_eq!(inv.sequence, 2);
        assert_eq!(inv.location.as_deref(), Some("Room 3"));
        // 2026-11-20 10:00 北京时间 = 02:00 UTC
        assert_eq!(
            inv.start,
            Some((days_from_civil(2026, 11, 20) * 86_400 + 2 * 3600) * 1000)
        );
        assert_eq!(
            inv.end,
            Some((days_from_civil(2026, 11, 20) * 86_400 + 3 * 3600) * 1000)
        );
        assert!(!inv.all_day);
        let org = inv.organizer.as_ref().unwrap();
        assert_eq!(org.name.as_deref(), Some("Lee, Ann"));
        assert_eq!(org.address.as_deref(), Some("ann@example.com"));
        assert_eq!(inv.attendees.len(), 2);
        assert_eq!(inv.attendees[1].status, "ACCEPTED");
        assert_eq!(
            inv.url.as_deref(),
            Some("https://teams.microsoft.com/l/meetup-join/abc")
        );
    }

    #[test]
    fn daylight_time_by_month() {
        let ics = OUTLOOK.replace(
            "DTSTART;TZID=China Standard Time:20261120T100000",
            "DTSTART;TZID=Pacific Standard Time:20260701T090000",
        );
        let inv = parse_ics(&ics).unwrap();
        // 7 月是夏令时，UTC-7
        assert_eq!(
            inv.start,
            Some((days_from_civil(2026, 7, 1) * 86_400 + 16 * 3600) * 1000)
        );
    }

    #[test]
    fn daylight_time_switches_on_the_right_day() {
        // 2026 年美国夏令时从 3 月 8 日（第二个星期日）2 点开始，11 月 1 日（第一个星期日）结束
        let at = |date: &str| {
            let ics = OUTLOOK.replace(
                "DTSTART;TZID=China Standard Time:20261120T100000",
                &format!("DTSTART;TZID=Pacific Standard Time:{date}"),
            );
            parse_ics(&ics).unwrap().start.unwrap() / 1000
        };
        let utc = |y: i64, m: i64, d: i64, h: i64| days_from_civil(y, m, d) * 86_400 + h * 3600;
        assert_eq!(at("20260307T090000"), utc(2026, 3, 7, 17)); // 还是 UTC-8
        assert_eq!(at("20260308T090000"), utc(2026, 3, 8, 16)); // 已经是 UTC-7
        assert_eq!(at("20261031T090000"), utc(2026, 10, 31, 16));
        assert_eq!(at("20261101T090000"), utc(2026, 11, 1, 17));
        assert_eq!(by_day("-1SU"), Some((-1, 0)));
        assert_eq!(by_day("2SU"), Some((2, 0)));
        // 坏掉的邀请里不是 ASCII 的写法：不认，不崩
        assert_eq!(by_day("2日曜"), None);
        assert_eq!(by_day("日"), None);
        // 最后一个星期日：2026 年 10 月是 25 日
        let rule = Rule {
            offset: 0,
            month: 10,
            by_day: Some((-1, 0)),
            day: 1,
            at: 3600,
        };
        assert_eq!(
            rule.start(2026),
            days_from_civil(2026, 10, 25) * 86_400 + 3600
        );
    }

    #[test]
    fn recurring_meetings_are_marked() {
        let ics = OUTLOOK.replace(
            "SEQUENCE:2\r\n",
            "SEQUENCE:2\r\nRRULE:FREQ=WEEKLY;BYDAY=FR\r\n",
        );
        let inv = parse_ics(&ics).unwrap();
        assert!(inv.recurring && !inv.occurrence);
        let one = OUTLOOK.replace(
            "SEQUENCE:2\r\n",
            "SEQUENCE:2\r\nRECURRENCE-ID;TZID=China Standard Time:20261127T100000\r\n",
        );
        assert!(parse_ics(&one).unwrap().occurrence);
    }

    #[test]
    fn all_day_and_utc_and_floating() {
        let base =
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:x\nSUMMARY:Off\n{}\nEND:VEVENT\nEND:VCALENDAR\n";
        let day = parse_ics(&base.replace("{}", "DTSTART;VALUE=DATE:20261224")).unwrap();
        assert!(day.all_day);
        assert_eq!(day.start_local.as_deref(), Some("2026-12-24"));
        let utc = parse_ics(&base.replace("{}", "DTSTART:20261224T090000Z")).unwrap();
        assert_eq!(
            utc.start,
            Some((days_from_civil(2026, 12, 24) * 86_400 + 9 * 3600) * 1000)
        );
        let floating =
            parse_ics(&base.replace("{}", "DTSTART;TZID=Somewhere/Unknown:20261224T090000"))
                .unwrap();
        assert_eq!(floating.start, None);
        assert_eq!(floating.start_local.as_deref(), Some("2026-12-24T09:00"));
        assert_eq!(day.method, "PUBLISH");
    }

    #[test]
    fn reply_goes_back_to_the_organizer() {
        let inv = parse_ics(OUTLOOK).unwrap();
        let me = Mailbox {
            name: Some("Me".into()),
            address: Some("me@example.com".into()),
        };
        let ics = reply_ics(&inv, &me, Partstat::Accepted, 1_763_600_000);
        assert!(ics.contains("METHOD:REPLY\r\n"));
        assert!(ics.contains("UID:040000008200E00074C5B7101A82E008\r\n"));
        assert!(ics.contains("SEQUENCE:2\r\n"));
        assert!(ics.contains("DTSTART;TZID=China Standard Time:20261120T100000\r\n"));
        assert!(ics.contains("ATTENDEE;PARTSTAT=ACCEPTED;CN=\"Me\":mailto:me@example.com\r\n"));
        assert!(ics.contains("ORGANIZER;CN=\"Lee, Ann\":mailto:ann@example.com\r\n"));
        assert!(ics.contains("SUMMARY:Q4 review\\, budget\r\n"));
        assert!(ics.contains("DTSTAMP:20251120T005320Z\r\n"), "{ics}");
        // 读回来还是同一个会议
        let back = parse_ics(&ics).unwrap();
        assert_eq!(back.method, "REPLY");
        assert_eq!(back.attendees[0].status, "ACCEPTED");
    }
}
