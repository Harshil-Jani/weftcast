#!/bin/bash
# Native check: what headless renders cannot show (sidebar vibrancy, inset traffic lights, the menu bar,
# the floating recording pill). Opens the built app, screenshots each screen by window id, then quits.
# Takes about 40 seconds and takes focus while it runs. Nothing is recorded: the pill is only opened
# (if your last setup had the camera on, its light comes on until the script quits the app).
#   bash scripts/native-check.sh            (uses your system appearance)
#   bash scripts/native-check.sh light      (or dark)
#   bash scripts/native-check.sh --phone    (also checks a phone plugged in over USB; see the end)
set -euo pipefail
cd "$(dirname "$0")/.."
PHONE=0; [ "${1:-}" = "--phone" ] && { PHONE=1; shift; }
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
key P; command sleep 2; shot connect-phone; osascript -e 'tell application "System Events" to key code 53'

# --phone: what only a real device can show. Plug in an iPhone (unlocked, Trust) and/or an Android
# phone (USB debugging on) first. Records 5 seconds from each with the bundled tools, no app UI needed.
if [ "$PHONE" = 1 ]; then
  BIN="$APP/Contents/MacOS"
  "$BIN/ffmpeg" -hide_banner -f avfoundation -list_devices true -i "" 2> "$OUT/avfoundation-devices.txt" || true
  IDX=$(grep -iE "\] \[[0-9]+\] .*(iphone|ipad)" "$OUT/avfoundation-devices.txt" | grep -iv camera | head -1 | sed -E 's/.*\] \[([0-9]+)\].*/\1/')
  if [ -n "$IDX" ]; then
    "$BIN/ffmpeg" -hide_banner -y -f avfoundation -framerate 30 -i "$IDX:none" -t 5 -c:v h264_videotoolbox "$OUT/iphone-screen.mp4" 2> "$OUT/iphone-record.log" \
      && "$BIN/ffmpeg" -hide_banner -y -ss 2 -i "$OUT/iphone-screen.mp4" -frames:v 1 "$OUT/iphone-screen.png" 2>/dev/null && echo "iPhone screen: recorded" || echo "iPhone screen: FAILED (see iphone-record.log)"
  else
    echo "iPhone screen: not listed (see avfoundation-devices.txt; unlock the phone and tap Trust)"
  fi
  ADB="$HOME/Library/Application Support/$(node -p 'require("./brand.json").identifier')/bin/platform-tools/adb"
  [ -x "$ADB" ] || ADB=$(command -v adb || true)
  if [ -n "$ADB" ] && SERIAL=$("$ADB" devices | awk 'NR>1 && $2=="device"{print $1; exit}') && [ -n "$SERIAL" ]; then
    ADB="$ADB" SCRCPY_SERVER_PATH="$APP/Contents/Resources/scrcpy-server" "$BIN/scrcpy" --serial "$SERIAL" --no-playback --no-window --no-audio \
      --no-control --time-limit=5 --record="$OUT/android-screen.mp4" > "$OUT/android-record.log" 2>&1 \
      && "$BIN/ffmpeg" -hide_banner -y -ss 2 -i "$OUT/android-screen.mp4" -frames:v 1 "$OUT/android-screen.png" 2>/dev/null && echo "Android screen: recorded" || echo "Android screen: FAILED (see android-record.log)"
  else
    echo "Android: no device (or no adb: use Connect Phone > Get Android Tools first)"
  fi
fi
echo "Screenshots in $OUT"; open "$OUT"
