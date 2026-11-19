//! 区分说话人：每段话算一个声纹（一串数，同一个人说的话声纹相近），和已经认出来的说话人比，
//! 够像就算同一个人，都不像就记成新的一个人。
//!
//! 不用一次处理整期的说话人分离：那要把整期音频放进内存，也没法边听边转。这样一段一段地来，
//! 从中间开始转、跳着转都行。认出来的说话人（每人一个平均声纹）存在字幕文件里，下次接着转编号不乱。
//!
//! 阈值是拿 3D-Speaker CAM++ 声纹量出来的：同一个人的 3 秒片段相似度 0.78–0.88，不同的人 0.2–0.6
//! （同一档节目里两位男主持也在 0.6 以下）。

use serde::{Deserialize, Serialize};

/// 比这更像就算同一个人（余弦相似度）。
const SAME: f32 = 0.68;
/// 短的一段声纹不准：至少这么像才归给某个人，不然就不标是谁。
const SHORT_SAME: f32 = 0.6;
/// 一段话至少这么长（秒）才可能记成新的人；更短的声纹不可靠。
const NEW_MIN_SECONDS: f64 = 1.5;
/// 最多分出这么多人。再多的归给最像的那个。
const MAX_SPEAKERS: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Speakers {
    /// 每个人的平均声纹（归一化过的），下标就是说话人编号。
    #[serde(default)]
    centroids: Vec<Vec<f32>>,
    /// 每个人算进平均里的段数。
    #[serde(default)]
    counts: Vec<u32>,
}

fn normalize(v: &[f32]) -> Option<Vec<f32>> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    (norm > f32::EPSILON && norm.is_finite()).then(|| v.iter().map(|x| x / norm).collect())
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

impl Speakers {
    pub fn len(&self) -> usize {
        self.centroids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.centroids.is_empty()
    }

    /// 这段话是谁说的。`seconds` 是这段话多长。认不准时为空。
    pub fn assign(&mut self, embedding: &[f32], seconds: f64) -> Option<u32> {
        let v = normalize(embedding)?;
        // 换了声纹模型（维数不同）：以前的不能比了，从头认
        if self.centroids.first().is_some_and(|c| c.len() != v.len()) {
            *self = Speakers::default();
        }
        let best = self
            .centroids
            .iter()
            .enumerate()
            .map(|(i, c)| (i, dot(c, &v)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let long = seconds >= NEW_MIN_SECONDS;
        let chosen = match best {
            Some((i, score)) if score >= SAME => Some(i),
            Some((i, score)) if !long && score >= SHORT_SAME => Some(i),
            // 人数到顶了：归给最像的
            Some((i, _)) if long && self.centroids.len() >= MAX_SPEAKERS => Some(i),
            _ if long => {
                self.centroids.push(v.clone());
                self.counts.push(0);
                Some(self.centroids.len() - 1)
            }
            _ => None,
        }?;
        // 只用够长的段更新这个人的平均声纹
        if long {
            let n = self.counts[chosen] as f32;
            let c = &mut self.centroids[chosen];
            for (x, y) in c.iter_mut().zip(&v) {
                *x = (*x * n + y) / (n + 1.0);
            }
            if let Some(again) = normalize(c) {
                *c = again;
            }
            self.counts[chosen] += 1;
        }
        Some(chosen as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个人的声纹：主方向加一点噪声。
    fn voice(axis: usize, noise: f32, seed: u32) -> Vec<f32> {
        let mut v = vec![0.0f32; 16];
        v[axis] = 1.0;
        let mut x = seed.wrapping_mul(2_654_435_761);
        for item in &mut v {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *item += noise * ((x % 1000) as f32 / 1000.0 - 0.5);
        }
        v
    }

    #[test]
    fn same_voices_get_the_same_number() {
        let mut s = Speakers::default();
        assert_eq!(s.assign(&voice(0, 0.3, 1), 4.0), Some(0));
        assert_eq!(s.assign(&voice(1, 0.3, 2), 4.0), Some(1));
        assert_eq!(s.assign(&voice(0, 0.3, 3), 3.0), Some(0));
        assert_eq!(s.assign(&voice(1, 0.3, 4), 6.0), Some(1));
        assert_eq!(s.assign(&voice(2, 0.3, 5), 2.0), Some(2));
        assert_eq!(s.len(), 3);
        // 存下来再读回来，接着认
        let saved = serde_json::to_string(&s).unwrap();
        let mut back: Speakers = serde_json::from_str(&saved).unwrap();
        assert_eq!(back.assign(&voice(1, 0.3, 6), 4.0), Some(1));
    }

    #[test]
    fn short_or_unclear_segments_dont_make_new_speakers() {
        let mut s = Speakers::default();
        // 一开始就是一小段：不知道是谁，也不记成新的人
        assert_eq!(s.assign(&voice(0, 0.3, 1), 0.8), None);
        assert!(s.is_empty());
        assert_eq!(s.assign(&voice(0, 0.3, 2), 4.0), Some(0));
        // 短的、像的归给那个人
        assert_eq!(s.assign(&voice(0, 0.6, 3), 0.8), Some(0));
        // 短的、不像任何人的：不标
        assert_eq!(s.assign(&voice(5, 0.3, 4), 0.8), None);
        assert_eq!(s.len(), 1);
        // 全是零的声纹没法比
        assert_eq!(s.assign(&[0.0; 16], 4.0), None);
    }

    #[test]
    fn at_most_eight_speakers() {
        let mut s = Speakers::default();
        for axis in 0..12 {
            s.assign(&voice(axis, 0.05, axis as u32), 4.0);
        }
        assert_eq!(s.len(), MAX_SPEAKERS);
    }

    #[test]
    fn a_different_embedding_size_starts_over() {
        let mut s = Speakers::default();
        s.assign(&voice(0, 0.1, 1), 4.0);
        assert_eq!(s.assign(&[1.0, 0.0, 0.0], 4.0), Some(0));
        assert_eq!(s.len(), 1);
    }
}
