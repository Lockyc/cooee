You are installing or updating **cooee** — a macOS menu-bar app that runs Google Meet in its
own window and floats the other people's faces in an always-on-top window you can snap to where
you look.

GitHub: `https://github.com/lockyc/cooee`

Build it from source and install `cooee.app` to `/Applications`. Source lives in a
persistent clone at `~/.cooee`; updates are `git pull` + rebuild.

---

## Steps

### 1. Detect context

Check whether the current working directory is a cooee checkout:

```bash
[ -f install.sh ] && [ -f src-tauri/tauri.conf.json ] && echo "IN_REPO" || echo "NOT_IN_REPO"
```

**If IN_REPO:** you will run the local `install.sh` in step 4 (it builds this checkout).
**If NOT_IN_REPO:** you will run the published installer over curl in step 4 (it manages a `~/.cooee` clone).

### 2. Check prerequisites and offer to install

Probe each build prerequisite:

```bash
command -v git         >/dev/null 2>&1 && echo "git: ok"       || echo "git: MISSING"
command -v cargo       >/dev/null 2>&1 && echo "cargo: ok"     || echo "cargo: MISSING"
command -v cargo-tauri >/dev/null 2>&1 && echo "tauri-cli: ok" || echo "tauri-cli: MISSING"
xcode-select -p        >/dev/null 2>&1 && echo "clt: ok"       || echo "clt: MISSING"
command -v brew        >/dev/null 2>&1 && echo "brew: ok"      || echo "brew: MISSING"
```

For each MISSING prerequisite (other than brew), use AskUserQuestion to offer to install it.
Only run an install command on confirmation:

- **Xcode Command Line Tools** (also provides `git`): `xcode-select --install`
  (opens a GUI installer — tell the user to finish it, then continue).
- **Rust** (`cargo`): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y`,
  then advise sourcing `~/.cargo/env` or restarting the shell.
- **Tauri CLI** (`cargo-tauri`): `cargo install tauri-cli --version '^2' --locked`
  (compiles the CLI — takes a few minutes). `install.sh` backstops this too, so it
  is safe to skip here and let the core install handle it.

cooee has **no Node/npm dependency** — do not probe for it. If a prerequisite the user
declined to install is still missing (other than the Tauri CLI, which `install.sh`
installs anyway), warn that `install.sh` will refuse to build until it is present, and
ask whether to continue anyway.

### 3. Probe current state

For smarter messaging:

```bash
[ -d /Applications/cooee.app ] && echo "app: installed" || echo "app: absent"
if [ ! -e ~/.cooee ]; then echo "src: fresh";
elif [ -d ~/.cooee/.git ]; then echo "src: clone";
else echo "src: not-a-clone"; fi
pgrep -f "/Applications/cooee.app/" >/dev/null && echo "running: yes" || echo "running: no"
```

If `src: not-a-clone`, tell the user `~/.cooee` exists but is not a git clone; `install.sh`
will refuse to touch it. They must move it aside before continuing (NOT_IN_REPO path only).

### 4. Run the core install

**If IN_REPO:**

```bash
PATH="$HOME/.cargo/bin:$PATH" bash install.sh
```

**If NOT_IN_REPO:**

```bash
curl -fsSL https://raw.githubusercontent.com/lockyc/cooee/main/install.sh | PATH="$HOME/.cargo/bin:$PATH" bash
```

(The `PATH="$HOME/.cargo/bin:$PATH"` prefix ensures a Rust toolchain / Tauri CLI you may
have just installed via rustup/cargo in step 2 is found — a fresh shell won't have picked
up rustup's profile changes yet.)

IN_REPO builds the current checkout; NOT_IN_REPO clones/updates `~/.cooee` and builds from it.
Both back the Tauri CLI install if absent, run `cargo tauri build`, install the app to
`/Applications/cooee.app`, and seed `~/.config/cooee/config.toml` if absent. The build takes
a few minutes. **If it fails, show the full output and stop** — do not run later steps.

### 5. Configure

`install.sh` has already seeded `~/.config/cooee/config.toml` from the example if it was
absent. Use AskUserQuestion to offer to open it for editing now:

- **Open in editor** → `open -e ~/.config/cooee/config.toml`
- **Reveal in Finder** → `open -R ~/.config/cooee/config.toml`
- **Skip** — leave it for later.

The config lists `[[profile]]` blocks, one per Google sign-in (`name`, plus the `account` email
that a Meet link's `authuser` is matched against), the faces-window `[[size]]` steps, and
`[hotkeys]`. The example's two profiles use `example.com` accounts: replace them with the user's
own. cooee reads the config at launch, so restart it after an edit.

### 6. Offer to launch

Use AskUserQuestion: **"Launch cooee now?"**

- **Launch** → `open /Applications/cooee.app`
- **Not now** — skip.

### 7. Self-install this command

So `/cooee:install` is available in future Claude Code sessions:

```bash
mkdir -p ~/.claude/commands/cooee
```

Copy `install.md` verbatim to `~/.claude/commands/cooee/install.md`. Source it from the
cwd checkout if IN_REPO (`.claude/commands/cooee/install.md`), otherwise from
`~/.cooee/.claude/commands/cooee/install.md` (present after step 4 cloned it).

### 8. Summary

Read the version from `src-tauri/Cargo.toml` (the `version` field) — use the cwd
checkout if IN_REPO, else `~/.cooee/src-tauri/Cargo.toml`.

Print:

**Installed**
- cooee vX.Y.Z → `/Applications/cooee.app` ✓
- Source clone → `~/.cooee` (NOT_IN_REPO) or "built from checkout" (IN_REPO)
- Config → `~/.config/cooee/config.toml` (seeded from example / already existed)

**Next steps**
- Edit `~/.config/cooee/config.toml`: set your profiles' names and Google accounts.
- Launch cooee, pick **Open Meet** in its menu-bar menu, and sign in once per profile.
- Update any time by re-running `/cooee:install` (or `curl … | bash`) — it git-pulls and rebuilds.
