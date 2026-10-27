<p align="center"><img src="docs/media/banner.png" alt="Weftcast: the open-source Loom for Mac" width="100%"></p>

<p align="center">
<a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-6d5efc.svg" alt="MIT license"></a>
<img src="https://img.shields.io/badge/macOS-Apple%20Silicon%20%7C%20Intel-black?logo=apple" alt="macOS">
<img src="https://img.shields.io/badge/built%20with-Tauri%202%20%2B%20Rust-24C8DB?logo=tauri" alt="Tauri and Rust">
<img src="https://img.shields.io/badge/transcripts-local%20Whisper-2ea44f" alt="Local Whisper">
<img src="https://img.shields.io/badge/MCP-server%20included-orange" alt="MCP server">
<a href="https://github.com/Harshil-Jani/weftcast/releases/latest"><img src="https://img.shields.io/github/v/release/Harshil-Jani/weftcast?color=6d5efc" alt="Latest release"></a>
</p>

<h3 align="center">Record your screen. Get a link. Let Claude do the editing.<br>Your videos never leave your Mac.</h3>

<p align="center">
<a href="https://github.com/Harshil-Jani/weftcast/releases/latest"><b>Download for Mac</b></a> ·
<a href="#see-it-work">See it work</a> ·
<a href="#weftcast-vs-loom-vs-obs-vs-cap">Compare</a> ·
<a href="#how-share-links-work">How share links work</a>
</p>

<p align="center"><a href="https://cdn.jsdelivr.net/gh/Harshil-Jani/weftcast@f31a5887d94b036ea6614b5303574f3321f0550d/docs/media/pitch.mp4"><img src="docs/media/pitch-poster.jpg" alt="Watch the 56-second pitch, recorded with Weftcast" width="80%"></a><br>
<sub>The 56-second pitch above was recorded, cleaned up and titled in Weftcast. Click it to play with sound.</sub></p>

## The idea

A loom has two sets of threads. The **warp** is stretched tight on the frame and never moves. The **weft** is the thread you pass through it, row after row, and it is the part that makes the cloth yours.

Screen recorders today are all warp: someone else's frame, someone else's server, someone else's limits on how long you can talk. Weftcast is the weft. It keeps the thing you made on your own machine, in plain MP4 files in a folder you can open, and brings the AI to your video instead of sending your video to the AI.

Four rules the app is built on:

1. **Local first.** Recording, transcription (whisper.cpp), editing and the watch page all run on your Mac. Only transcript text goes to Claude, and only when you press an AI button.
2. **Preview before anything changes.** Every AI edit shows up as a readable plan first. You apply it, and you can undo it.
3. **Fast path first.** Shortcut, talk, stop, link. Everything else waits in an inspector you can ignore.
4. **Your Claude, not ours.** The AI runs on your own Claude Code login or API key. Weftcast has no account, no server and no subscription.

## See it work

Every clip below is a real screen capture of the app, run on the pitch video above. Nothing is mocked; slow AI steps are sped up.

### Record

A floating pill picks the screen, mic and camera. **Option+Shift+R** starts and stops from any app. The camera is a round bubble you can drag and resize.

<img src="docs/media/record.gif" alt="Recording with the camera bubble" width="100%">

### Transcribe on your Mac

whisper.cpp runs on your Mac with Metal acceleration, with word timings, captions in the player and SRT/VTT export. Nothing is uploaded.

<img src="docs/media/transcribe.gif" alt="Local transcription" width="100%">

### Title, summary and chapters, written by Claude

The speech model heard "lume". Claude knew it was Loom.

<img src="docs/media/chapters.gif" alt="Claude writes the title, summary and chapters" width="100%">

### Remove the ums and long pauses

Preview every cut, then apply. Here: 9 pauses, 1:03 down to 0:56. The original stays on disk so you can restore it.

<img src="docs/media/remove-pauses.gif" alt="Removing pauses" width="100%">

### Edit by typing

