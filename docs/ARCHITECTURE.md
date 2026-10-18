# Architecture

A Tauri v2 app: a Rust core that does the work, and a React UI in three windows (library, recording controls, camera bubble). A small local web server serves media to the UI and the watch page to viewers.

```
brand.json ──> scripts/brand.mjs ──> tauri.conf.json, package.json name, README title
                                └──> Rust (include_str!) and UI (import) read it directly

UI (React)  ──invoke──>  commands.rs
                            ├── capture.rs     bundled ffmpeg process per segment (avfoundation)
                            ├── media.rs       probe, trim, cut render, captions, thumbnails
                            ├── transcribe.rs  whisper.cpp compiled in (whisper-rs, Metal), chunked
                            ├── edits.rs       edit plans: validate, diff, apply, undo
                            ├── shorter.rs     sentence-level "make it shorter"
                            ├── captions.rs    caption translation with kept timings
                            ├── shorts.rs      9:16 shorts: clip picking, framing, ASS captions
                            ├── importer.rs    file and YouTube import (bundled yt-dlp)
                            ├── agent.rs       MCP + CLI tools (cli crate)
                            ├── cuts.rs        filler + silence cut planner (pure, tested)
                            ├── ai.rs          Claude: CLI or API provider, schemas, validation
                            ├── library.rs     folders, meta.json, transcript.json, SRT/VTT
                            ├── server.rs      axum: /v, /media, /captions, /api (localhost)
                            └── share.rs       cloudflared quick tunnel for public links
```

## Decisions

**Capture with an ffmpeg process, not a capture crate.** ffmpeg's avfoundation input records the screen and microphone in one process, encodes with the hardware H.264 encoder (VideoToolbox) and writes a standard MP4. It is the same tool the app already needs for trimming, cutting and captions, so there is one dependency instead of two. Windows (gdigrab or ddagrab plus dshow) and Linux (x11grab or kmsgrab plus PulseAudio) only need a different `input_args` in `capture.rs`. The trade-off: avfoundation records whole displays, so single-window recording is on the LATER list and would use ScreenCaptureKit directly.

**Pause and resume as segments.** ffmpeg cannot pause, so pause stops the current segment and resume starts a new one. Stop joins the segments with the concat demuxer (no re-encode).

**Camera as a separate window.** The bubble is a round, always-on-top webview using getUserMedia, so it is captured with the screen and the user can drag it anywhere. The camera only runs while recording.

**Everything bundled.** ffmpeg and ffprobe (a static LGPL build with libass, built by `scripts/build-ffmpeg.sh` from pinned sources), cloudflared, yt-dlp and the agent CLI ship as Tauri sidecars in `Contents/MacOS`. `media::tool` finds them: a developer override env var, then a refreshed yt-dlp in app data, then the bundled copy; dev builds also look in `src-tauri/binaries` and PATH. Release builds never look at Homebrew.

**Transcripts compiled in.** whisper.cpp runs inside the app through whisper-rs with Metal. Audio is decoded to 16 kHz samples by ffmpeg and processed in 10 minute chunks, so a 3 hour video does not need gigabytes of memory. One segment per word (`max_len 1`, `split_on_word`) drives captions, filler detection and Claude's timestamps. The model downloads once into app data and is checked against a pinned sha256.

**Cuts.** `cuts.rs` turns filler words (from the transcript) and long silences (from ffmpeg `silencedetect`) into keep ranges. `media::render_keep` cuts video and audio with `trim`/`atrim` and joins them with `concat`, so both streams stay in sync. Edits keep `original.mp4` so they can be undone, and clear the transcript because the timings no longer match.

**Claude.** One function, `ai::ask_json`, with two providers:
- `Cli` runs `claude -p --output-format json` and validates the JSON it returns.
- `Api` calls the Messages API with structured output (`output_config.format`), `claude-opus-5-5`, and the default server-side refusal fallback.

Every response is validated: chapters are sorted, kept inside the video, spaced at least 5 seconds apart and start at 0.

**Sharing and safety.**
- The server only listens on 127.0.0.1. Every URL carries a random 20-character share id, so a public tunnel exposes only videos someone was given a link to.
- Media and caption routes allow cross-origin GET (the app window loads them). Asking questions is POST with JSON, which other websites cannot send cross-origin.
- Requests through Cloudflare (`cf-connecting-ip`) can ask questions only when the owner turns it on for that video, and are limited to 20 an hour per video.

## Library layout

```
~/Movies/<name>/<id>/video.mp4  original.mp4 (after an edit)  thumb.jpg  meta.json  transcript.json
~/Movies/<name>/.settings.json   (provider, optional API key, mode 600)
~/Library/Caches/<slug>/models/ggml-base.en.bin
```
