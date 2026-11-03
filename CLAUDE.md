---
type: architecture
description: Agent orientation for cooee — layout, the Meet adapter boundary, conventions, footguns.
links:
  - rel: see-also
    to: docs/FOLLOWUPS.md
---

# cooee — agent notes

macOS-only Tauri v2 menu-bar app (Rust workspace). Sibling of curator / warden / lector / flotilla:
shell-core supplies the release scripts, plugin registration, the Safari UA
(`shell_core::web::desktop_ua`) and per-session WebKit data stores; config-core supplies
config-path resolution.

**What's next:** [docs/FOLLOWUPS.md](docs/FOLLOWUPS.md). Start there before planning new work.

## Layout

- `crates/cooee-config/` — platform-neutral, unit-tested standalone: the config schema
  (`lib.rs`), Meet links and profile choice (`link.rs`), spot geometry (`spots.rs`), and the
  `cooee://join` + `profiles.json` contract (`deeplink.rs`).
- `src-tauri/src/` — the app: `meet.rs` (the one Meet window and its navigation policy),
  `faces.rs` (the popup made always-on-top), `spots.rs` (snapping, saving, global hotkeys),
  `tray.rs` (menu bar), `bridge.rs` (commands the injected scripts call), and `deeplink.rs`.
- `src-tauri/inject/` — scripts injected into meet.google.com:
  - `meet-adapter.js` is the **only code that knows Meet's DOM**. It publishes a neutral model on
    `window.__cooee` (`model`, `subscribe`, `act`, `dump`) and reports call state over IPC.
  - `faces.js` renders the faces popup from that model alone.

  Supporting another meeting platform means a second adapter that emits the same model.

## How the faces window works

`faces.js` calls `window.open("")` when the model says you're in a call. The Meet window's
`on_new_window` turns that request into the `faces` Tauri window, built with the opener's WebKit
configuration (`window_features`). That shared configuration is what lets the popup play the
call's `MediaStream`s. `faces.js` keeps a `RENDER = "canvas"` fallback in case a shared stream
won't render.

The faces window closes when the call ends (`meet_state` with `in_call = false`), when the Meet
page navigates, or when the Meet window is destroyed. A close by the user only hides it.

## Constraints that look optional but aren't

- **Meet needs a Safari UA.** WKWebView's native UA renders Meet blank. Use
  `shell_core::web::desktop_ua()`, never a literal.
- **Injected scripts reach Rust over Tauri IPC only.** Commands are declared in `build.rs`'s app
  manifest, and `capabilities/meet.json` grants them to `https://meet.google.com/*` alone. Do not
  report by navigating to a custom scheme: such a report made at page load froze Google's sign-in
  page.
- **The Meet window's start page is Google sign-in** (`meet::HOME_URL`), continuing to Meet. Meet's
  own `/` sends a signed-out profile to the Workspace marketing page, which `route_navigation`
  redirects to `HOME_URL`. Pointing `HOME_URL` at `/` would loop.
- **`on_navigation` sees every frame, not only the main frame**, so it can't send a page to the
  browser without also breaking add-on iframes and federated sign-in. Everything loads in place
  except the marketing redirect. Links Meet opens in a new window go to the browser from
  `on_new_window`.
- **Camera and mic need a bundled `.app`** with Info.plist usage strings, so `just run` builds a
  debug bundle. `cargo run` can't use the camera.
- **`cooee://` links arrive as `RunEvent::Opened`.** `Info.plist` declares the scheme, the same
  way flotilla does. `tauri-plugin-deep-link` never received the event here.
- **Spot geometry is in logical points.** Physical pixels are not one space on macOS: tao scales
  each monitor's origin by that monitor's own factor. A spot is the window's outer origin plus
  its inner size, because that is what `set_position` and `set_size` apply.

## Build, test, release

`just gate` before every commit (fmt check, clippy, active-patch guard, tests). The
`shell-dev`/`shell-pin` and `config-dev`/`config-pin` recipes switch a shared core between its
sibling checkout and its pinned rev.

Releasing uses shell-core's generated `scripts/release.sh` (`just release`). It needs the Apple
signing and notary env, plus the updater key (`TAURI_SIGNING_PRIVATE_KEY`, from
`~/.tauri/cooee-updater.key`). The updater's public key lives in `tauri.conf.json`
(`plugins.updater.pubkey`).

The version source is `src-tauri/Cargo.toml`.

The app icon is rendered from `src-tauri/icons/icon.svg`. Run this from `src-tauri/`:

```sh
rsvg-convert -w 1024 -h 1024 icons/icon.svg -o icons/icon.png
cargo tauri icon icons/icon.png -o icons
```

Then keep only the macOS sizes listed in `tauri.conf.json`.
