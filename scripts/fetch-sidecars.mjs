// Prepares every bundled helper for one or more targets, verified against pinned checksums.
//   node scripts/fetch-sidecars.mjs                      -> this Mac's architecture
//   node scripts/fetch-sidecars.mjs universal-apple-darwin
// Output: src-tauri/binaries/<name>-<target> (gitignored). See THIRD_PARTY_NOTICES.md for licenses.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "src-tauri", "binaries");
const cache = join(root, ".sidecar-cache", "downloads");
mkdirSync(out, { recursive: true });
mkdirSync(cache, { recursive: true });

// Pinned releases. Bump the version, URL and sha256 together, and update THIRD_PARTY_NOTICES.md.
const PINS = {
  "yt-dlp": {
    version: "2026.08.19",
    // One universal (arm64 + x86_64) binary for macOS.
    "aarch64-apple-darwin": { url: "https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp_macos", sha256: "0f192b7ec147ab6288885d6351d9ab67367640029b4377576ef46dd79cf7b202" },
    "x86_64-apple-darwin": { url: "https://github.com/yt-dlp/yt-dlp/releases/download/2026.08.19/yt-dlp_macos", sha256: "0f192b7ec147ab6288885d6351d9ab67367640029b4377576ef46dd79cf7b202" },
    "x86_64-pc-windows-msvc": null, // TODO: yt-dlp.exe
    "x86_64-unknown-linux-gnu": null, // TODO: yt-dlp_linux
  },
  cloudflared: {
    version: "2026.10.1",
    "aarch64-apple-darwin": { url: "https://github.com/cloudflare/cloudflared/releases/download/2026.10.1/cloudflared-darwin-arm64.tgz", sha256: "647615df99d49cd179ea5e13ef760b9bbfb33630f6ae62f87e33dfadda60adcc", tgz: "cloudflared" },
    "x86_64-apple-darwin": { url: "https://github.com/cloudflare/cloudflared/releases/download/2026.10.1/cloudflared-darwin-amd64.tgz", sha256: "3b0dbaf378337e9d7fb6cd235a76d1d6a7dee434c2d8edf9281164fb6a3837ab", tgz: "cloudflared" },
    "x86_64-pc-windows-msvc": null, // TODO: cloudflared-windows-amd64.exe (sha256 4d2490e7...)
    "x86_64-unknown-linux-gnu": null, // TODO: cloudflared-linux-amd64 (sha256 3bfc539f...)
  },
};
const MAC = ["aarch64-apple-darwin", "x86_64-apple-darwin"];

const sha256 = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");
const run = (cmd, args, opts = {}) => execFileSync(cmd, args, { stdio: "inherit", cwd: root, ...opts });

async function download(url, expected) {
  const file = join(cache, `${expected.slice(0, 12)}-${url.split("/").pop()}`);
  if (!existsSync(file) || sha256(file) !== expected) {
    process.stdout.write(`downloading ${url}\n`);
    const res = await fetch(url);
    if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
    writeFileSync(file, Buffer.from(await res.arrayBuffer()));
  }
  const got = sha256(file);
  if (got !== expected) throw new Error(`checksum mismatch for ${url}\n  expected ${expected}\n  got      ${got}`);
  return file;
}

async function fetchPinned(name, target) {
  const pin = PINS[name][target];
  if (!pin) throw new Error(`${name} for ${target} is not set up yet (see PINS in this script)`);
  const file = await download(pin.url, pin.sha256);
  const dest = join(out, `${name}-${target}`);
  if (pin.tgz) {
    const dir = join(cache, `x-${pin.sha256.slice(0, 12)}`);
    mkdirSync(dir, { recursive: true });
    run("tar", ["-xzf", file, "-C", dir]);
    copyFileSync(join(dir, pin.tgz), dest);
  } else {
    copyFileSync(file, dest);
  }
  chmodSync(dest, 0o755);
}

async function prepare(target) {
  if (!MAC.includes(target)) throw new Error(`${target}: only macOS targets are wired up so far`);
  for (const name of Object.keys(PINS)) await fetchPinned(name, target);
  if (!existsSync(join(out, `ffmpeg-${target}`)) || !existsSync(join(out, `ffprobe-${target}`))) {
    run("bash", ["scripts/build-ffmpeg.sh", target]);
  }
  run("cargo", ["build", "--release", "-p", "cli", "--target", target], { cwd: join(root, "src-tauri") });
  copyFileSync(join(root, "src-tauri", "target", target, "release", "cli"), join(out, `cli-${target}`));
  console.log(`sidecars ready for ${target}`);
}

const host = execFileSync("uname", ["-m"]).toString().trim() === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
const target = process.argv[2] || host;
if (target === "universal-apple-darwin") {
  for (const t of MAC) await prepare(t);
  for (const name of ["ffmpeg", "ffprobe", "cloudflared", "cli"]) {
    run("lipo", ["-create", ...MAC.map((t) => join(out, `${name}-${t}`)), "-output", join(out, `${name}-universal-apple-darwin`)]);
  }
  copyFileSync(join(out, "yt-dlp-aarch64-apple-darwin"), join(out, "yt-dlp-universal-apple-darwin"));
  console.log("universal sidecars ready");
} else {
  await prepare(target);
}
