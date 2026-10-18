// Generates every file that has to spell the product name, from brand.json.
// Run automatically by `npm run dev`, `npm run build` and `npm run tauri ...`.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const brand = JSON.parse(readFileSync(join(root, "brand.json"), "utf8"));

for (const key of ["name", "slug", "identifier", "tagline", "accent", "repo"]) {
  if (typeof brand[key] !== "string" || !brand[key]) throw new Error(`brand.json: "${key}" is required`);
}

// GENERATED files (listed in scripts/check-brand.mjs as allowed to contain the name).
const tauriConf = {
  $schema: "https://schema.tauri.app/config/2",
  productName: brand.name,
  version: "0.1.0",
  identifier: brand.identifier,
  build: {
    beforeDevCommand: "npm run vite:dev",
    devUrl: "http://localhost:1420",
    beforeBuildCommand: "npm run vite:build",
    frontendDist: "../dist",
  },
  app: {
    macOSPrivateApi: true,
    withGlobalTauri: false,
    security: { csp: null },
    windows: [
      { label: "main", title: brand.name, url: "index.html", width: 1180, height: 760, minWidth: 820, minHeight: 560 },
      {
        label: "hud", title: `${brand.name} controls`, url: "index.html#/hud", width: 420, height: 64,
        visible: false, decorations: false, transparent: true, alwaysOnTop: true, resizable: false,
        skipTaskbar: true, shadow: false,
      },
      {
        label: "bubble", title: `${brand.name} camera`, url: "index.html#/bubble", width: 220, height: 220,
        visible: false, decorations: false, transparent: true, alwaysOnTop: true, resizable: true,
        skipTaskbar: true, shadow: false,
      },
    ],
  },
  bundle: {
    active: true,
    targets: "all",
    icon: ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.png"],
    macOS: { minimumSystemVersion: "12.3", entitlements: null },
  },
};
writeFileSync(join(root, "src-tauri/tauri.conf.json"), JSON.stringify(tauriConf, null, 2) + "\n");

const pkgPath = join(root, "package.json");
const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
if (pkg.name !== brand.slug) {
  pkg.name = brand.slug;
  writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");
}

const readmePath = join(root, "README.md");
const readme = readFileSync(readmePath, "utf8").split("\n");
readme[0] = `# ${brand.name}`;
writeFileSync(readmePath, readme.join("\n"));

console.log(`brand: ${brand.name} (${brand.identifier})`);
