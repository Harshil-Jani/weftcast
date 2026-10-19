# Third-party notices

This app is MIT licensed. It ships the programs below inside the app bundle, each under its own license. Only FFmpeg is modified (one patch, described below); the others are the publishers' release binaries.

## FFmpeg 8.0.1 (ffmpeg, ffprobe)

- License: GNU Lesser General Public License, version 2.1 or later (LGPL). Built **without** `--enable-gpl` and without `--enable-nonfree`; no GPL components such as libx264 are included. Video is encoded with Apple's VideoToolbox, audio with FFmpeg's native AAC encoder.
- Source: https://ffmpeg.org/releases/ffmpeg-8.0.1.tar.xz (sha256 `05ee0b03119b45c0bdb4df654b96802e909e0a752f72e4fe3794f487229e5a41`)
- **Modification:** `scripts/patches/ffmpeg-ios-screen-devices.patch` (applied by the build script) sets CoreMediaIO's `kCMIOHardwarePropertyAllowScreenCaptureDevices` in the avfoundation input, so a USB-connected iPhone or iPad screen can be captured, as QuickTime Player does. The patch is part of this repository under the same LGPL terms.
- Build script and configure flags: `scripts/build-ffmpeg.sh` in this repository. You may rebuild and replace the bundled ffmpeg with your own build of the same or a modified source (it lives at `Contents/MacOS/ffmpeg` inside the app).
- Statically linked libraries:
  - libass 0.17.4, ISC license: https://github.com/libass/libass/releases/download/0.17.4/libass-0.17.4.tar.xz
  - FreeType 2.14.1, FreeType License (FTL): https://download.savannah.gnu.org/releases/freetype/freetype-2.14.1.tar.xz
  - GNU FriBidi 1.0.16, LGPL 2.1 or later: https://github.com/fribidi/fribidi/releases/download/v1.0.16/fribidi-1.0.16.tar.xz
  - HarfBuzz 12.1.0, MIT ("Old MIT"): https://github.com/harfbuzz/harfbuzz/releases/download/12.1.0/harfbuzz-12.1.0.tar.xz

  Checksums for every source are pinned in `scripts/build-ffmpeg.sh`.

## whisper.cpp (compiled into the app through whisper-rs)

- License: MIT. https://github.com/ggml-org/whisper.cpp
- Speech models (ggml base.en / small.en) are downloaded from https://huggingface.co/ggerganov/whisper.cpp on first use and checked against pinned sha256 values. MIT license.

## yt-dlp 2026.08.19

- License: The Unlicense (public domain). https://github.com/yt-dlp/yt-dlp
- Binary: https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp_macos (sha256 `0f192b7ec147ab6288885d6351d9ab67367640029b4377576ef46dd79cf7b202`). The macOS build bundles a Python runtime under the PSF license.
- "Update YouTube importer" may replace it with a newer release, verified against that release's SHA2-256SUMS.

## cloudflared 2026.10.1

- License: Apache License 2.0. https://github.com/cloudflare/cloudflared
- Binaries: `cloudflared-darwin-arm64.tgz` (sha256 `647615df99d49cd179ea5e13ef760b9bbfb33630f6ae62f87e33dfadda60adcc`) and `cloudflared-darwin-amd64.tgz` (sha256 `3b0dbaf378337e9d7fb6cd235a76d1d6a7dee434c2d8edf9281164fb6a3837ab`) from https://github.com/cloudflare/cloudflared/releases/tag/2026.10.1

## scrcpy 4.1 (Android screen recording)

- License: Apache License 2.0. https://github.com/Genymobile/scrcpy (Copyright Genymobile and Romain Vimont)
- Binaries: `scrcpy-macos-aarch64-v4.1.tar.gz` (sha256 `20fd47c9014dd5e0fa77091f3cb7adbda8445a360c4584aeaa0150b5b3988ff3`) and `scrcpy-macos-x86_64-v4.1.tar.gz` (sha256 `ee2a7223bc8dbdc4f482db1134bcf441178dafb833492b71ca4c22090c58ce72`), joined into one universal binary. The phone-side server `scrcpy-server-v4.1` (sha256 `deacb991ed2509715160ffdc7907e47b4160eb30d1566217e9047fd5b8850cae`) ships as a resource.
- The scrcpy release binary statically includes FFmpeg (LGPL 2.1 or later), SDL (zlib license) and libusb (LGPL 2.1 or later), as built by the scrcpy project from its published build scripts: https://github.com/Genymobile/scrcpy/tree/v4.1/app/deps
- The `adb` program in the scrcpy archive is **not** included.

## Android platform-tools (adb): not bundled

- adb is distributed by Google under the Android Software Development Kit License Agreement (https://developer.android.com/studio/terms), which does not allow redistribution. The app does not ship it. When you choose Get Android Tools, it shows those terms, and only after you agree downloads `platform-tools_r37.0.1-darwin.zip` from Google (sha256 `ee39ad5967e95c2a07f04dbcbde96b1a0c916ba376096db5d2f498b7727a5d1d`, checked before use) into the app's data folder. An adb you already have (Android Studio, Homebrew) also works.

## Rust and JavaScript dependencies

Compiled-in crates and npm packages are under MIT, Apache-2.0, ISC, BSD or Zlib licenses. Run `cargo tree` and `npm ls` for the full list.
