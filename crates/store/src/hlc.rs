//! 混合逻辑时钟。
//!
//! 每条操作都带一个时间戳，合并时同一字段取时间戳最大的那次写入。
//! 时间戳由「毫秒墙钟、计数器、设备 ID」组成：墙钟保证大体按真实时间排序，
//! 计数器保证同一毫秒内的先后，设备 ID 用来打破平局。

use std::fmt;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// 观察到其他设备的时间戳时，最多允许把本地时钟往前推这么多。
/// 防止一台时钟错得离谱的设备把所有设备的时钟都带偏。
const MAX_DRIFT_MS: u64 = 24 * 60 * 60 * 1000;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Timestamp {
    pub ms: u64,
    pub counter: u32,
    pub device: String,
}

impl fmt::Display for Timestamp {
    /// 定宽编码，字符串的字典序与 (ms, counter, device) 的元组序一致，
    /// 所以可以直接在 SQLite 里比较字符串。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:013}-{:06x}-{}", self.ms, self.counter, self.device)
    }
}

impl FromStr for Timestamp {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.splitn(3, '-');
        let (Some(ms), Some(counter), Some(device)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(format!("invalid timestamp: {s}"));
        };
        if ms.len() != 13 || counter.len() != 6 || device.is_empty() {
            return Err(format!("invalid timestamp: {s}"));
        }
        Ok(Timestamp {
            ms: ms.parse().map_err(|_| format!("invalid timestamp: {s}"))?,
            counter: u32::from_str_radix(counter, 16)
                .map_err(|_| format!("invalid timestamp: {s}"))?,
            device: device.to_string(),
        })
    }
}

impl From<Timestamp> for String {
    fn from(ts: Timestamp) -> String {
        ts.to_string()
    }
}

impl TryFrom<String> for Timestamp {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

#[derive(Debug)]
pub struct Clock {
    device: String,
    last_ms: u64,
    counter: u32,
}

impl Clock {
    pub fn new(device: &str) -> Self {
        Clock {
            device: device.to_string(),
            last_ms: 0,
            counter: 0,
        }
    }

    /// 生成一个严格大于此前所有已发出、已观察到的时间戳的新时间戳。
    pub fn tick(&mut self) -> Timestamp {
        self.tick_at(now_ms())
    }

    pub(crate) fn tick_at(&mut self, wall: u64) -> Timestamp {
        if wall > self.last_ms {
            self.last_ms = wall;
            self.counter = 0;
        } else {
            self.counter += 1;
        }
        Timestamp {
            ms: self.last_ms,
            counter: self.counter,
            device: self.device.clone(),
        }
    }

    /// 读到其他设备的操作时调用，让本地时钟不落后于它。
    pub fn observe(&mut self, ts: &Timestamp) {
        self.observe_at(ts, now_ms());
    }

    pub(crate) fn observe_at(&mut self, ts: &Timestamp, wall: u64) {
        if ts.ms > wall.saturating_add(MAX_DRIFT_MS) {
            return;
        }
        if ts.ms > self.last_ms {
            self.last_ms = ts.ms;
            self.counter = ts.counter;
        } else if ts.ms == self.last_ms {
            self.counter = self.counter.max(ts.counter);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_order_matches_tuple_order() {
        let a = Timestamp {
            ms: 999,
            counter: 5,
            device: "aa".into(),
        };
        let b = Timestamp {
            ms: 1000,
            counter: 0,
            device: "aa".into(),
        };
        let c = Timestamp {
            ms: 1000,
            counter: 1,
            device: "aa".into(),
        };
        let d = Timestamp {
            ms: 1000,
            counter: 1,
            device: "bb".into(),
        };
        let all = [&a, &b, &c, &d];
        for w in all.windows(2) {
            assert!(w[0] < w[1]);
            assert!(w[0].to_string() < w[1].to_string());
        }
    }

    #[test]
    fn roundtrip() {
        let ts = Timestamp {
            ms: 1_794_000_000_123,
            counter: 0x2a,
            device: "ab12cd34ef56".into(),
        };
        let s = ts.to_string();
        assert_eq!(s, "1794000000123-00002a-ab12cd34ef56");
        assert_eq!(s.parse::<Timestamp>().unwrap(), ts);
        assert!("garbage".parse::<Timestamp>().is_err());
    }

    #[test]
    fn tick_is_monotonic_even_if_wall_clock_goes_back() {
        let mut clock = Clock::new("dev");
        let a = clock.tick_at(5_000);
        let b = clock.tick_at(4_000);
        let c = clock.tick_at(4_000);
        assert!(a < b && b < c);
    }

    #[test]
    fn observe_advances_but_ignores_far_future() {
        let mut clock = Clock::new("dev");
        let remote = Timestamp {
            ms: 10_000,
            counter: 7,
            device: "other".into(),
        };
        clock.observe_at(&remote, 9_000);
        assert!(clock.tick_at(9_000) > remote);

        let bogus = Timestamp {
            ms: 9_000 + MAX_DRIFT_MS * 3,
            counter: 0,
            device: "other".into(),
        };
        clock.observe_at(&bogus, 9_000);
        assert!(clock.tick_at(9_001) < bogus);
    }
}
