//! AI features end to end: `cargo run --example ai -- <id>`.
//! Checks the remapped transcript, makes a 10 second short version and translates captions to Hindi.
use app_lib::{captions, library, shorter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let id = std::env::args().nth(1).expect("video id");
    let v = library::load(&id)?;
    let t = library::load_transcript(&id).expect("transcript");
    println!(
        "after edit: {:.2}s, first words at {:.2}s: {}",
        v.duration,
        t.words[0].start,
        t.words
            .iter()
            .take(6)
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    );

    let plan = shorter::plan(&v, &t, 10.0).await?;
    println!(
        "short plan: {:.2}s from {:?}\n  why: {}",
        plan.length, plan.ranges, plan.explanation
    );
    let short = shorter::render(&v, &plan.ranges).await?;
    println!(
        "short video {} = {:.2}s, title {:?}",
        short.id, short.duration, short.title
    );

    captions::translate(&id, "hi").await?;
    let vtt = std::fs::read_to_string(captions::track_path(&id, "hi"))?;
    let hi = captions::parse(&vtt);
    let en = library::cues(&t.words);
    let same_timing = hi.len() == en.len()
        && hi
            .iter()
            .zip(&en)
            .all(|(a, b)| (a.0 - b.0).abs() < 0.002 && (a.1 - b.1).abs() < 0.002);
    println!(
        "hindi track: {} cues, timings identical: {same_timing}\n{}",
        hi.len(),
        vtt.lines().take(6).collect::<Vec<_>>().join("\n")
    );
    Ok(())
}
