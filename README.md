# Recorder

Record your screen and camera, get a link to share, and let Claude do the boring parts: titles, summaries, chapters, cutting out the ums and long pauses, and answering viewers' questions about the video. Everything runs on your computer. Your videos never go to a server, transcripts are made locally with Whisper, and the AI uses your own Claude Code login or API key.

<!-- GIF placeholder: docs/demo.gif (record, stop, share link, ask the video) -->

## What it does

- **Record** a display with your voice, plus a round camera bubble (small, medium or large, drag it anywhere). A floating pill picks the screen, microphone and camera; a large countdown, then pause, resume, stop or discard from the pill, the menu bar icon or **Option+Shift+R**.
- **Library and player** with rename, delete, trim, captions, chapters and a clickable transcript.
- **Local transcripts** with word timings (whisper.cpp, compiled in, Metal accelerated), captions in the player, SRT and VTT export, and an MP4 with captions.
- **Claude writes** the title, summary and chapters from the transcript.
- **Remove ums and silences**: preview every cut, then apply. The original is kept so you can restore it.
- **Ask the video**: questions answered from the transcript, with timestamps that jump the player.
- **Edit with a prompt**: "cut the part about pricing", "speed up the setup 2x", "make my voice louder". Claude proposes an edit plan, you see it as a readable list of changes, then Apply or Discard. Undo restores the previous version.
- **Make it shorter**: pick 30s, 60s, 2 min or any length. Claude keeps the sentences that carry the point and the result is saved as a new video next to the original.
- **Translate captions** into another language, line by line with the original timings, with a language picker in the player and on the watch page.
- **Voice level**: a gain slider you hear live, and Auto level that evens out your voice to -16 LUFS, with a loudness readout and a clipping warning.
- **Shorts**: turn one long video into vertical 9:16 shorts. Import a file or a YouTube link (only videos you own or have the rights to; downloading other people's videos breaks YouTube's terms), let Claude pick self-contained moments with hook titles and virality scores, nudge and re-roll them, then render 1080x1920 clips with big word-highlighted captions, a blurred-background, center-crop or screen-and-camera layout, and your voice level.
- **Connect your phone** over Wi-Fi (the Connect Phone button next to New Recording, the Phone item in the sidebar, File > Connect Phone or **Shift+Command+P**, or "Use your phone" in the recording controls):
  - **In one tap on an iPhone**: "Send to my iPhone" AirDrops the link, and the phone opens it from the notification. Any phone can scan the QR code instead, or open the short link and type the 6-digit code.
  - **Phone as your face cam, a second angle, or a remote**: up to four phones at once, each with its own job, chosen on the phone or on the Mac (picking a job another phone has swaps them). The camera streams to the bubble over your Wi-Fi (720p at 30 fps) and is recorded as its own track; a second angle (hands, a whiteboard, a product) can be an inset, a split or cut-ins. Every phone has a Remote tab with Start, Pause, Stop and Discard, the timer and your microphone level; a phone set to Remote turns its camera off.
  - **Phone screen next to yours**, for app demos. An iPhone over Wi-Fi: mirror it to this Mac with AirPlay (Control Center, Screen Mirroring, pick this Mac; needs AirPlay Receiver on) and its window is recorded. An Android phone over Wi-Fi (Android 11 and later): pair once with the code from Wireless debugging, then connect. A cable gives the best quality: an iPhone over USB shows up as a capture device, as in QuickTime; an Android phone over USB uses the bundled scrcpy. Android needs Google's adb, which the app downloads once after you agree to Google's terms. The phone screen is its own file in a clean device frame (right, left, floating or split, at S, M or L), composited on share or export, so the layout can change later in the Edit inspector.
  - **Remembered phones**: after the first time, add the page to the phone's Home Screen and open it from there to reconnect. Settings > Phones lists them, with Forget. A phone that closes the page drops out after 20 seconds.
  - **Prompt edits**: "show my phone on the right from 0:20 to 1:10" (`show_phone`) and "cut to the second angle while I sketch" (`switch_angle`).
  - **How pairing works**: the code is a one-time token. The page is served through the app's temporary public link, so the phone gets a valid HTTPS certificate and shows no warning (phones only allow the camera on a secure page). Only the connection setup goes that way; the video goes straight from the phone to the Mac on your Wi-Fi. Offline, the page is served on your local network with a certificate this Mac makes for itself, and the phone asks you to trust it once.
- **Share**: every video has a watch page. One click turns it into a public link through a free Cloudflare quick tunnel. The link works while the app is running. Viewer questions are off unless you turn them on for that video.

## Install

