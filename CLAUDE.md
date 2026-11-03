---
type: architecture
description: Agent orientation for cooee — layout, the Meet adapter boundary, conventions.
---

# cooee — agent notes

macOS-only Tauri v2 menu-bar app (Rust workspace). Sibling of curator / warden / lector / flotilla:
shell-core supplies the release scripts, plugin registration, Safari UA and per-session WebKit
data stores; config-core supplies config-path resolution.

- `src-tauri/` — the app (package `cooee`, lib `cooee_lib`).
- `crates/cooee-config/` — platform-neutral config + Meet-link parsing; unit-tested standalone.

`just run` builds a debug `.app` (camera/mic need a real bundle with Info.plist usage strings, so
never `cargo run`) and launches it against `examples/config.toml`. `just gate` before every commit.
