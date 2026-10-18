//! End-to-end check without the UI: `cargo run --example smoke -- [--record SECONDS | --file PATH]`.
//! Records (or imports a file), transcribes locally, asks Claude for title/summary/chapters,
//! plans the um/silence cuts, and fetches the watch page from the local server.
use app_lib::{ai, capture, cuts, library, media, server, transcribe};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let v = if let Some(i) = args.iter().position(|a| a == "--file") {
        let id = library::new_id();
        std::fs::create_dir_all(library::dir(&id))?;
        let seg = library::dir(&id).join("segment-0.mp4");
        std::fs::copy(&args[i + 1], &seg)?;
        capture::finish(&id, &[seg]).await?
    } else {
        let secs: u64 = args
            .iter()
            .position(|a| a == "--record")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        let devs = capture::devices().await?;
        println!(
            "devices: {} screens, {} mics",
            devs.screens.len(),
            devs.mics.len()
        );
        let rec = capture::Capture::default();
        let opts = capture::StartOptions {
            screen: devs.screens[0].index.clone(),
            mic: devs.mics.first().map(|m| m.index.clone()),
        };
        rec.start(opts).await?;
        tokio::time::sleep(std::time::Duration::from_secs(secs / 2)).await;
        rec.pause().await?;
        println!("paused, state: {:?}", rec.state().await);
        rec.resume().await?;
        tokio::time::sleep(std::time::Duration::from_secs(secs - secs / 2)).await;
        rec.stop().await?
    };
    println!(
        "saved {} ({:.1}s) at {}",
        v.id,
        v.duration,
        library::video_path(&v.id).display()
    );

    let t = transcribe::transcribe(
        &v.id,
        std::sync::Arc::new(|stage: &str, d: u64, t: u64| {
            if let Some(p) = (d * 100).checked_div(t) {
                eprint!("\r{stage} {p}%   ")
            }
        }),
    )
    .await?;
    println!(
        "transcript ({} words): {}",
        t.words.len(),
        t.text.chars().take(200).collect::<String>()
    );
    let mut v = library::load(&v.id)?;
    v.has_transcript = !t.words.is_empty();
    library::save(&v)?;

    if !t.words.is_empty() {
        let ins = ai::insights(&t, v.duration).await?;
        println!(
            "title: {}\nsummary: {}\nchapters: {:?}",
            ins.title, ins.summary, ins.chapters
        );
        v.title = ins.title;
        v.summary = Some(ins.summary);
        v.chapters = ins.chapters;
        library::save(&v)?;
        let a = ai::ask(&t, &v.title, "What is this video about?").await?;
        println!("ask: {} {:?}", a.answer, a.timestamps);
    }

    let log = media::silences(&library::video_path(&v.id), -35, 0.8).await?;
    let plan = cuts::plan(
        v.duration,
        &t.words,
        &cuts::parse_silencedetect(&log, v.duration),
        &cuts::CutOptions::default(),
    );
    println!(
        "cuts: {} removals, {:.1}s -> {:.1}s",
        plan.removed.len(),
        plan.original,
        plan.result
    );

    let port = server::start().await?;
    let page = reqwest::get(format!("http://127.0.0.1:{port}/v/{}", v.share_id))
        .await?
        .text()
        .await?;
    let meta: serde_json::Value =
        reqwest::get(format!("http://127.0.0.1:{port}/api/v/{}", v.share_id))
            .await?
            .json()
            .await?;
    let media = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/media/{}.mp4", v.share_id))
        .header("range", "bytes=0-99")
        .send()
        .await?;
    println!(
        "watch page {} bytes, meta title {:?}, media range status {}",
        page.len(),
        meta["title"],
        media.status()
    );
    println!("WATCH http://127.0.0.1:{port}/v/{}", v.share_id);
    if args.iter().any(|a| a == "--serve") {
        tokio::time::sleep(std::time::Duration::from_secs(600)).await;
    }
    Ok(())
}