*"Cut the part about Loom pricing and make my voice louder."* Claude shows a plan with the exact cut, the gain and the new length, and explains the trade-off. Apply, or Discard. Undo is one click.

<img src="docs/media/edit-by-prompt.gif" alt="Editing the video with a prompt" width="100%">

### Make it shorter

Pick 30 seconds, a minute or any length. Claude keeps the sentences that carry the point and saves the result as a new video next to the original.

<img src="docs/media/make-shorter.gif" alt="Make it shorter" width="100%">

### Ask the video

Questions are answered from the transcript, with timestamps that jump the player. When the transcript does not say something, the answer says so.

<img src="docs/media/ask-the-video.gif" alt="Ask the video" width="100%">

### Translate captions

Line by line, with the original timings, plus a language picker in the player and on the watch page.

<img src="docs/media/translate-captions.gif" alt="Hindi captions" width="100%">

### Fix a quiet mic

Auto level brings your voice to -16 LUFS. This recording went from -42.9 LUFS to -16.

<img src="docs/media/voice-level.gif" alt="Voice auto level" width="100%">

### Shorts from a long video

Claude picks moments that stand on their own, with a hook title and a score for each. You re-roll, nudge and render 1080x1920 clips with word-highlighted captions.

<img src="docs/media/shorts.gif" alt="Shorts suggestions" width="100%">

### Share a link

