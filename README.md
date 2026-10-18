# Recorder

Record your screen and camera, get a link to share, and let Claude do the boring parts: titles, summaries, chapters, cutting out the ums and long pauses, and answering viewers' questions about the video. Everything runs on your computer. Your videos never go to a server, transcripts are made locally with Whisper, and the AI uses your own Claude Code login or API key.

<!-- GIF placeholder: docs/demo.gif (record, stop, share link, ask the video) -->

## What it does

- **Record** a display with your voice, plus a round camera bubble. Countdown, pause and resume, stop from the controls bar, the menu bar icon or **Alt+Shift+R**.
- **Library and player** with rename, delete, trim, captions, chapters and a clickable transcript.
- **Local transcripts** with word timings (Whisper), captions in the player, SRT and VTT export, and an MP4 with captions.
- **Claude writes** the title, summary and chapters from the transcript.
- **Remove ums and silences**: preview every cut, then apply. The original is kept so you can restore it.
- **Ask the video**: questions answered from the transcript, with timestamps that jump the player.
- **Share**: every video has a watch page. One click turns it into a public link through a free Cloudflare quick tunnel. The link works while the app is running. Viewer questions are off unless you turn them on for that video.

## Install (macOS, from source)

```bash
brew install ffmpeg whisper-cpp cloudflared   # whisper-cpp and cloudflared are optional
npm install
npm run doctor        # checks Node, Rust, ffmpeg and the optional tools
npm run tauri dev     # or: npm run build
```

The first time you record, macOS asks for Screen Recording, Microphone and Camera permission. If a recording does not start, open Settings in the app and use the permission buttons, then try again. The speech model (about 150 MB) downloads the first time you transcribe.

## How it uses Claude

Pick one in Settings:

1. **Your Claude Code login.** The app runs your installed `claude` command in headless mode (`claude -p`). It works with whatever account you already logged into. The app never sees, stores or forwards that login.
2. **Your API key.** Set `ANTHROPIC_API_KEY` or paste a key in Settings (saved only on this computer, readable only by your user).

Only the transcript text and your question are sent to Claude. The video and audio stay on your computer.

## Renaming the app

The name, bundle identifier, tagline, accent colour and repo link live in one file: [`brand.json`](brand.json). Change it and run `npm run brand` (the dev and build scripts do this for you). `npm run check:brand` fails if the name is hardcoded anywhere else.

## Development

```bash
npm run typecheck && npm run lint
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cargo run --example smoke -- --record 10      # record, transcribe, ask Claude, serve, without the UI
cargo run --example smoke -- --file demo.mp4  # same, on an existing video
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/FEATURES.md](docs/FEATURES.md) and [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT
