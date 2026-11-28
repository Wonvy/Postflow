//! 用真正的模型翻译。要从 Mozilla 下载约 100 MB，默认不跑：
//!
//!     SHIBAPOST_TRANSLATE_DIR=/tmp/shibapost-models cargo test -p shibapost-translate --release -- --ignored --nocapture
//!
//! 模型下载过一次就留在那个目录里。

use shibapost_translate::{Mark, Segment, Translator};

async fn translator() -> Translator {
    let dir = std::env::var("SHIBAPOST_TRANSLATE_DIR").expect("set SHIBAPOST_TRANSLATE_DIR");
    let t = Translator::new(dir).unwrap();
    for pair in ["en-zh", "zh-en"] {
        if !t.installed().iter().any(|i| i.pair == pair) {
            assert!(t.begin_download(pair).unwrap());
            t.download(pair, |p| eprintln!("{} {}/{}", p.pair, p.received, p.total))
                .await
                .unwrap();
        }
    }
    t
}

fn marked(s: &Segment, m: &Mark) -> String {
    String::from_utf16(&s.text.encode_utf16().collect::<Vec<_>>()[m.start..m.end]).unwrap()
}

#[tokio::test]
#[ignore]
async fn translates_with_formatting() {
    let t = translator().await;

    let en = Segment {
        text: "Hi Mia, the Q4 budget is attached. Please review it before Friday.".into(),
        // 「Q4 budget」是粗体，「before Friday」是链接
        marks: vec![
            Mark {
                start: 12,
                end: 21,
                id: 0,
            },
            Mark {
                start: 52,
                end: 65,
                id: 1,
            },
        ],
    };
    let out = t.translate("en", "zh", vec![en.clone()]).unwrap();
    eprintln!("{} -> {:?}", en.text, out[0]);
    for m in &out[0].marks {
        eprintln!("  mark {} = {}", m.id, marked(&out[0], m));
    }
    assert!(out[0].text.contains("预算"));
    assert!(!out[0].marks.is_empty());

    let zh = Segment {
        text: "本地优先的软件把数据放在你自己的设备上。同步交给你信任的工具。".into(),
        marks: vec![Mark {
            start: 0,
            end: 5,
            id: 0,
        }],
    };
    let out = t
        .translate(
            "zh",
            "en",
            vec![zh.clone(), Segment::plain("Best regards, Mia")],
        )
        .unwrap();
    eprintln!("{} -> {:?}", zh.text, out[0]);
    for m in &out[0].marks {
        eprintln!("  mark {} = {}", m.id, marked(&out[0], m));
    }
    // 句子之间要有空格
    assert!(out[0].text.contains(". "));
    // 没有中文的段落原样保留
    assert_eq!(out[1].text, "Best regards, Mia");

    let plan = t
        .plan("今天天气很好，我们下午去公园散步吧。", None, "en")
        .await
        .unwrap();
    assert_eq!(plan.from.as_deref(), Some("zh"));
    assert!(plan.missing.is_empty());
}
