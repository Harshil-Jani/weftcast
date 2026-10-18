// Checks everything the app needs on this machine. `npm run doctor`
import { execSync } from "node:child_process";

const has = (cmd) => { try { execSync(`command -v ${cmd}`, { stdio: "ignore", shell: "/bin/bash" }); return true; } catch { return false; } };
const checks = [
  ["node", "Node.js 20+", "https://nodejs.org", true],
  ["cargo", "Rust toolchain", "https://rustup.rs", true],
  ["ffmpeg", "ffmpeg (recording and editing)", "brew install ffmpeg", true],
  ["whisper-cli", "whisper.cpp CLI (local transcripts)", "brew install whisper-cpp", false],
  ["cloudflared", "cloudflared (public links)", "brew install cloudflared", false],
  ["claude", "Claude Code CLI (AI features, or use an API key)", "https://claude.com/claude-code", false],
];
let missing = 0;
for (const [cmd, label, fix, required] of checks) {
  const ok = has(cmd);
  if (!ok && required) missing++;
  console.log(`${ok ? "ok  " : required ? "MISS" : "opt "}  ${label}${ok ? "" : `  ->  ${fix}`}`);
}
if (process.platform !== "darwin") console.log("\nNote: recording currently supports macOS only. See docs/ARCHITECTURE.md to add your OS.");
process.exit(missing ? 1 : 0);
