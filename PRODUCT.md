# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

A macOS desktop app built with Tauri (a native window around a web view). The founder made Apple's macOS Human Interface Guidelines binding for the desktop app, so it should look and behave like a native Mac app, not a website in a window. The public watch page is a regular web page.

## Users

Founders and engineers who send async video instead of meetings: walkthroughs, bug reports, design reviews, updates for a team or a client. They record often, share fast, and care that recordings stay on their own machine.

## Product Purpose

A local, private alternative to Loom. Record the screen with camera and voice, get a link, and let Claude handle titles, summaries, chapters, cleanup, translations, shorter cuts and vertical shorts. Success: from "I should explain this" to a shared link in under a minute, with nothing uploaded to a third-party video host.

## Positioning

Everything runs on the user's Mac: recording, transcription (whisper.cpp), editing and the watch page. Public links are a tunnel that only exists while the app runs. AI features use the user's own Claude (their Claude Code login or their API key); the app never holds anyone's login.

## Operating Context

- Used between other work: start with a shortcut, record a few minutes, stop, share.
- Viewers open the watch page in any browser, often on a phone.
- AI agents can drive the library through the bundled MCP server and CLI.

## Capabilities and Constraints

- Record a display with mic and a camera bubble; pause, resume, discard.
- Library, player, trim, remove ums and silences, voice level, local transcripts and captions, caption translation.
- Claude: title, summary, chapters, ask the video, edit with a prompt, make it shorter, shorts picking.
- Shorts: 9:16 renders with burned-in captions.
- Sharing through a temporary public link; exports to Downloads.
- macOS only for now. The product name is a variable (`brand.json`) and is not decided yet.

## Brand Commitments

- Name, accent colour and tagline come from `brand.json`; nothing hardcodes the name.
- Voice: calm and precise. Plain words, no hype, no em dashes.
- Desktop app follows Apple's macOS HIG (founder's directive). The watch page follows the same language adapted for the web.

## Evidence on Hand

No customers, testimonials or usage numbers exist yet. Do not invent any.

## Product Principles

1. Local first: the video never has to leave the Mac to be useful.
2. Preview before anything changes: every AI edit is shown, applied on request and undoable.
3. Fast path first: recording and sharing take the fewest possible steps; everything else waits in an inspector.
4. Native expectations win: Mac shortcuts, menus, sheets and system colours behave the way Mac users expect.

## Accessibility & Inclusion

WCAG AA contrast (4.5:1 text), full keyboard access, visible focus rings, VoiceOver labels on every control, and reduced motion respected.

<!-- Inferred, not interviewed: this file was written from the founder's brief (product, audience, voice, HIG directive) and the repository, without a live interview. -->
