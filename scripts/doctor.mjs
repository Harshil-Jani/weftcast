// Developer check: what you need to BUILD the app. Users never need any of this;
// the app bundles ffmpeg, cloudflared and yt-dlp, and compiles whisper in.
import { execSync } from "node:child_process";
import { existsSync } from "node:fs";

const has = (cmd) => { try { execSync(`command -v ${cmd}`, { stdio: "ignore", shell: "/bin/bash" }); return true; } catch { return false; } };
const target = process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
const checks = [
  ["node", "Node.js 20+", "https://nodejs.org", true],
  ["cargo", "Rust toolchain", "https://rustup.rs", true],
  ["cmake", "cmake (compiles whisper.cpp)", "brew install cmake", true],
  ["meson", "meson (builds ffmpeg's caption libraries)", "brew install meson", true],
  ["ninja", "ninja", "brew install ninja", true],
  ["claude", "Claude Code CLI (to try the AI features)", "https://claude.com/claude-code", false],
];
let missing = 0;
for (const [cmd, label, fix, required] of checks) {
  const ok = has(cmd);
  if (!ok && required) missing++;
  console.log(`${ok ? "ok  " : required ? "MISS" : "opt "}  ${label}${ok ? "" : `  ->  ${fix}`}`);
}
const sidecars = ["ffmpeg", "ffprobe", "cloudflared", "yt-dlp", "cli"].filter((n) => !existsSync(`src-tauri/binaries/${n}-${target}`));
console.log(sidecars.length ? `MISS  bundled helpers (${sidecars.join(", ")})  ->  node scripts/fetch-sidecars.mjs` : "ok    bundled helpers for " + target);
if (sidecars.length) missing++;
if (process.platform !== "darwin") console.log("\nNote: only macOS is wired up so far. See docs/ARCHITECTURE.md.");
process.exit(missing ? 1 : 0);
