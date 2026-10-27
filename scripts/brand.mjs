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
  version: "0.1.1",
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
      {
        // Unified toolbar: content runs under the title bar, traffic lights inset into the toolbar,
        // and the sidebar shows the system sidebar material through the transparent window.
        label: "main", title: brand.name, url: "index.html", width: 1180, height: 760, minWidth: 900, minHeight: 600,
        titleBarStyle: "Overlay", hiddenTitle: true, trafficLightPosition: { x: 20, y: 26 },
        transparent: true, windowEffects: { effects: ["sidebar"], state: "followsWindowActiveState" },
      },
      {
        label: "hud", title: `${brand.name} controls`, url: "index.html#/hud", width: 440, height: 60,
        visible: false, decorations: false, transparent: true, alwaysOnTop: true, resizable: false,
        skipTaskbar: true, shadow: false,
      },
      {
        label: "bubble", title: `${brand.name} camera`, url: "index.html#/bubble", width: 220, height: 220,
        visible: false, decorations: false, transparent: true, alwaysOnTop: true, resizable: false,
        skipTaskbar: true, shadow: false,
      },
    ],
  },
  bundle: {
    active: true,
    targets: "all",
    icon: ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.png"],
    // Helpers shipped inside the app (scripts/fetch-sidecars.mjs prepares them).
    externalBin: ["binaries/ffmpeg", "binaries/ffprobe", "binaries/cloudflared", "binaries/yt-dlp", "binaries/scrcpy", "binaries/cli"],
    // The scrcpy server runs on the phone (one jar for every Mac), next to the notices.
    resources: { "../THIRD_PARTY_NOTICES.md": "THIRD_PARTY_NOTICES.md", "binaries/scrcpy-server": "scrcpy-server" },
    macOS: {
      minimumSystemVersion: "12.3",
      entitlements: "Entitlements.plist",
      hardenedRuntime: true,
      // Ad-hoc signed by default so it runs on Apple Silicon. For Developer ID signing and
      // notarization set APPLE_SIGNING_IDENTITY, APPLE_TEAM_ID and APPLE_ID + APPLE_PASSWORD
      // (or APPLE_API_KEY / APPLE_API_ISSUER) in the environment; tauri build picks them up.
      signingIdentity: process.env.APPLE_SIGNING_IDENTITY || "-",
    },
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
// The first line is the title, unless the README opens with a banner image instead.
if (readme[0].startsWith("# ")) readme[0] = `# ${brand.name}`;
let readmeText = readme.join("\n");
const mcp = [
  "<!-- brand:mcp -->",
  "```bash",
  "cd src-tauri && cargo build --release -p cli",
  `claude mcp add ${brand.slug} -- "$PWD/target/release/cli" mcp`,
  "```",
  "",
  `Then ask Claude Code things like "list my ${brand.name} videos" or "cut the first 10 seconds of my latest recording".`,
  "The same tool works without an agent: `cli list`, `cli transcript ID`, `cli plan ID \"remove the intro\"`, `cli apply ID plan.json --preview`.",
  "<!-- /brand:mcp -->",
].join("\n");
readmeText = readmeText.replace(/<!-- brand:mcp -->[\s\S]*?<!-- \/brand:mcp -->/, mcp);
const xattr = ["<!-- brand:xattr -->", "```bash", `xattr -dr com.apple.quarantine "/Applications/${brand.name}.app"`, "```", "<!-- /brand:xattr -->"].join("\n");
readmeText = readmeText.replace(/<!-- brand:xattr -->[\s\S]*?<!-- \/brand:xattr -->/, xattr);
writeFileSync(readmePath, readmeText);

console.log(`brand: ${brand.name} (${brand.identifier})`);
