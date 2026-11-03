//! cooee's platform-neutral config: profiles, faces-window sizes and hotkeys, plus Meet link
//! parsing ([`link`]), spot geometry ([`spots`]) and the link-router contract ([`deeplink`]). No
//! Tauri or macOS dependencies, so it unit-tests standalone.

pub mod deeplink;
pub mod link;
pub mod spots;

pub use link::{parse_meet_link, profile_for_link, MeetLink, ProfilePick};

use serde::Deserialize;
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

/// The env var that overrides the config path (dev runs point it at `examples/config.toml`).
pub const CONFIG_ENV: &str = "COOEE_CONFIG";
/// The app's directory under `~/.config`.
pub const APP_DIR: &str = "cooee";
/// The profile every config has when it declares none.
pub const DEFAULT_PROFILE: &str = "default";

/// The resolved config path: `$COOEE_CONFIG`, else `~/.config/cooee/config.toml`.
pub fn resolve_config_path() -> PathBuf {
    config_core::paths::resolve_config_path(CONFIG_ENV, APP_DIR)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Never empty: a config with no `[[profile]]` gets [`DEFAULT_PROFILE`].
    pub profiles: Vec<Profile>,
    /// The faces-window sizes "cycle size" steps through, smallest first.
    pub sizes: Vec<Size>,
    pub hotkeys: Hotkeys,
}

/// One isolated Google sign-in. `account` is the email a Meet link's `authuser` is matched against.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub account: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// Global shortcuts, in `tauri-plugin-global-shortcut` accelerator syntax.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Hotkeys {
    pub talk: String,
    pub aside: String,
    pub cycle_size: String,
    pub toggle_faces: String,
    pub toggle_meet: String,
    pub save_talk: String,
    pub save_aside: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            talk: "Ctrl+Alt+Cmd+T".into(),
            aside: "Ctrl+Alt+Cmd+A".into(),
            cycle_size: "Ctrl+Alt+Cmd+S".into(),
            toggle_faces: "Ctrl+Alt+Cmd+F".into(),
            toggle_meet: "Ctrl+Alt+Cmd+M".into(),
            save_talk: "Ctrl+Alt+Cmd+Shift+T".into(),
            save_aside: "Ctrl+Alt+Cmd+Shift+A".into(),
        }
    }
}

fn default_sizes() -> Vec<Size> {
    vec![
        Size {
            width: 360,
            height: 225,
        },
        Size {
            width: 560,
            height: 350,
        },
        Size {
            width: 800,
            height: 500,
        },
    ]
}

/// The file as written: everything optional.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    profile: Vec<Profile>,
    #[serde(default)]
    size: Vec<Size>,
    #[serde(default)]
    hotkeys: Hotkeys,
}

#[derive(Debug)]
pub enum ConfigError {
    Read(PathBuf, std::io::Error),
    Toml(toml::de::Error),
    Invalid(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Read(p, e) => write!(f, "can't read {}: {e}", p.display()),
            ConfigError::Toml(e) => write!(f, "{e}"),
            ConfigError::Invalid(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for ConfigError {}

pub fn parse_and_validate(src: &str) -> Result<Config, ConfigError> {
    let raw: RawConfig = toml::from_str(src).map_err(ConfigError::Toml)?;
    let mut seen = HashSet::new();
    for p in &raw.profile {
        if p.name.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "a [[profile]] has an empty name".into(),
            ));
        }
        if !seen.insert(p.name.as_str()) {
            return Err(ConfigError::Invalid(format!(
                "profile name {:?} is used twice",
                p.name
            )));
        }
    }
    if let Some(s) = raw.size.iter().find(|s| s.width == 0 || s.height == 0) {
        return Err(ConfigError::Invalid(format!(
            "size {}×{} has a zero side",
            s.width, s.height
        )));
    }
    let profiles = if raw.profile.is_empty() {
        vec![Profile {
            name: DEFAULT_PROFILE.into(),
            account: None,
        }]
    } else {
        raw.profile
    };
    let mut sizes = if raw.size.is_empty() {
        default_sizes()
    } else {
        raw.size
    };
    sizes.sort_by_key(|s| s.width);
    Ok(Config {
        profiles,
        sizes,
        hotkeys: raw.hotkeys,
    })
}

/// Load `path`; a missing file is an empty config (all defaults), so a fresh install just works.
pub fn load_config(path: &Path) -> Result<Config, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(src) => parse_and_validate(&src),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => parse_and_validate(""),
        Err(e) => Err(ConfigError::Read(path.to_path_buf(), e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_has_default_profile() {
        let cfg = parse_and_validate("").unwrap();
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(cfg.profiles[0].name, "default");
        assert_eq!(cfg.profiles[0].account, None);
        assert_eq!(
            cfg.sizes,
            vec![
                Size {
                    width: 360,
                    height: 225
                },
                Size {
                    width: 560,
                    height: 350
                },
                Size {
                    width: 800,
                    height: 500
                },
            ]
        );
        assert_eq!(cfg.hotkeys.talk, "Ctrl+Alt+Cmd+T");
        assert_eq!(cfg.hotkeys.save_aside, "Ctrl+Alt+Cmd+Shift+A");
    }

    #[test]
    fn profiles_parse_in_order() {
        let cfg = parse_and_validate(
            "[[profile]]\nname = \"work\"\naccount = \"me@example.com\"\n\n[[profile]]\nname = \"home\"\n",
        )
        .unwrap();
        let names: Vec<_> = cfg.profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["work", "home"]);
        assert_eq!(cfg.profiles[0].account.as_deref(), Some("me@example.com"));
    }

    #[test]
    fn unknown_key_is_an_error() {
        assert!(parse_and_validate("colour = \"red\"\n").is_err());
        assert!(parse_and_validate("[[profile]]\nname = \"a\"\nemail = \"x\"\n").is_err());
    }

    #[test]
    fn duplicate_profile_name_is_an_error() {
        let err = parse_and_validate("[[profile]]\nname = \"a\"\n[[profile]]\nname = \"a\"\n")
            .unwrap_err();
        assert!(err.to_string().contains("\"a\""), "{err}");
    }

    #[test]
    fn empty_profile_name_is_an_error() {
        assert!(parse_and_validate("[[profile]]\nname = \"\"\n").is_err());
    }

    #[test]
    fn zero_size_is_an_error() {
        assert!(parse_and_validate("[[size]]\nwidth = 0\nheight = 100\n").is_err());
    }

    #[test]
    fn hotkeys_override_individually() {
        let cfg = parse_and_validate("[hotkeys]\ntalk = \"Cmd+F1\"\n").unwrap();
        assert_eq!(cfg.hotkeys.talk, "Cmd+F1");
        assert_eq!(cfg.hotkeys.aside, "Ctrl+Alt+Cmd+A");
    }

    #[test]
    fn missing_file_loads_defaults() {
        let cfg = load_config(std::path::Path::new("/nonexistent/cooee/config.toml")).unwrap();
        assert_eq!(cfg.profiles[0].name, "default");
    }
}
