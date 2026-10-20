// Draws the app icon (logo 21, "frame and bubble") in the brand.json accent and regenerates
// every icon size: `npm run icon`. Needs rsvg-convert (brew install librsvg) on the dev machine.
import { readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const { accent } = JSON.parse(readFileSync(join(root, "brand.json"), "utf8"));
const out = join(root, "assets", "brand");
mkdirSync(out, { recursive: true });

// The mark on a 100-unit tile: a screen frame with the camera bubble on its corner, the bubble
// cut cleanly out of the frame so the tile shows through the gap.
const mark = (fill) => `
  <mask id="gap"><rect width="100" height="100" fill="#fff"/><circle cx="72" cy="72" r="18" fill="#000"/></mask>
  <rect x="20" y="20" width="56" height="56" rx="14" fill="none" stroke="${fill}" stroke-width="6" mask="url(#gap)"/>
  <circle cx="72" cy="72" r="13" fill="${fill}"/>`;

// macOS icon grid: 824pt squircle centred on a 1024 canvas, with the standard soft drop shadow.
const icon = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#fff" stop-opacity="0.16"/><stop offset="1" stop-color="#fff" stop-opacity="0"/>
    </linearGradient>
    <filter id="shadow" x="-10%" y="-10%" width="120%" height="125%">
      <feDropShadow dx="0" dy="10" stdDeviation="12" flood-color="#000" flood-opacity="0.28"/>
    </filter>
  </defs>
  <g filter="url(#shadow)"><rect x="100" y="100" width="824" height="824" rx="185" fill="${accent}"/></g>
  <rect x="100" y="100" width="824" height="824" rx="185" fill="url(#sheen)"/>
  <g transform="translate(100 100) scale(8.24)">${mark("#fff")}</g>
</svg>`;

// Menu bar template image: black on transparent, the bubble cut out of the frame (macOS tints it).
const tray = `<svg xmlns="http://www.w3.org/2000/svg" width="44" height="44" viewBox="0 0 100 100">
  <defs><mask id="cut"><rect width="100" height="100" fill="#fff"/><circle cx="72" cy="72" r="23" fill="#000"/></mask></defs>
  <rect x="12" y="12" width="62" height="62" rx="16" fill="none" stroke="#000" stroke-width="9" mask="url(#cut)"/>
  <circle cx="72" cy="72" r="15" fill="#000"/>
</svg>`;

writeFileSync(join(out, "icon.svg"), icon);
writeFileSync(join(out, "tray.svg"), tray);
const png = (svg, file, size) =>
  execFileSync("rsvg-convert", ["-w", String(size), "-h", String(size), "-o", join(out, file), join(out, svg)]);
png("icon.svg", "icon-1024.png", 1024);
png("tray.svg", "tray@2x.png", 44);
png("tray.svg", "tray.png", 22);
execFileSync("npx", ["tauri", "icon", join(out, "icon-1024.png")], { cwd: root, stdio: "inherit" });
// Desktop app only: drop the mobile icon sets the generator also writes.
for (const dir of ["android", "ios"]) rmSync(join(root, "src-tauri", "icons", dir), { recursive: true, force: true });
console.log(`icon: drawn in ${accent}, all sizes regenerated`);
