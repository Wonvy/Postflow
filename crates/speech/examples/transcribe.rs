//! 命令行里试语音识别，开发时用：
//!
//! ```text
//! cargo run --release -p postflow-speech --example transcribe -- models <目录>
//! cargo run --release -p postflow-speech --example transcribe -- download <目录> <模型 ID>
//! cargo run --release -p postflow-speech --example transcribe -- run <目录> <模型 ID> <音频文件> [从第几秒] [到第几秒]
//! ```
//!
//! `<目录>` 相当于应用的 `本机目录/speech`，比如 `.postflow-dev/local/speech`。

use std::path::Path;
use std::time::Instant;

use postflow_speech::{AudioReader, Speakers, Speech};

fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0);
    format!(
        "{:02}:{:02}:{:06.3}",
        (s / 3600.0) as u64,
        ((s / 60.0) as u64) % 60,
        s % 60.0
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    match (arg(0), arg(1)) {
        (Some("models"), Some(root)) => {
            let speech = Speech::new(root)?;
            speech.refresh_catalog().await;
            let overview = speech.overview();
            for m in overview.models {
                println!(
                    "{:<20} {:>5} MB  {}{}{}",
                    m.model.id,
                    m.size / 1_000_000,
                    m.model.languages.join(","),
                    if m.installed { "  [installed]" } else { "" },
                    if overview.selected.as_deref() == Some(&m.model.id) {
                        "  [selected]"
                    } else {
                        ""
                    },
                );
            }
        }
        (Some("download"), Some(root)) => {
            let id = arg(2).ok_or("which model?")?;
            let speech = Speech::new(root)?;
            if !speech.begin_download(id)? {
                return Err("already downloading".into());
            }
            let started = Instant::now();
            let mut last = 0u64;
            speech
                .download(id, |p| {
                    if p.received - last >= 20_000_000 || p.received == p.total {
                        last = p.received;
                        println!("{} / {} MB", p.received / 1_000_000, p.total / 1_000_000);
                    }
                })
                .await?;
            println!("installed in {:.1}s", started.elapsed().as_secs_f64());
            println!("sources, fastest first: {:?}", speech.sources());
        }
        (Some("run"), Some(root)) => {
            let id = arg(2).ok_or("which model?")?;
            let audio = arg(3).ok_or("which audio file?")?;
            let from: f64 = arg(4).map_or(Ok(0.0), str::parse)?;
            let until: f64 = arg(5).map_or(Ok(f64::INFINITY), str::parse)?;
            let speech = Speech::new(root)?;
            let started = Instant::now();
            let engine = speech.load(id)?;
            println!("model loaded in {:.2}s", started.elapsed().as_secs_f64());
            let mut reader = AudioReader::open(Path::new(audio))?;
            println!("duration: {:?}", reader.duration().map(clock));
            let begin = if from > 0.0 { reader.seek(from)? } else { 0.0 };
            let started = Instant::now();
            let mut first: Option<f64> = None;
            let mut count = 0usize;
            let mut speakers = Speakers::default();
            let outcome = engine.run(&mut reader, until, &mut speakers, &mut |step| {
                for cue in &step.cues {
                    first.get_or_insert(started.elapsed().as_secs_f64());
                    count += 1;
                    let who = cue
                        .speaker
                        .map(|s| format!(" [S{}]", s + 1))
                        .unwrap_or_default();
                    println!(
                        "[{} → {}]{who} {}",
                        clock(cue.start),
                        clock(cue.end),
                        cue.text
                    );
                }
                true
            })?;
            if !speakers.is_empty() {
                println!("voices: {:?}", speakers.genders());
            }
            let took = started.elapsed().as_secs_f64();
            let audio_seconds = outcome.reached - begin;
            println!(
                "\n{count} cues, {} of audio in {took:.1}s ({:.1}x real time), first cue after {:.2}s, eof={}",
                clock(audio_seconds),
                audio_seconds / took.max(0.001),
                first.unwrap_or(0.0),
                outcome.eof,
            );
        }
        _ => {
            eprintln!("usage: transcribe models|download|run <dir> [model] [audio] [from] [until]");
            std::process::exit(2);
        }
    }
    Ok(())
}
