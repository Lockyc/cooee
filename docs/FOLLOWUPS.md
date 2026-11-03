---
type: reference
description: cooee's deferred work — each item with its reason and what unlocks it.
links:
  - rel: part-of
    to: CLAUDE.md
---

# Follow-ups

- **Calibrate the Meet adapter against a live call.** Its selectors (self tile, presentation tile,
  active speaker) are unverified, and the speaker cue is unset. Next action: save a layout
  snapshot (Diagnostics) in a call with a second participant, then set `SELECTORS` from it.
- **Confirm shared-stream rendering in the faces window.** If the popup shows black, switch
  `RENDER` to `"canvas"` in `inject/faces.js`.
- **Faces-window polish, from the v0.1 review.** None of these blocks a call. Next action: take
  them after the live calibration, since several need a live call to see.
  - The window takes focus on join, so Meet's ⌘D/⌘E stop working until you click back (`.focused(false)`).
  - Every render re-appends each tile, which may flicker video (re-append only on reorder).
  - A brief network freeze (`track.muted`) turns a tile into a chip and back (debounce 1–2 s).
  - Removed tiles and the canvas fallback keep their streams alive (`srcObject = null`).
  - Re-opening it while an old one is still registered hits the same destroy-then-build race the
    Meet window had.
  - Tray items for the spots and sizes, as a fallback when a hotkey clashes.
  - With 3+ profiles, a `cooee://join` link gets a clipboard instruction instead of a picker.
  - A profile named "Cancel" would join when Cancel is clicked.
  - Meet's page can trigger `adapter_dump` repeatedly (gate it on a tray-set flag).
  - A failed `meet_state` report is never retried.
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
