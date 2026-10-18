// Fails when the product name or bundle identifier is hardcoded anywhere except
// brand.json and the files scripts/brand.mjs generates. Run in CI: `npm run check:brand`.
import { readFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const brand = JSON.parse(readFileSync(join(root, "brand.json"), "utf8"));

const ALLOWED = new Set(["brand.json", "src-tauri/tauri.conf.json", "package.json", "package-lock.json", "README.md"]);
const files = execSync("git ls-files --cached --others --exclude-standard", { cwd: root, encoding: "utf8" })
  .split("\n")
  .filter((f) => f && !ALLOWED.has(f) && !/\.(png|icns|ico|jpg|gif|mp4|lock)$/.test(f));

const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const needles = [new RegExp(`\\b${escape(brand.name)}\\b`), new RegExp(escape(brand.identifier))];

const hits = [];
for (const f of files) {
  let text;
  try { text = readFileSync(join(root, f), "utf8"); } catch { continue; }
  text.split("\n").forEach((line, i) => {
    if (needles.some((re) => re.test(line))) hits.push(`${f}:${i + 1}: ${line.trim()}`);
  });
}

if (hits.length) {
  console.error(`The name "${brand.name}" is hardcoded. Read it from brand.json instead:\n` + hits.join("\n"));
  process.exit(1);
}
console.log(`check:brand ok (${files.length} files scanned)`);
