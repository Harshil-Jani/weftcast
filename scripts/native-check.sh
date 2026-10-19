#!/bin/bash
# Native check: what headless renders cannot show (sidebar vibrancy, inset traffic lights, the menu bar,
# the floating recording pill). Opens the built app, screenshots each screen by window id, then quits.
# Takes about 40 seconds and takes focus while it runs. Nothing is recorded: the pill is only opened
# (if your last setup had the camera on, its light comes on until the script quits the app).
#   bash scripts/native-check.sh            (uses your system appearance)
#   bash scripts/native-check.sh light      (or dark)
#   bash scripts/native-check.sh --phone    (also checks a phone plugged in over USB; see the end)
#   bash scripts/native-check.sh --airplay  (an iPhone mirroring to this Mac with AirPlay: start it first)
#   bash scripts/native-check.sh --android-wifi 192.168.1.20:41235   (an Android phone paired over Wi-Fi)
set -euo pipefail
cd "$(dirname "$0")/.."
PHONE=0; AIRPLAY=0; WIFI=""
while [ $# -gt 0 ]; do
  case "$1" in
    --phone) PHONE=1; shift ;;
    --airplay) AIRPLAY=1; shift ;;
    --android-wifi) WIFI="${2:?give the address and port from Wireless debugging}"; shift 2 ;;
    *) break ;;
  esac
done
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
# --airplay: finds the mirroring window the way the app does and records 5 seconds of just that window.
if [ "$AIRPLAY" = 1 ]; then
  BIN="$APP/Contents/MacOS"
  JS=$(python3 - <<'PY'
import re
src = open("src-tauri/src/airplay.rs").read()
print(re.search(r'const LIST_JS: &str = r#"(.*?)"#;', src, re.S).group(1))
PY
)
  osascript -l JavaScript -e "$JS" > "$OUT/windows.json" || true
  read -r SCREEN CROP < <(python3 - "$OUT/windows.json" <<'PY'
import json, sys
s = json.load(open(sys.argv[1]))
ws = [w for w in s["windows"] if ("airplay" in w["owner"].lower() or "mirroring" in w["owner"].lower()) and w["layer"] == 0 and w["b"]["Width"] >= 160 and w["b"]["Height"] >= 160]
if not ws: print("none none"); sys.exit()
w = max(ws, key=lambda w: w["b"]["Width"] * w["b"]["Height"]); b = w["b"]; main_h = s["screens"][0]["h"]
for i, d in enumerate(s["screens"]):
    top = main_h - (d["y"] + d["h"]); cx, cy = b["X"] + b["Width"] / 2, b["Y"] + b["Height"] / 2
    if d["x"] <= cx < d["x"] + d["w"] and top <= cy < top + d["h"]:
        chrome = 28 if w.get("name") else 0; k = d["scale"]
        x = round(max(0, b["X"] - d["x"]) * k); y = round(max(0, b["Y"] - top + chrome) * k)
        cw = min(int(b["Width"] * k), int(d["w"] * k) - x) & ~1; ch = min(int((b["Height"] - chrome) * k), int(d["h"] * k) - y) & ~1
        print(i, f"crop={cw}:{ch}:{x}:{y}"); break
else: print("none none")
PY
)
  if [ "$SCREEN" = none ]; then
    echo "AirPlay: no iPhone mirroring (on the iPhone: Control Center, Screen Mirroring, pick this Mac)"
  else
    "$BIN/ffmpeg" -hide_banner -f avfoundation -list_devices true -i "" 2> "$OUT/avfoundation-devices.txt" || true
    DEV=$(grep -E "\] Capture screen $SCREEN\$" "$OUT/avfoundation-devices.txt" | sed -E 's/.*\] \[([0-9]+)\].*/\1/' | head -1)
    "$BIN/ffmpeg" -hide_banner -y -f avfoundation -framerate 30 -capture_cursor 0 -i "$DEV:none" -t 5 -vf "$CROP" -c:v h264_videotoolbox "$OUT/airplay-screen.mp4" 2> "$OUT/airplay-record.log" \
      && "$BIN/ffmpeg" -hide_banner -y -ss 2 -i "$OUT/airplay-screen.mp4" -frames:v 1 "$OUT/airplay-screen.png" 2>/dev/null && echo "AirPlay screen: recorded ($CROP)" || echo "AirPlay screen: FAILED (see airplay-record.log)"
  fi
fi
# --android-wifi ADDR: connects over Wi-Fi (pair first in Connect Phone), then records 5 seconds.
if [ -n "$WIFI" ]; then
  BIN="$APP/Contents/MacOS"
  ADB="$HOME/Library/Application Support/$(node -p 'require("./brand.json").identifier')/bin/platform-tools/adb"
  [ -x "$ADB" ] || ADB=$(command -v adb || true)
  if [ -z "$ADB" ]; then echo "Android Wi-Fi: no adb (use Connect Phone > Get Android Tools first)"; else
    "$ADB" connect "$WIFI" > "$OUT/android-wifi-connect.log" 2>&1 || true
    ADB="$ADB" SCRCPY_SERVER_PATH="$APP/Contents/Resources/scrcpy-server" "$BIN/scrcpy" --serial "$WIFI" --no-playback --no-window --no-audio \
      --no-control --time-limit=5 --record="$OUT/android-wifi-screen.mp4" > "$OUT/android-wifi-record.log" 2>&1 \
      && "$BIN/ffmpeg" -hide_banner -y -ss 2 -i "$OUT/android-wifi-screen.mp4" -frames:v 1 "$OUT/android-wifi-screen.png" 2>/dev/null && echo "Android over Wi-Fi: recorded" || echo "Android over Wi-Fi: FAILED (see android-wifi-*.log)"
  fi
fi
echo "Screenshots in $OUT"; open "$OUT"
