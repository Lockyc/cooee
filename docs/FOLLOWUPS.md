---
type: reference
description: cooee's deferred work — each item with its reason and what unlocks it.
links:
  - rel: part-of
    to: CLAUDE.md
---

# Follow-ups

- **Calibrate the Meet adapter against a live call.** Its selectors (self tile, presentation tile,
  your own presentation's tile, active speaker) are unverified, and the speaker cue is unset. Next
  action: save a layout snapshot (Diagnostics) in a call where a second participant shares their
  screen, and one while you share yours, then set `SELECTORS` from them.
- **The faces window changes size when dragged from one TV to the other, and ⌘-Tab to cooee
  doesn't bring the Meet window forward.** Both TVs are scale 1, and nothing in cooee sizes a
  popup on a move. Next action: after a call where either happens, read `window-events.log` in the
  app data dir for the resize's source and for which window held focus when cooee activated.
- **Confirm shared-stream rendering in the popups.** If a popup shows black, switch `RENDER` to
  `"canvas"` in `inject/popup.js`.
- **Manicule target.** Manicule should route `meet.google.com` links to cooee, reading
  `profiles.json` and launching
  `open -b au.lsjc.cooee 'cooee://join?profile=<name>&url=<link>'`. That change belongs in
  `lockyc/manicule`, as a new target kind beside flotilla's.
- **Calendar list in the menu bar** (today's meetings, each with Join). Deferred until there is a
  calendar source covering every profile's account. One candidate is macOS EventKit, once each
  account is added to Calendar.app.
- **Chromium engine.** Swap to Tauri's CEF runtime once `tauri-runtime-cef` is stable, if
  WebKit's Meet support degrades.
- **Hot-reloading the config.** Today it is read at launch. Unlock: wanting edits to apply live.
  shell-core's `watch` is the mechanism.
- **Other meeting platforms.** Each would be a second adapter emitting the same model. Unlock: a
  platform worth supporting.
- **Transparent or per-person faces windows.** Possible natively. Unlock: wanting it.
