//! 真的跑一次语音识别：下载目录里的模型、解码一段 Opus 录音、识别、分说话人，看认出来的字对不对。
//!
//! 要联网下载约 190 MB 的模型，所以默认跳过：设置了环境变量 `POSTFLOW_SPEECH_SMOKE`（模型放在哪个目录）
//! 才跑。CI 在 Linux 和 Windows 上跑它，模型目录缓存起来，模型目录（public/catalog/speech.json）变了才重新下载。
//!
//! 它能发现只有真跑才知道的问题：预编译的 sherpa-onnx 库在这个系统上加载不了、Opus 解码不对、
//! 下载或校验出错、声纹模型加载不了。
//!
//! ```text
//! POSTFLOW_SPEECH_SMOKE=/tmp/speech-smoke cargo test -p postflow-speech --test recognize -- --nocapture
//! ```

use std::path::Path;

use postflow_speech::{AudioReader, Cue, Speakers, Speech};

const MODEL: &str = "whisper-base";
/// 录音里说的话（LibriSpeech 的标注）。
const REFERENCE: &str = "after early nightfall the yellow lamps would light up here and there the squalid quarter of the brothels";
const SPEAKER_MODEL: &str = "speaker-campplus";

#[tokio::test(flavor = "multi_thread")]
async fn recognizes_real_speech() {
    let Some(root) = std::env::var_os("POSTFLOW_SPEECH_SMOKE") else {
        eprintln!("skipped: set POSTFLOW_SPEECH_SMOKE to a folder for the models to run this test");
        return;
    };
    let speech = Speech::new(root).expect("open the models folder");
    for id in [MODEL, SPEAKER_MODEL] {
        let installed = speech
            .overview()
            .models
            .iter()
            .any(|m| m.model.id == id && m.installed);
        if installed {
            continue;
        }
        assert!(speech.begin_download(id).expect("start download"));
        speech
            .download(id, |_| {})
            .await
            .unwrap_or_else(|e| panic!("download {id}: {e}"));
        eprintln!(
            "downloaded {id}; sources, fastest first: {:?}",
            speech.sources()
        );
    }

    let engine = speech.load(MODEL).expect("load the recognition model");
    assert!(engine.has_speakers(), "the speaker model should load too");

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nightfall-opus.caf");
    let mut reader = AudioReader::open(&fixture).expect("open the recording");
    let mut speakers = Speakers::default();
    let mut cues: Vec<Cue> = Vec::new();
    let outcome = engine
        .run(&mut reader, f64::INFINITY, &mut speakers, &mut |step| {
            cues.extend(step.cues);
            true
        })
        .expect("recognize");
    for c in &cues {
        eprintln!("[{:.2} → {:.2}] {:?} {}", c.start, c.end, c.speaker, c.text);
    }

    assert!(outcome.eof);
    assert!((6.0..7.5).contains(&outcome.reached), "{}", outcome.reached);
    let text = cues
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    // 量化模型在不同的 CPU 上算出来会有一点差别，个别词可能认得不一样（比如 squalid）。
    // 这里要发现的是「识别不能用」，所以大部分词对上就算过
    let said: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let expected = REFERENCE.split(' ').collect::<Vec<_>>();
    let found = expected
        .iter()
        .filter(|w| said.iter().any(|s| s == *w))
        .count();
    assert!(
        found * 10 >= expected.len() * 8,
        "only {found} of {} words recognized: {text:?}",
        expected.len()
    );
    // 句子按时间排，落在录音里
    assert!(cues.windows(2).all(|w| w[0].start <= w[1].start));
    assert!(
        cues.iter()
            .all(|c| c.start >= 0.0 && c.end <= 7.5 && c.start < c.end)
    );
    // 声纹模型在工作：只有一个人在说，第一句就记成第一个人
    assert_eq!(cues[0].speaker, Some(0), "{} speakers", speakers.len());
}
