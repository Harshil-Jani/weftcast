# Feature map

Built clean room from Loom's public product pages only. No Loom app, binary or private API was inspected.

Sources:
- [Loom product page](https://www.atlassian.com/software/loom) (S1)
- [Loom AI page](https://www.atlassian.com/software/loom/ai) (S2)
- [Loom help center](https://support.atlassian.com/loom/) (S3)

Scope decided by the maintainers: a desktop app (Tauri), local first with an optional public link, and Claude for the smart parts through the user's own `claude` CLI or API key.

## MUST (v1)

| Feature | What it means here | Source |
|---|---|---|
| Screen recording with voice | Record a display with mic narration in a few clicks | S1 |
| Camera bubble | A round, always-on-top camera window that is captured with the screen | S1 |
| Recording controls | Countdown, pause, resume, stop, a global shortcut | S1 |
| Instant library | Every recording lands in a local library to watch, rename, delete | S1 |
| Trim | Cut the start and end | S1 ("Editing") |
| Shareable link | A watch page; one click turns it into a public link while the app runs | S1 |
| Transcript and captions | Local Whisper transcript with word timings, captions in the player, SRT/VTT export | S2 |
| Auto title | Claude names the video from the transcript | S2 |
| Summary | Claude writes a short overview | S2 |
| Chapters | Claude splits the video into labelled sections that seek the player | S2 |
| Filler word removal | Cut "um", "uh" and similar using word timings | S2 |
| Silence removal | Trim dead air using silence detection | S2 |
| Ask the video | Viewers ask a question, Claude answers from the transcript with timestamps | Our addition, in the spirit of S2's AI workflows |

## LATER

| Feature | Note | Source |
|---|---|---|
| Record a single window | Needs a native capture path per OS (v1 records whole displays) | S1 |
| 4K recording | Depends on display and encoder settings | S1 |
| Tasks / action items from a video | Same pipeline as summary | S2 |
| Video to document (SOP, PR description) | Same pipeline as summary | S2 |
| Video to issue (bug report) | Needs tracker integrations | S2 |
| Message to accompany the link | Same pipeline as summary | S2 |
| Screenshot tool with annotation | Separate capture flow | S1 |
| Viewer comments and reactions | Needs a shared store for public viewers | S1 |
| Call-to-action link on the video | Watch page option | S2 |
| Multilingual transcripts | Whisper multilingual models are larger | S2 |
| Windows and Linux capture | Architecture leaves room (see ARCHITECTURE.md) | n/a |

## SKIP

| Feature | Why | Source |
|---|---|---|
| Meeting bots and meeting recaps | Not a local recorder's job | S1 |
| Atlassian MCP, Rovo, Jira, Confluence | Vendor ecosystem | S1 |
| AI voices and avatars | Out of scope | S2 |
| Workspaces, admin, SSO, viewer analytics dashboards | Needs a hosted backend | S1 |
