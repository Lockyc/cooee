# cooee

![Platform](https://img.shields.io/badge/platform-macOS-555)
![Rust](https://img.shields.io/badge/built%20with-Tauri%20v2-24C8DB?logo=tauri&logoColor=white)
[![License](https://img.shields.io/github/license/lockyc/cooee)](LICENSE)

Google Meet in its own window, plus an always-on-top window of the **other people's faces** that
you can snap to where you look: under the webcam while you talk, out of the way in a corner while
you work. macOS only.

Meet's own pop-out has to be big to show anyone, because it also shows your own screen share and
gives everyone with their camera off a full-size tile. cooee's faces window shows only the
people on camera. Everyone else is a name chip along the bottom edge.

## Status

Early, not yet released: built and unit-tested, not yet proven in a live call. Reading Meet's
call layout is the part most likely to need adjusting (see Diagnostics below).

## What it does

- **Menu-bar app.** No Dock icon between calls. The menu has **Join Meet Link from
  Clipboard**, one **Open Meet** item per profile, and Diagnostics.
- **Profiles.** Each profile is its own Google sign-in, in its own WebKit data store. A Meet link
  carrying `authuser=<email>` opens in the profile with that account. Otherwise cooee asks which
  profile to use.
- **Faces window.** It opens when you join a call and closes when you leave. It shows the other
  participants with their cameras on, re-flowed to fit the window, with the speaker outlined.
  Camera-off people are name chips. Before anyone else arrives it says **Waiting for others**,
  and it pulses when someone joins. Hovering shows Mute, Camera, Leave and the spot buttons.
- **Spots.** **Talk** is centred under the top edge of your largest screen, where a top-mounted
  webcam sits. **Aside** is that screen's bottom-right corner. Dragging or resizing the window
  never changes a spot; **Save Talk** / **Save Aside** does.
- **One call at a time.** A join that arrives during a call asks before leaving it.
- **Link routers.** `cooee://join?profile=<name>&url=<percent-encoded Meet link>` opens a link
  in a profile, and `~/Library/Application Support/au.lsjc.cooee/profiles.json` lists the
  profiles.

## Hotkeys

| Default | Action |
|---|---|
| ⌃⌥⌘T | Snap the faces window to Talk |
| ⌃⌥⌘A | Snap to Aside |
| ⌃⌥⌘S | Cycle size (keeps the top-centre fixed) |
| ⌃⌥⌘F | Show or hide the faces window |
| ⌃⌥⌘M | Show or hide the Meet window |
| ⌃⌥⌘⇧T | Save the current position as Talk |
| ⌃⌥⌘⇧A | Save the current position as Aside |

## Install

**Guided (Claude Code):** run `/cooee:install`. It checks prerequisites (offering to install any
that are missing), builds cooee from source, installs it, and seeds your config.

**Manual:**

```bash
curl -fsSL https://raw.githubusercontent.com/lockyc/cooee/main/install.sh | bash
```

This clones cooee to `~/.cooee`, builds it, installs it to `/Applications`, and seeds
`~/.config/cooee/config.toml`. Needs Rust and the Xcode Command Line Tools; the installer
backstops the Tauri CLI.

## Config

`~/.config/cooee/config.toml` (or `$COOEE_CONFIG`), read at launch. A missing file means all
defaults. An unknown key is an error.

```toml
[[profile]]
name = "work"
account = "me@example.com"

[[size]]
width = 560
height = 350

[hotkeys]
talk = "Ctrl+Alt+Cmd+T"
```

**`[[profile]]`** (repeatable; none declared means one profile named `default`)

| Key | Default | What it does |
|---|---|---|
| `name` | required | The profile's name, unique. It keys the sign-in, so renaming a profile signs it out. |
| `account` | none | The Google account email matched against a Meet link's `authuser`. |

**`[[size]]`** (repeatable; the steps "cycle size" moves through, in points)

| Key | Default | What it does |
|---|---|---|
| `width`, `height` | 360×225, 560×350, 800×500 | One faces-window size. Talk's default uses the middle one, Aside's the smallest. |

**`[hotkeys]`** (accelerators like `Ctrl+Alt+Cmd+T`; `Cmd`, `Ctrl`, `Alt`, `Shift`)

| Key | Default |
|---|---|
| `talk` | `Ctrl+Alt+Cmd+T` |
| `aside` | `Ctrl+Alt+Cmd+A` |
| `cycle_size` | `Ctrl+Alt+Cmd+S` |
| `toggle_faces` | `Ctrl+Alt+Cmd+F` |
| `toggle_meet` | `Ctrl+Alt+Cmd+M` |
| `save_talk` | `Ctrl+Alt+Cmd+Shift+T` |
| `save_aside` | `Ctrl+Alt+Cmd+Shift+A` |

## Diagnostics

If Google changes Meet's layout, the faces window says it can't find the video tiles. In a call,
**Diagnostics → Save Meet Layout Snapshot** writes the tiles' structure (no video, no chat) to
`adapter-dump.json` and reveals it, which is what a fix is made from.

## Develop

```sh
just run      # build a debug .app and run it against examples/config.toml
just gate     # format check, clippy, tests
```

## License

MIT
