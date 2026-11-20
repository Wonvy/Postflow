//! 命令行里试朗读，开发时用：
//!
//! ```text
//! cargo run --release -p postflow-speech --example speak -- models <目录>
//! cargo run --release -p postflow-speech --example speak -- download <目录> <模型 ID>
//! cargo run --release -p postflow-speech --example speak -- import <目录> <模型 ID> <放模型文件的文件夹>
//! cargo run --release -p postflow-speech --example speak -- prepare <目录> <模型 ID> <文字>
//! cargo run --release -p postflow-speech --example speak -- say <目录> <模型 ID> <声音> <文字> [输出.wav]
//! ```
//!
//! ZipVoice 没有自带的声音：`<声音>` 写一段录音的路径，再用环境变量 `SAMPLE_TEXT` 给出录音里说的话。
//!
//! `<目录>` 相当于应用的 `本机目录/voices`，比如 `.postflow-dev/local/voices`。

use std::path::Path;
use std::time::Instant;

use postflow_speech::{AudioReader, Speaker, Speech, tts};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    let root = arg(1).ok_or("which folder?")?;
    let voices = Speech::voices(root)?;
    match arg(0) {
        Some("models") => {
            voices.refresh_catalog().await;
            for m in voices.overview().models {
                println!(
                    "{:<16} {:>5} MB  {} voices{}",
                    m.model.id,
                    m.size / 1_000_000,
                    m.model.speakers.len(),
                    if m.installed { "  [installed]" } else { "" },
                );
            }
        }
        Some("download") => {
            let id = arg(2).ok_or("which model?")?;
            if !voices.begin_download(id)? {
                return Err("already downloading".into());
            }
            let mut last = 0u64;
            voices
                .download(id, |p| {
                    if p.received - last >= 20_000_000 || p.received == p.total {
                        last = p.received;
                        println!("{} / {} MB", p.received / 1_000_000, p.total / 1_000_000);
                    }
                })
                .await?;
            println!("installed {id}");
        }
        Some("import") => {
            let (id, folder) = (
                arg(2).ok_or("which model?")?,
                arg(3).ok_or("which folder?")?,
            );
            voices.import(id, Path::new(folder)).await?;
            println!("installed {id}");
        }
        Some("prepare") => {
            let (id, text) = (arg(2).ok_or("which model?")?, arg(3).ok_or("what text?")?);
            let voice = voices.load_voice(id)?;
            println!("{}", voice.prepare(text));
        }
        Some("say") => {
            let id = arg(2).ok_or("which model?")?;
            let speaker = arg(3).ok_or("which voice?")?;
            let text = arg(4).ok_or("what text?")?;
            let out = arg(5).unwrap_or("speak.wav");
            let started = Instant::now();
            let voice = voices.load_voice(id)?;
            println!("loaded in {:.2}s", started.elapsed().as_secs_f64());
            println!("reading: {}", voice.prepare(text));
            let clip;
            let sample_text = std::env::var("SAMPLE_TEXT").unwrap_or_default();
            let who = if voice.needs_sample() {
                clip = tts::read_sample(AudioReader::open(Path::new(speaker))?)?;
                println!(
                    "sample: {:.1}s",
                    clip.len() as f64 / f64::from(tts::SAMPLE_RATE)
                );
                Speaker::Sample {
                    audio: &clip,
                    text: &sample_text,
                }
            } else {
                Speaker::Preset(speaker)
            };
            let started = Instant::now();
            let samples = voice.speak(text, who, 1.0)?.samples;
            let took = started.elapsed().as_secs_f64();
            let seconds = samples.len() as f64 / f64::from(voice.sample_rate());
            println!(
                "{seconds:.2}s of audio in {took:.2}s ({:.1}x real time)",
                seconds / took
            );
            std::fs::write(out, tts::wav(&samples, voice.sample_rate()))?;
            println!("saved {out}");
        }
        _ => return Err("models | download | import | prepare | say".into()),
    }
    Ok(())
}
