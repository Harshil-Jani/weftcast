# Design audit: after the redesign

Date: 2026-10-19. Compare with [AUDIT-before.md](AUDIT-before.md).

How this was produced:
- **Headless renders.** Every screen and state of the built frontend was rendered in headless Chromium (Playwright) with the Tauri IPC mocked:
  - **Fixtures:** real library data from the local media server (metadata, transcripts, Shorts suggestions). The public sample video also got a fixture summary and chapters, so those panels have content.
  - **Coverage:** light and dark, at 1440x900 and 900x600.
- **Detector.** impeccable 4.6.0 ran three times:
  - on a DOM snapshot of each rendered screen, with the light and the dark tokens each forced, because the detector's browser follows the host's colour scheme;
  - on the static source;
  - on the live watch page.
- **Native-only parts.** These were checked by reviewing the generated `tauri.conf.json` and the Rust menu code, not by rendering:
  - sidebar vibrancy
  - inset traffic lights
  - the menu bar
  - window chrome

  `scripts/native-check.sh` screenshots them in about 40 seconds.
- **Earlier native captures.** The redesign was first checked in the real window during development, before the headless rule. That pass found the popover overflow, the stale menu command replay and the invisible dark fields listed below.

## Detector

| Scan | Before | After |
|---|---|---|
| Static, `src/` + `watch.html` | 1 | 0 |
| Watch page, 1280x800 | 10 | 0 |
| Watch page, 390x844 | 9 | 0 |
| App screens: 19 screens x 2 sizes x 2 modes = 76 snapshots | not measurable before (no mockable states) | 0 |
| **Total** | **20** | **0** |

First after-run on the app screens, before the fixes below: 240 findings.
- undersized UI text x120
- nested cards x60
- monotonous spacing x16
- flat type hierarchy x16
- text occlusion x16
- low contrast x12

The fixes:
- 11px caption floor
- the sidebar edge draws the divider (`.main` no longer reads as a card)
- flat lists inside sheets
- Title 1 sheet titles and Title 3 section heads
- secondary text `#636366` in light
- the selected sidebar count in label colour
- Share moved from a popover to a sheet, as the brief specifies
- an 8px rhythm inside Short cards

## Audit scores (0 to 4)

| Dimension | Before | After | Why |
|---|---|---|---|
| Accessibility | 2 | 3 | Things that now hold:<br>- every text pair passes 4.5:1 (detector, both modes)<br>- focus rings on all controls<br>- labelled icon buttons, `role=switch` toggles, radiogroup segmented controls with arrow keys<br>- focus trap and Esc in sheets<br>- `aria-live` task status<br>- reduced motion honoured<br><br>Not yet: a VoiceOver pass in the real window. |
| Performance | 3 | 3 | The inspector mounts one tab at a time. Hover previews load only while hovered. The player still re-renders on `timeupdate`. |
| Theming | 2 | 4 | One tokens file (`src/styles/tokens.css`) with semantic light and dark sets. Accent variants (fill, text, soft, focus) are derived from `brand.json`, and there are no one-off colours in components. |
| Responsive (window sizes) | 1 | 4 | The 900x600 minimum is enforced. The sidebar shrinks to 200 and the inspector to 288 below 1100. Nothing clips or wraps badly at 900x600 (all screens rendered). |
| Implementation integrity | 2 | 3 | Done:<br>- overlay title bar<br>- native menu bar with standard shortcuts<br>- native context menus and alerts<br>- sheets<br>- inspector<br>- Lucide icons<br>- shared components<br><br>Native rendering not yet confirmed (see the native check). |
| **Total** | **10 / 20** | **17 / 20** | |

## Critique (Nielsen heuristics, 0 to 4)

| Heuristic | Before | After | Note |
|---|---|---|---|
| Visibility of system status | 2 | 3 | A spinner and determinate progress sit under the control that started the task. The render progress bar is in Shorts. |
| Match with the real world (macOS) | 1 | 3 | Mac conventions now in place:<br>- menu bar, ⌘N, ⌘,, ⌘E, ⇧⌘S, ⌘1/2/3, ⌥⌘I, ⌃⌘S<br>- Space/J/K/L<br>- sheets, native context menus, native delete alert, NSOpenPanel import |
| User control and freedom | 2 | 4 | Esc closes every sheet. Esc on the welcome sheet means Skip. Undo and Restore Original live in the More menu. The recording can be discarded from the pill. |
| Consistency and standards | 1 | 4 | One button family (22/28px, 6px radius), segmented controls, toggles, one slider and one field style. |
| Error prevention | 3 | 3 | AI plans are previewed. Trim disables an empty range. Delete asks first. |
| Recognition over recall | 2 | 3 | Four inspector tabs replace nine stacked panels. |
| Flexibility and efficiency | 1 | 3 | Full shortcut set, native right-click menu on library items, tab-reachable controls. |
| Aesthetic and minimalist design | 1 | 3 | The video leads. Tools wait in the inspector. Grouped lists replace bordered cards. |
| Error recognition and recovery | 2 | 3 | Errors appear in red right under the control that failed; the HUD error offers Open Settings. |
| Help and documentation | 2 | 3 | Shortcuts are in the README and in tooltips. Inline copy is shorter. |
| **Total** | **17 / 40** | **32 / 40** | |

