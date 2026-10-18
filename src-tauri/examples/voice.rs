//! Loudness end to end: `cargo run --example voice -- <input.mp4>`.
//! Imports the file, then checks a +6 dB gain moves the level by about 6 dB and that
//! Auto level lands within 1 LU of -16 LUFS with a true peak at or under -1.5 dBTP.
use app_lib::{capture, library, voice};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let src = std::env::args().nth(1).expect("input video");
    let id = library::new_id();
    std::fs::create_dir_all(library::dir(&id))?;
    let seg = library::dir(&id).join("segment-0.mp4");
    std::fs::copy(&src, &seg)?;
    let mut v = capture::finish(&id, &[seg]).await?;
    v.voice = voice::Voice::default();
    library::save(&v)?;
    let _ = std::fs::remove_file(library::voiced_path(&id));

    let raw = voice::analyze(&library::video_path(&id), None, None).await?;
    println!(
        "raw:        {:>6.2} LUFS  peak {:>6.2} dBTP",
        raw.lufs(),
        raw.true_peak()
    );

    let gain = voice::Voice {
        gain_db: 6.0,
        auto: false,
    };
    voice::render(&id, &gain).await?;
    let g = voice::analyze(&library::voiced_path(&id), None, None).await?;
    println!(
        "gain +6 dB: {:>6.2} LUFS  peak {:>6.2} dBTP  (delta {:+.2} dB)",
        g.lufs(),
        g.true_peak(),
        g.lufs() - raw.lufs()
    );
    assert!(
        (g.lufs() - raw.lufs() - 6.0).abs() < 0.5,
        "gain should move the level by 6 dB"
    );

    let auto = voice::Voice {
        gain_db: 0.0,
        auto: true,
    };
    voice::render(&id, &auto).await?;
    let a = voice::analyze(&library::voiced_path(&id), None, None).await?;
    println!(
        "auto level: {:>6.2} LUFS  peak {:>6.2} dBTP",
        a.lufs(),
        a.true_peak()
    );
    assert!(
        (a.lufs() - voice::TARGET_LUFS).abs() <= 1.0,
        "auto level should land within 1 LU of -16"
    );
    assert!(
        a.true_peak() <= voice::TARGET_TP + 0.3,
        "true peak should stay near or under -1.5"
    );

    let s = voice::stats(
        &id,
        &voice::Voice {
            gain_db: 12.0,
            auto: false,
        },
    )
    .await?;
    println!(
        "clip warning at +12 dB gain: {} (predicted peak {:.2})",
        s.clips, s.predicted_peak
    );
    println!(
        "video stream copied: {}",
        app_lib::media::run(
            "ffprobe",
            &[
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=codec_name",
                "-of",
                "csv=p=0",
                &library::voiced_path(&id).to_string_lossy()
            ]
        )
        .await?
        .trim()
    );
    library::delete(&id)?;
    println!("ok");
    Ok(())
}
