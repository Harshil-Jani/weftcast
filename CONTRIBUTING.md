# Contributing

Thanks for helping. Keep each pull request focused on one change, and open an issue first for anything large.

## Setup

```bash
npm install
npm run doctor
npm run tauri dev
```

## Before you open a pull request

```bash
npm run typecheck && npm run lint && npm run check:brand
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## Ground rules

- **Never hardcode the app name.** Read it from `brand.json` (`BRAND` in Rust, `brand` in the UI). `npm run check:brand` enforces this.
- **Clean room.** Build features from public product descriptions. Do not decompile or copy code from other recorders, and do not paste code from projects under licenses that conflict with MIT.
- **No login handling.** The app must never read, store or forward anyone's Claude login. Use the `claude` CLI or an API key the user provides.
- **Local first.** Video and audio stay on the user's computer. Only transcript text and questions go to Claude.
- **Add a test** when you change the cut planner, the chapter validator, caption formatting or the brand check.

## Good first areas

- Windows and Linux capture in `src-tauri/src/capture.rs` (see ARCHITECTURE.md).
- Bundling `whisper-cli` and `ffmpeg` as Tauri sidecars for one-click installs.
- Single-window recording through ScreenCaptureKit.
