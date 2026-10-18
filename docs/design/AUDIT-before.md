# Design audit: before the redesign

Date: 2026-10-18. Build: universal release of commit `92cab5c`, opened from the bundle.

How this was produced:
- Native window screenshots of the real app (`screencapture -l <window id>`), dark mode, at 1180x760 and 900x600.
- impeccable 4.6.0 detector: a static scan of `src/` and `watch.html`, plus URL scans of the live watch page at 1280x800 and 390x844.
- impeccable audit and critique heuristics, applied by reading the code and the screenshots.
- Light mode was checked in a Chromium render at the same size, so the founder's system appearance was not switched.
- The recording HUD and camera bubble were reviewed in code and in an earlier browser render; they were not started here.

## Detector

| Scan | Findings | Rules |
|---|---|---|
| Static, `src/` + `watch.html` | 1 | side-tab (3px accent `border-left` on the AI diff) |
| Watch page, 1280x800 | 10 | low-contrast x9 (accent `#6d5efc` text 4.0:1 on dark; placeholder 4.2:1), line-length x1 (~109 characters) |
| Watch page, 390x844 | 9 | low-contrast x9 |
| **Total** | **20** | |

## Audit scores (0 to 4)

| Dimension | Score | Why |
|---|---|---|
| Accessibility | 2 | No focus styling beyond defaults on most controls, tiny checkboxes, accent text below 4.5:1 in dark, icon-less unlabeled glyph buttons (✕, ▶). Esc does not close sheets. |
| Performance | 3 | Small bundle, but every inspector panel is mounted at once and the player re-renders on every `timeupdate`. |
| Theming | 2 | Tokens exist but many one-off values (`#e5484d`, `#15131bf2`, `#b26a00`), no system colours, accent used as text without a contrast variant. |
| Responsive (window sizes) | 1 | At 900x600 the player is a long page; panels clip; nothing collapses. Minimum window size 820x560 is below the 900x600 target. |
| Implementation integrity | 2 | Web-app shell inside a Mac window: duplicate title (window title bar plus a logo heading), no menu bar, no keyboard shortcuts beyond one global one, cards as the only container. |
| **Total** | **10 / 20** | |

## Critique (Nielsen heuristics, 0 to 4)

| Heuristic | Score | Note |
|---|---|---|
| Visibility of system status | 2 | Progress is a grey line of text; no determinate progress for transcription or renders. |
| Match with the real world (macOS) | 1 | No menu bar, no ⌘N / ⌘, / ⌘E, no context menus, no sheets, no inspector. |
| User control and freedom | 2 | Undo exists for AI edits, but Esc and ⌘. do not dismiss dialogs. |
| Consistency and standards | 1 | Three button styles, checkboxes vs. no toggles, mixed radii (6, 9, 10, 14, 16, 999). |
| Error prevention | 3 | Plans are previewed; destructive delete uses `confirm()`, which is a web dialog, not a Mac alert. |
| Recognition over recall | 2 | Nine panels in a scrolling grid; the user has to remember where things are. |
| Flexibility and efficiency | 1 | No shortcuts for play, scrub, export or inspector. |
| Aesthetic and minimalist design | 1 | Every feature is a bordered card of equal weight; the video and the transcript compete with ten action panels. |
| Error recognition and recovery | 2 | Errors are red text far from the control that failed. |
| Help and documentation | 2 | Helpful inline copy, but long sentences inside cramped cards. |
| **Total** | **17 / 40** | |

## Defects in the real app

| # | Type | Where | Defect |
|---|---|---|---|
| 1 | Overflow | Player, Share panel | Local link and "Get public link" overflow past the card's right edge; the checkbox label "(uses your Claude)" is clipped. |
| 2 | Clipped text | Player, Edit with a prompt | Textarea placeholder clipped by the button below it ("make my voice lo..."). |
| 3 | Clipped text | Player, Transcript card | Fixed max-height cuts a line in half with no fade or scrollbar hint. |
| 4 | Misaligned control | Player, Voice panel | "Auto level" checkbox renders as a 4px dot, misaligned with its two-line label. |
| 5 | Wrong spacing | Player, Transcript and AI | "Translate captions" wraps to two lines beside a squashed language picker. |
| 6 | Wrong spacing | Player, panel grid | Panels have unequal heights with large empty areas (Trim, Transcript and AI). |
| 7 | Layout at small sizes | Player at 900x600 | The whole player becomes a long scrolling page; only the video and two panels are visible. |
| 8 | Layout at small sizes | Window | Minimum window size is 820x560, below the 900x600 floor. |
| 9 | Native expectations | Window | Standard title bar plus an in-app logo heading: the name appears twice; no unified toolbar. |
| 10 | Native expectations | App | No menu bar items (only the default), no ⌘N, ⌘, , ⌘E, space, J/K/L. |
| 11 | Native expectations | New recording | A centred web modal instead of a Mac sheet or the floating capture toolbar; Esc does not close it. |
| 12 | Native expectations | Library | No context menu, no hover preview, no empty-state illustration. |
| 13 | Native expectations | Delete | Uses the web `confirm()` dialog. |
| 14 | Inconsistent controls | Everywhere | Three button styles, native number spinners, checkboxes for booleans, pill chips mixed with rectangles. |
| 15 | Contrast | Watch page and app, dark | Accent `#6d5efc` used as text: 4.0:1 on `#17161b` (needs 4.5:1). |
| 16 | Contrast | Watch page | Placeholder 4.2:1. |
| 17 | Contrast | Disabled primary button | Disabled "Apply" is muddy purple on dark, unclear whether it is disabled or loading. |
| 18 | Detector: side-tab | AI diff | 3px accent left border on the plan preview. |
| 19 | Line length | Watch page | Summary runs to about 109 characters per line. |
| 20 | Missing state | Library | No loading state; an empty library shows plain centred text. |
| 21 | Missing state | Long tasks | No determinate progress for transcription, renders or downloads in the player. |
| 22 | Missing state | Errors | Error text appears above the panels, far from the control that failed. |
| 23 | Sidebar | Sidebar | Settings is a sidebar tab next to content sections; there is no Shared section. |
| 24 | Icons | Everywhere | Unicode glyphs (●, ▶, ✕, ↗, ✓, ○) stand in for an icon set. |
| 25 | HUD | Recording controls | A dark slab with text buttons and a ✕ glyph; no source or camera choice in the pill; countdown is small text in the bar. |
| 26 | Bubble | Camera | 3px accent border and no shadow; no size options. |
| 27 | Theming | CSS | One-off colours outside tokens (`#e5484d`, `#f5a524`, `#15131bf2`, `#b26a00`, `#000a`). |

### Defect counts by type

| Type | Count |
|---|---|
| Overflow and clipped text | 3 |
| Misaligned controls and wrong spacing | 3 |
| Layout at small window sizes | 2 |
| Native macOS expectations (menus, sheets, shortcuts, toolbar, alerts, context menus) | 6 |
| Inconsistent controls and icons | 2 |
| Contrast | 3 |
| Detector anti-patterns (side-tab, line length) | 2 |
| Missing empty, loading and error states | 3 |
| Information architecture (sidebar, Settings, Shared) | 1 |
| Recording HUD and camera bubble | 2 |
| Theming tokens | 1 |
| **Total** | **28** (27 listed; #14 covers two kinds) |
