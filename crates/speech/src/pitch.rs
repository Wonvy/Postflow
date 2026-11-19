//! 估一段话的音高（基频，赫兹），用来猜说话人是男声还是女声。
//!
//! 不用模型，用 YIN 算法（de Cheveigné & Kawahara 2002）：每 32 毫秒一帧，够响的帧算「累积均值归一化差分」，
//! 在 70–400 Hz 对应的周期里找第一个低于阈值的谷，比直接找自相关的最高点少犯「高八度」的错。
//! 一段里取所有浊音帧的中位数。成年男声平均大约 85–160 Hz，女声 165–255 Hz。
//! 只用来给图标分男女，猜不准的（中间那段）就不分。

use crate::audio::SAMPLE_RATE;

/// 每帧拿来比的长度（样本数，16 kHz 下 32 毫秒）。
const WINDOW: usize = 512;
/// 帧和帧隔多远。
const HOP: usize = 320;
const MIN_HZ: f32 = 70.0;
const MAX_HZ: f32 = 400.0;
/// 归一化差分低于它就算找到了周期（YIN 论文里的绝对阈值）。
const THRESHOLD: f32 = 0.1;
/// 两倍周期处的谷比找到的谷还低这么多倍，就是找成了高八度。
const OCTAVE: f32 = 0.7;
/// 最好的谷都高于它：没有明显的周期，当成清音或噪声。
const UNVOICED: f32 = 0.35;
/// 比这一段最响的帧小这么多倍的帧当成静音。
const QUIET: f32 = 0.1;
/// 至少要这么多浊音帧才给结果。
const MIN_FRAMES: usize = 5;

/// 一帧的周期（样本数，带小数）；不是浊音是 None。`x` 长 `WINDOW + max_lag`。
fn yin(x: &[f32], min_lag: usize, max_lag: usize, diff: &mut Vec<f32>) -> Option<f32> {
    diff.clear();
    diff.resize(max_lag + 1, 0.0);
    for (tau, d) in diff.iter_mut().enumerate().skip(1) {
        *d = x[..WINDOW]
            .iter()
            .zip(&x[tau..tau + WINDOW])
            .map(|(a, b)| (a - b) * (a - b))
            .sum();
    }
    // 累积均值归一化
    diff[0] = 1.0;
    let mut running = 0.0f32;
    for (tau, d) in diff.iter_mut().enumerate().skip(1) {
        running += *d;
        *d = if running > f32::EPSILON {
            *d * tau as f32 / running
        } else {
            1.0
        };
    }
    // 第一个低于阈值的谷；没有就用整体最低的（够低才算）
    let mut tau = (min_lag..=max_lag).find(|&t| diff[t] < THRESHOLD);
    if let Some(mut t) = tau {
        while t < max_lag && diff[t + 1] < diff[t] {
            t += 1;
        }
        tau = Some(t);
    } else {
        let best = (min_lag..=max_lag).min_by(|&a, &b| diff[a].total_cmp(&diff[b]))?;
        if diff[best] > UNVOICED {
            return None;
        }
        tau = Some(best);
    }
    let mut t = tau?;
    // 「高八度」：基频弱、二次谐波强时（大声说话、有回声），半个周期处的谷也够低，会被当成周期。
    // 两倍周期处的谷明显更深，真正的周期是它
    let double = t * 2;
    if double + 2 <= max_lag {
        let around = (double - 2..=double + 2).min_by(|&a, &b| diff[a].total_cmp(&diff[b]));
        if let Some(d) = around
            && diff[d] < diff[t] * OCTAVE
        {
            t = d;
        }
    }
    // 抛物线插值，周期精确到小数
    if t > min_lag && t < max_lag {
        let (a, b, c) = (diff[t - 1], diff[t], diff[t + 1]);
        let den = a - 2.0 * b + c;
        if den.abs() > f32::EPSILON {
            return Some(t as f32 + 0.5 * (a - c) / den);
        }
    }
    Some(t as f32)
}

/// 这一段话的音高（赫兹）。认不出来（太短、太吵、没有浊音）是 None。`samples` 是 16 kHz 单声道。
pub fn median_pitch(samples: &[f32]) -> Option<f32> {
    let rate = SAMPLE_RATE as f32;
    let min_lag = (rate / MAX_HZ).floor() as usize;
    let max_lag = (rate / MIN_HZ).ceil() as usize;
    let frame = WINDOW + max_lag;
    if samples.len() < frame {
        return None;
    }
    let energy = |f: &[f32]| f.iter().map(|x| x * x).sum::<f32>() / f.len() as f32;
    let frames: Vec<&[f32]> = (0..=samples.len() - frame)
        .step_by(HOP)
        .map(|i| &samples[i..i + frame])
        .collect();
    let loudest = frames
        .iter()
        .map(|f| energy(&f[..WINDOW]))
        .fold(0.0f32, f32::max);
    if loudest <= f32::EPSILON {
        return None;
    }
    let mut diff = Vec::new();
    let mut found: Vec<f32> = frames
        .into_iter()
        .filter(|f| energy(&f[..WINDOW]) >= loudest * QUIET)
        .filter_map(|f| yin(f, min_lag, max_lag, &mut diff))
        .map(|period| rate / period)
        .collect();
    if found.len() < MIN_FRAMES {
        return None;
    }
    found.sort_by(f32::total_cmp);
    Some(found[found.len() / 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 像说话声的信号：基频加几个泛音，音高慢慢起伏，混一点噪声。
    fn voice(f0: f32, seconds: f32) -> Vec<f32> {
        let n = (SAMPLE_RATE as f32 * seconds) as usize;
        let mut phase = 0.0f32;
        let mut seed = 12345u32;
        (0..n)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let f = f0 * (1.0 + 0.05 * (2.0 * std::f32::consts::PI * 3.0 * t).sin());
                phase += 2.0 * std::f32::consts::PI * f / SAMPLE_RATE as f32;
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let noise = (seed % 1000) as f32 / 1000.0 - 0.5;
                0.5 * phase.sin()
                    + 0.3 * (2.0 * phase).sin()
                    + 0.2 * (3.0 * phase).sin()
                    + 0.05 * noise
            })
            .collect()
    }

    #[test]
    fn male_and_female_voices_are_told_apart() {
        let male = median_pitch(&voice(115.0, 2.0)).unwrap();
        assert!((105.0..125.0).contains(&male), "{male}");
        let female = median_pitch(&voice(215.0, 2.0)).unwrap();
        assert!((200.0..230.0).contains(&female), "{female}");
    }

    #[test]
    fn no_pitch_in_silence_noise_or_tiny_clips() {
        assert_eq!(median_pitch(&[0.0; 16000]), None);
        assert_eq!(median_pitch(&voice(150.0, 0.02)), None);
        let mut seed = 7u32;
        let noise: Vec<f32> = (0..32000)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed % 1000) as f32 / 1000.0 - 0.5
            })
            .collect();
        assert_eq!(median_pitch(&noise), None);
    }
}
