#!/bin/bash
# Native check: what headless renders cannot show (sidebar vibrancy, inset traffic lights, the menu bar,
# the floating recording pill). Opens the built app, screenshots each screen by window id, then quits.
# Takes about 40 seconds and takes focus while it runs. Nothing is recorded: the pill is only opened
# (if your last setup had the camera on, its light comes on until the script quits the app).
#   bash scripts/native-check.sh            (uses your system appearance)
#   bash scripts/native-check.sh light      (or dark)
set -euo pipefail
cd "$(dirname "$0")/.."
NAME=$(node -p 'require("./brand.json").name')
SLUG=$(node -p 'require("./brand.json").slug.toUpperCase().replace(/-/g, "_")')
APP="src-tauri/target/universal-apple-darwin/release/bundle/macos/$NAME.app"
OUT="${TMPDIR:-/tmp}/native-check"; rm -rf "$OUT"; mkdir -p "$OUT"
[ -d "$APP" ] || { echo "Build first: npm run build -- --target universal-apple-darwin"; exit 1; }

env ${1:+${SLUG}_APPEARANCE=$1} "$APP/Contents/MacOS/app" >/dev/null 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null || true' EXIT

wid() { # window id by title
  swift -e "import CoreGraphics; import Foundation
for w in CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]] {
  if (w[\"kCGWindowName\"] as? String) == \"$1\" { print(w[\"kCGWindowNumber\"]!); break } }"
}
key() { osascript -e "tell application \"System Events\" to tell (first process whose unix id is $PID) to set frontmost to true" \
  -e "tell application \"System Events\" to keystroke \"$1\" using {command down}"; command sleep 1.5; }
shot() { screencapture -x -o -l "$(wid "$NAME")" "$OUT/$1.png"; }

command sleep 5
key 1; shot library
osascript -e "tell application \"System Events\" to tell (first process whose unix id is $PID) to set bounds of window 1 to {0, 30, 1180, 790}" 2>/dev/null || true
osascript -e 'tell application "System Events" to click at {300, 170}'; command sleep 2; shot player
key e; shot export; osascript -e 'tell application "System Events" to key code 53'; command sleep 1
key 2; shot shorts
key 3; shot shared
key ,; shot settings; osascript -e 'tell application "System Events" to key code 53'; command sleep 1
osascript -e "tell application \"System Events\" to tell (first process whose unix id is $PID) to click menu bar item \"File\" of menu bar 1"
command sleep 1; screencapture -x "$OUT/menu-bar.png"; osascript -e 'tell application "System Events" to key code 53'
key n; command sleep 2; screencapture -x -o -l "$(wid "$NAME controls")" "$OUT/recording-pill.png"
echo "Screenshots in $OUT"; open "$OUT"