Download the `.dmg`, drag the app to Applications and open it. Everything it needs is inside: the recording and editing engine (FFmpeg, LGPL), speech recognition (whisper.cpp, compiled in), public links (cloudflared) and the YouTube importer (yt-dlp). No Homebrew, no terminal. The speech model (about 150 MB) downloads with a progress bar the first time you transcribe, or you can install it from a file in Settings.

**First open on an unsigned build.** Until the app is signed with an Apple Developer ID and notarized, macOS says it "can't be opened because Apple cannot check it". Right-click the app, choose **Open**, then **Open** again. Or run once in Terminal:

<!-- brand:xattr -->
```bash
xattr -dr com.apple.quarantine "/Applications/Recorder.app"
```
<!-- /brand:xattr -->

Signing and notarizing removes this step. It needs an Apple Developer account ($99 a year); see "Building" below.

The first time you record, macOS asks for Screen Recording, Microphone and Camera permission. If a recording does not start, open Settings in the app and use the permission buttons, then try again.

## Building

```bash
npm install
npm run doctor                                    # build tools: Rust, cmake, meson, ninja
node scripts/fetch-sidecars.mjs                   # this Mac; or: universal-apple-darwin
npm run tauri dev
npm run build -- --target universal-apple-darwin  # .app and .dmg for Apple Silicon and Intel
```

`fetch-sidecars.mjs` downloads pinned, checksum-verified yt-dlp and cloudflared, builds an LGPL-only FFmpeg with libass from pinned sources (`scripts/build-ffmpeg.sh`), and builds the command line tool. Binaries are not committed.

**Signing and notarization.** Set these before `npm run build` and the build signs and notarizes:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_TEAM_ID=TEAMID
export APPLE_ID=you@example.com APPLE_PASSWORD=app-specific-password
```

Without them the app is ad-hoc signed, which runs on Apple Silicon but shows the warning above.

**Developers** can point any helper at their own build with an environment variable named after the brand slug, for example `RECORDER_FFMPEG=/path/to/ffmpeg`.

## How it uses Claude

Pick one in Settings:

1. **Your Claude Code login.** The app runs your installed `claude` command in headless mode (`claude -p`) and checks `claude auth status` to see whether you are signed in. It never sees, stores or forwards that login.
2. **Your API key.** Paste it on first run or in Settings. It is stored in the macOS Keychain, never in a file. `ANTHROPIC_API_KEY` in the environment also works.

Without either, everything except the AI features works, and the AI buttons say what they need.

Only the transcript text and your question are sent to Claude. The video and audio stay on your computer.

## Use it from AI agents

The app ships a command line tool that is also an MCP server. Agents can list videos, read transcripts and summaries, and apply edit plans (the same validated plans the app shows you before rendering). Deleting a video and making a public link need an explicit `confirm`.

<!-- brand:mcp -->
```bash
cd src-tauri && cargo build --release -p cli
claude mcp add recorder -- "$PWD/target/release/cli" mcp
```

Then ask Claude Code things like "list my Recorder videos" or "cut the first 10 seconds of my latest recording".
The same tool works without an agent: `cli list`, `cli transcript ID`, `cli plan ID "remove the intro"`, `cli apply ID plan.json --preview`.
<!-- /brand:mcp -->

## Renaming the app

The name, bundle identifier, tagline, accent colour and repo link live in one file: [`brand.json`](brand.json). Change it and run `npm run brand` (the dev and build scripts do this for you). `npm run check:brand` fails if the name is hardcoded anywhere else.

## Keyboard shortcuts

The app follows the macOS conventions and has a full menu bar.

| Shortcut | Does |
|---|---|
| Command+N | New recording |
| Option+Shift+R | Start or stop recording from any app |
| Command+, | Settings |
| Command+O | Import a video (Shorts) |
| Command+E | Export the open video |
| Shift+Command+S | Share the open video |
| Command+1, 2, 3 | Library, Shorts, Shared |
| Control+Command+S | Show or hide the sidebar |
| Option+Command+I | Show or hide the inspector |
| Space or K | Play and pause |
| J and L (or the arrow keys) | Back and forward 5 seconds |

Right-click a video in the library for Open, Copy Link, Show in Finder, Make Shorts and Delete.

## Development

```bash
npm run typecheck && npm run lint
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cargo run --example smoke -- --record 10      # record, transcribe, ask Claude, serve, without the UI
cargo run --example smoke -- --file demo.mp4  # same, on an existing video
```

Two developer overrides (prefix = the brand slug in upper case) help with testing and screenshots:
`RECORDER_LIBRARY=/some/folder` uses another library (an empty folder shows the empty state), and
`RECORDER_APPEARANCE=light` or `dark` forces the window appearance without changing the system setting.
Design tokens live in `src/styles/tokens.css`; shared controls in `src/components/ui.tsx`.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/FEATURES.md](docs/FEATURES.md) and [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT. Bundled third-party programs keep their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
