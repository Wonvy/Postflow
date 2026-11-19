//! 音频解码：不用模型，只看解出来的样本对不对。

use std::path::Path;

use postflow_speech::AudioReader;
use postflow_speech::audio::SAMPLE_RATE;

/// 读完整个文件，返回所有样本。
fn read_all(reader: &mut AudioReader) -> Vec<f32> {
    let mut all = Vec::new();
    while let Some(chunk) = reader.read().unwrap() {
        all.extend_from_slice(chunk.samples);
    }
    all
}

/// Opus（Symphonia 不支持，用 opus-decoder）：立体声的 440 Hz 正弦波解成 16 kHz 单声道，长度和音高都对。
#[test]
fn opus_decodes_to_16khz_mono() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tone-440hz.caf");
    let mut reader = AudioReader::open(&path).unwrap();
    let samples = read_all(&mut reader);
    let seconds = samples.len() as f64 / f64::from(SAMPLE_RATE);
    assert!((seconds - 1.5).abs() < 0.05, "decoded {seconds:.3} s");
    assert!((reader.duration().unwrap() - 1.5).abs() < 0.05);

    // 中间一秒：音量和过零次数（440 Hz 一秒过零 880 次）
    let middle = &samples[SAMPLE_RATE as usize / 4..SAMPLE_RATE as usize * 5 / 4];
    let rms = (middle.iter().map(|s| s * s).sum::<f32>() / middle.len() as f32).sqrt();
    assert!(rms > 0.25 && rms < 0.45, "rms {rms}");
    let crossings = middle
        .windows(2)
        .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
        .count();
    assert!(
        (860..=900).contains(&crossings),
        "{crossings} zero crossings"
    );
    // 跳转在 CAF 里不好使（解码库的问题）；播客用的 Ogg 封装手动试过能跳
}