Turn on **Public link** in the Share sheet and you get an `https://...trycloudflare.com` link that opens on any browser or phone. The watch page below was opened through a real public link of this video, which was turned off again after the screenshot. [How that works, and its limits](#how-share-links-work).

<table><tr>
<td width="70%"><img src="docs/media/watch-page.png" alt="Watch page on desktop"></td>
<td width="30%"><img src="docs/media/watch-phone.png" alt="Watch page on a phone"></td>
</tr></table>

## Screenshots

<table>
<tr><td><img src="docs/media/library.png" alt="Library"><br><sub>Library</sub></td><td><img src="docs/media/chapters.png" alt="Title, summary and chapters"><br><sub>Claude's title, summary and chapters</sub></td></tr>
<tr><td><img src="docs/media/edit-by-prompt.png" alt="Edit plan"><br><sub>An edit plan, before anything changes</sub></td><td><img src="docs/media/remove-pauses.png" alt="Pause cuts"><br><sub>Every cut listed before you apply</sub></td></tr>
<tr><td><img src="docs/media/make-shorter.png" alt="Make it shorter"><br><sub>Make it shorter: 0:56 to 0:28</sub></td><td><img src="docs/media/ask-the-video.png" alt="Ask the video"><br><sub>Ask the video</sub></td></tr>
<tr><td><img src="docs/media/hindi-captions.png" alt="Translated captions"><br><sub>Hindi captions, same timings</sub></td><td><img src="docs/media/voice-level.png" alt="Voice level"><br><sub>Voice level with a loudness readout</sub></td></tr>
<tr><td><img src="docs/media/shorts.png" alt="Shorts"><br><sub>Shorts, scored and titled by Claude</sub></td><td><img src="docs/media/settings.png" alt="Settings"><br><sub>Settings: your Claude Code login or API key</sub></td></tr>
</table>

## How share links work

This is the part people ask about first, so here it is plainly.

- **Every video has a watch page**, served by a small web server inside the app.
- **Public link off** (the default): the link is `http://127.0.0.1:...`, which only opens on your own Mac. Good for checking the page before you send it.
- **Public link on**: Weftcast starts a free [Cloudflare quick tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/do-more-with-tunnels/trycloudflare/) with the bundled `cloudflared` and gives you an `https://<random-words>.trycloudflare.com/v/<id>` link. Anyone with that link can watch on any browser or phone. No Cloudflare account, no sign-up, no port forwarding.
- **Nothing is uploaded or stored anywhere.** The video streams from your Mac through the tunnel while someone watches.
- **The link works while Weftcast is running.** Quit the app or turn the link off and it stops. Each video has its own unguessable id, so the link reaches that video and nothing else in your library.
- **Viewer questions** are off by default on public links. Turn them on per video and they are answered with your Claude.
- **Need a link that stays up when your Mac sleeps?** Export the MP4 (with captions burned in if you like) and put it wherever you host files. A permanent, self-hosted link is on the roadmap.

## Weftcast vs Loom vs OBS vs Cap

Prices and limits checked October 2026 from each product's public pages and reviews; they change, so check before you rely on them. "Not listed" means we could not find it in their public material, not that it is missing.

| | **Weftcast** | **Loom** | **OBS Studio** | **Cap** |
|---|---|---|---|---|
| Price | Free (MIT) | Free tier; Business about $18 and Business + AI about $24 per user/month | Free (GPL) | Free for personal use; Pro from about $8 per user/month |
| Open source | Yes, MIT | No | Yes, GPL | Yes, AGPL |
| Where videos live | Your Mac | Loom's cloud | Your computer | Cap's cloud or your own S3 |
| Free recording limit | None | 25 videos, 5 min each | None | 5 min shareable links |
| Share link | Yes, a public HTTPS link served from your Mac while the app runs | Yes, cloud | No | Yes, cloud |
| Camera bubble | Yes | Yes | Manual scene setup | Yes |
| Transcripts and captions | Local Whisper, SRT/VTT, burned-in MP4 | Cloud | Plugins only | Cloud AI captions |
| Translated captions | Yes | Not listed | No | Not listed |
| AI titles, summary, chapters | Yes (your Claude) | Paid AI plan | No | Pro plan |
| Remove ums and silences | Yes, preview every cut | Paid AI plan | No | Not listed |
| Edit with a prompt | Yes, reviewable plan with undo | Edit by transcript | No | Not listed |
| Make it shorter (30s, 60s, any length) | Yes, saved as a new video | No | No | Not listed |
| Viewers ask the video | Yes, answers jump to timestamps | No | No | Not listed |
| Vertical shorts with captions | Yes | No | No | Not listed |
| Phone as camera, second angle, remote or screen | Yes, up to 4 phones | No | Via plugins | No |
| MCP server for AI agents | Yes | No | No | No |
| Live streaming | No | No | Yes | No |
| Platforms | macOS | Mac, Windows, web, mobile | Mac, Windows, Linux | Mac, Windows |

**Pick Loom** if your team lives in a shared cloud library with comments and admin controls. **Pick OBS** for live streaming and complex scenes. **Pick Cap** if you want an open-source, Loom-style cloud on Windows too. **Pick Weftcast** if you want unlimited recording that never leaves your Mac and an AI that actually edits the video for you.

### Honest limits

- macOS only for now (Apple Silicon and Intel).
- Share links work while the app is running, because the video is served from your Mac (see [How share links work](#how-share-links-work)).
- AI features need your own Claude Code login or Anthropic API key. Everything else works without one.
- Builds are not yet signed with an Apple Developer ID, so the first open needs a right-click > Open (see Install).

## Everything it does

<details><summary>The full feature list, including phones as cameras, remotes and screens</summary>


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

</details>

## Install

Download the `.dmg`, drag the app to Applications and open it. Everything it needs is inside: the recording and editing engine (FFmpeg, LGPL), speech recognition (whisper.cpp, compiled in), public links (cloudflared) and the YouTube importer (yt-dlp). No Homebrew, no terminal. The speech model (about 150 MB) downloads with a progress bar the first time you transcribe, or you can install it from a file in Settings.

**First open on an unsigned build.** Until the app is signed with an Apple Developer ID and notarized, macOS says it "can't be opened because Apple cannot check it". Right-click the app, choose **Open**, then **Open** again. Or run once in Terminal:

<!-- brand:xattr -->
```bash
xattr -dr com.apple.quarantine "/Applications/Weftcast.app"
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
claude mcp add weftcast -- "$PWD/target/release/cli" mcp
```

Then ask Claude Code things like "list my Weftcast videos" or "cut the first 10 seconds of my latest recording".
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