## Before defects: status

All 27 defects from AUDIT-before.md are fixed in the code. Five of them (#9 to #11, #25, #26) are verified in headless renders and in config review only, not in a native window yet.

| Type | Before | Fixed |
|---|---|---|
| Overflow and clipped text | 3 | 3 |
| Misaligned controls and wrong spacing | 3 | 3 |
| Layout at small window sizes | 2 | 2 |
| Native macOS expectations | 6 | 6 |
| Inconsistent controls and icons | 2 | 2 |
| Contrast | 3 | 3 |
| Detector anti-patterns | 2 | 2 |
| Missing empty, loading and error states | 3 | 3 |
| Information architecture | 1 | 1 |
| Recording HUD and camera bubble | 2 | 2 |
| Theming tokens | 1 | 1 |
| **Total** | **28** | **28** |

Defects found and fixed during the redesign itself:
1. The Share panel's link row overflowed its container (grid items with `min-width: auto`). Every grid container now uses `minmax(0, 1fr)`.
2. A menu command replayed when a screen remounted, so the Export sheet opened on its own. Fixed with `useCommand`, which ignores the command that opened the screen, and navigation now clears pending commands.
3. Text fields and the selected segment were nearly invisible in dark mode. They now have their own field and segment tokens.
4. Plain buttons stretched to full width inside grid sections.
5. The tooltip and toolbar stacking order.
6. The dialog plugin's crate and npm versions didn't match. Both are now pinned to 2.8.1.
7. The 240 detector findings listed above.

## Not verified here

- Sidebar vibrancy, traffic-light inset, the real menu bar and the overlay title bar in the bundled window: run `bash scripts/native-check.sh`.
- The camera bubble with a real camera. The headless render uses Chromium's synthetic camera.
- VoiceOver reading order in WKWebView.

## Round 2 (design gate fixes, 2026-10-19)

The design gate passed at 52/60. These fixes followed it:
1. **Shorts**
   - fixed 232 to 264px columns
   - two-line title that is still renamed in place
   - headline-size score pill, accent-tinted at 8 or more
   - previews paused on a mid-clip frame instead of the often-dark first frame
2. **Transcript**
   - sentence rows in the app and on the watch page: they end at . ? or ! (also mid-cue, with the start time estimated) or before a 1.5s pause
   - the active row is highlighted and kept in view while playing
   - 44px tabular time column
   - `npm test` checks the rule
3. **Sheets**
   - Export has Cancel (Esc) and Export as the default button (Return)
   - formats read SRT Subtitles and WebVTT Subtitles
   - Share labels the field "This Mac only" while the public link is off; once it's on, Copy Link is the default
4. **Recording pill**
   - real display names from the system, with "Display N" as the fallback (unit tested)
   - the microphone toggle and its menu form one split button
5. **Layout**
   - solid sheet footers with a hairline above
   - Suggest Clips moved into the Shorts toolbar
   - toolbar titles take the free space
   - trim times are typed as m:ss.s
6. **Watch page**
   - title, then date and duration, then the summary
   - the caption picker sits in the Transcript header
   - the app icon is the brand mark
   - the dark background matches the app's #1e1e1e

Detector after round 2: 0 findings on 80 app snapshots, 0 static, and 0 on the watch page at 1440, 900 and 390.

## Round 3: Connect Phone (2026-10-19)

New surfaces:
- the Connect Phone sheet (Phone Screen, Face Cam, Remote & Angle)
- the phone label and Connect Phone button in the recording pill
- the inspector's Phone section
- the phone web page (Camera and Remote tabs)

Detector: 0 findings on 96 app snapshots (light and dark, 1440 and 900), and 0 on the live phone page at 390 and 1440.

Headless checks, with no real phone:
- 11 browser checks in Chromium, using fake camera and microphone:
  - WebRTC frames reach the bubble
  - a browser page from another origin gets 403 on the pairing routes
  - a wrong token gets 401
  - the phone camera is recorded, paused in step, uploaded and attached as a track
  - the phone's Start, timer and Stop drive the app through the controls window
- Compositor run on synthetic media, for each layout (right, left, PiP, split, second-angle inset and split, B-roll):
  - the phone flash lands at its offset exactly (2.500s for a 0.5s offset)
  - A/V stays within a quarter of a frame (beep 8ms after the matching frame)
  - after a cut, both tracks move with the edit
