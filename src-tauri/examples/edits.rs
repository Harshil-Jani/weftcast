//! Exercises the editing paths on an existing library video: `cargo run --example edits -- <id>`.
use app_lib::{cuts, library, media};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let id = std::env::args().nth(1).expect("video id");
    let v = library::load(&id)?;
    let words = library::load_transcript(&id)
        .map(|t| t.words)
        .unwrap_or_default();
    let src = library::video_path(&id);
    let dir = library::dir(&id);

    let log = media::silences(&src, -35, 0.8).await?;
    let plan = cuts::plan(
        v.duration,
        &words,
        &cuts::parse_silencedetect(&log, v.duration),
        &cuts::CutOptions::default(),
    );
    let cut = dir.join("test-cut.mp4");
    media::render_keep(&src, &cut, &plan.keep).await?;
    println!(
        "cut: planned {:.2}s, rendered {:.2}s",
        plan.result,
        media::duration(&cut).await?
    );

    let trimmed = dir.join("test-trim.mp4");
    media::trim(&src, &trimmed, 1.0, 6.0).await?;
    println!("trim 1-6s: {:.2}s", media::duration(&trimmed).await?);

    let srt = dir.join("test.srt");
    std::fs::write(&srt, library::to_srt(&words))?;
    let cc = dir.join("test-cc.mp4");
    if media::can_burn().await {
        media::burn_captions(&src, &srt, &cc).await?;
        println!("captions burned in");
    } else {
        media::embed_captions(&src, &srt, &cc).await?;
        let streams = media::run(
            "ffprobe",
            &[
                "-v",
                "error",
                "-show_entries",
                "stream=codec_name",
                "-of",
                "csv=p=0",
                &cc.to_string_lossy(),
            ],
        )
        .await?;
        println!(
            "captions embedded, streams: {}",
            streams.split_whitespace().collect::<Vec<_>>().join(",")
        );
    }
    println!(
        "first SRT cue:\n{}",
        library::to_srt(&words)
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join("\n")
    );
    for f in [cut, trimmed, cc, srt] {
        let _ = std::fs::remove_file(f);
    }
    Ok(())
}
