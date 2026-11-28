//! 对着一个真实的 AI 服务把几种请求走一遍，开发时排查某个服务或模型的问题用：
//!
//! ```text
//! SHIBAPOST_AI_URL=https://open.bigmodel.cn/api/paas/v4 SHIBAPOST_AI_MODEL=glm-4.5 \
//! SHIBAPOST_AI_KEY=… cargo run -p shibapost-ai --example try
//! ```
//!
//! `SHIBAPOST_AI_PRESET` 是预设的 ID（比如 `zhipu`），不给就是 `custom`。密钥只从环境变量读，不会打印。
//! 发出去的是下面这段公开的文字，不会碰你的邮件和订阅。

use std::time::Instant;

use shibapost_ai::{Client, Message, Provider, Request, prompts};

const DOC: prompts::Doc<'static> = prompts::Doc {
    title: "Enabling the parallel frontend on nightly",
    source: "Rust Blog",
    text: "TL;DR: We are enabling the parallel frontend of the Rust compiler on nightly by default, to find \
remaining issues in preparation for its stabilization.\n\n\
The Rust compiler has been using multiple threads to compile code in its backend part (using LLVM) for a long \
time. However, its frontend part (which includes type checking and borrow checking) has been executing \
sequentially, on a single thread, which of course limits the performance of the compiler.\n\n\
The compiler frontend actually had support for running in parallel for a long time, but it was not enabled by \
default due to various issues. Three years ago, we made it possible to opt into the parallel frontend on the \
nightly channel, with the plan to stabilize it in 2024. Clearly, we did not achieve that goal, but work on the \
parallel frontend was still slowly progressing in the meantime.\n\n\
Last year, several contributors started a coordinated effort to finally get the parallel frontend over the \
finish line. Since then, many long-standing issues were fixed, and its performance was improved. With eight \
threads, clean builds of large crates are typically 20 to 30 percent faster, at the cost of somewhat higher \
memory use.",
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let provider = Provider {
        id: "try".into(),
        preset: env("SHIBAPOST_AI_PRESET").unwrap_or_else(|| "custom".into()),
        name: "try".into(),
        base_url: env("SHIBAPOST_AI_URL").ok_or("set SHIBAPOST_AI_URL")?,
        model: env("SHIBAPOST_AI_MODEL").ok_or("set SHIBAPOST_AI_MODEL")?,
        proxy: env("SHIBAPOST_AI_PROXY"),
    };
    let key = env("SHIBAPOST_AI_KEY");
    let lang = env("SHIBAPOST_AI_LANG").unwrap_or_else(|| "zh".into());
    let client = Client::new();
    println!("{} · {}", provider.base_url, provider.model);

    // 和设置里的「测试连接」一样：只给 16 个 token
    let started = Instant::now();
    let ping = Request {
        messages: vec![Message::user("Reply with the single word OK.")],
        max_tokens: Some(16),
        ..Default::default()
    };
    match client.complete(&provider, key.as_deref(), &ping).await {
        Ok(r) => println!(
            "\n[test] {:.1}s, {} output tokens: {}",
            started.elapsed().as_secs_f64(),
            r.usage.output,
            r.text.trim()
        ),
        Err(e) => println!("\n[test] failed ({}): {e}", e.code()),
    }

    let started = Instant::now();
    let mut pieces = 0usize;
    let mut first: Option<f64> = None;
    let summary = client
        .stream(
            &provider,
            key.as_deref(),
            &prompts::summary(DOC, &lang),
            |_| {
                pieces += 1;
                first.get_or_insert(started.elapsed().as_secs_f64());
            },
        )
        .await;
    match summary {
        Ok(r) => println!(
            "\n[summary] {:.1}s (first text after {:.1}s), {pieces} pieces, {} output tokens:\n{}",
            started.elapsed().as_secs_f64(),
            first.unwrap_or(0.0),
            r.usage.output,
            r.text
        ),
        Err(e) => println!("\n[summary] failed ({}): {e}", e.code()),
    }

    let started = Instant::now();
    match client
        .complete(&provider, key.as_deref(), &prompts::keywords(DOC, &lang))
        .await
    {
        Ok(r) => println!(
            "\n[keywords] {:.1}s, {} output tokens: {:?}",
            started.elapsed().as_secs_f64(),
            r.usage.output,
            prompts::parse_keywords(&r.text)
        ),
        Err(e) => println!("\n[keywords] failed ({}): {e}", e.code()),
    }
    Ok(())
}
