#!/usr/bin/env bash
# install.sh — build cooee from source and install it to /Applications.
# Usage:  bash install.sh
#    or:  curl -fsSL https://raw.githubusercontent.com/lockyc/cooee/main/install.sh | bash
#
# The curl URL and the git clone below require the GitHub repo to be PUBLIC.
#
# Two modes, auto-detected:
#   • IN_REPO     — run from a checkout: builds the current working tree (so local
#                   changes are picked up). No clone/pull.
#   • NOT_IN_REPO — otherwise: manages a persistent source clone at ~/.cooee
#                   (clone if absent, git pull if present) and builds from it.
#
# Never relaunches the app (the caller decides) and never depends on `just`.
# For guided setup with prerequisite installation, use /cooee:install in Claude Code.
set -euo pipefail

if [[ "$(uname)" != "Darwin" ]]; then
  echo "cooee is a macOS-only app; install.sh only runs on macOS." >&2
  exit 1
fi

REPO_URL="https://github.com/lockyc/cooee"
INSTALL_DIR="$HOME/.cooee"

# 1. Hard prerequisites. /cooee:install offers to install these; the bare script
#    only refuses with a hint (except the Tauri CLI, which it backstops below).
missing=0
for c in git cargo; do
  if ! command -v "$c" >/dev/null 2>&1; then
    echo "cooee: '$c' is required but not found on PATH" >&2
    missing=1
  fi
done
if [ "$missing" -ne 0 ]; then
  echo "cooee: install Rust (https://rustup.rs) and Xcode Command Line Tools" >&2
  echo "       (xcode-select --install), then re-run." >&2
  exit 1
fi

# 2. Resolve the source dir (IN_REPO vs clone at ~/.cooee).
if [ -f install.sh ] && [ -f src-tauri/tauri.conf.json ]; then
  SRC="$(pwd)"
  echo "→ building from the current cooee checkout: $SRC"
else
  if [ ! -e "$INSTALL_DIR" ]; then
    echo "→ cloning cooee into $INSTALL_DIR"
    git clone "$REPO_URL" "$INSTALL_DIR"
  elif [ -d "$INSTALL_DIR/.git" ]; then
    echo "→ updating cooee clone in $INSTALL_DIR"
    git -C "$INSTALL_DIR" pull --ff-only
  else
    echo "cooee: $INSTALL_DIR exists but is not a git clone — move it aside and re-run." >&2
    exit 1
  fi
  SRC="$INSTALL_DIR"
fi

# 3. Tauri CLI backstop — a source build needs `cargo tauri`; ship it as a cargo global.
if ! command -v cargo-tauri >/dev/null 2>&1; then
  echo "→ installing the Tauri CLI (cargo install tauri-cli — this takes a while)"
  cargo install tauri-cli --version '^2' --locked
fi

# 4. Build the release bundle.
cd "$SRC"
echo "→ building release bundle (this takes a few minutes)"
( cd src-tauri && cargo tauri build )

# 5. Install the built app into /Applications.
bash scripts/install-app.sh "target/release/bundle/macos/cooee.app"

# 6. Seed the user config from the example (never overwrite an existing one).
mkdir -p "$HOME/.config/cooee"
if [ ! -f "$HOME/.config/cooee/config.toml" ]; then
  cp examples/config.toml "$HOME/.config/cooee/config.toml"
  echo "→ seeded ~/.config/cooee/config.toml from the example"
else
  echo "→ ~/.config/cooee/config.toml already exists — left untouched"
fi

echo ""
echo "✓ cooee installed to /Applications/cooee.app"
echo "  Edit ~/.config/cooee/config.toml, then launch cooee."
echo "  Update any time by re-running this installer (it git-pulls + rebuilds)."
