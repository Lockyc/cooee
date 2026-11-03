//! `cooee://join` links from link routers (Manicule), and the profiles manifest they read. The
//! links arrive as `RunEvent::Opened` (Info.plist declares the scheme), at launch or later.

use tauri::{AppHandle, Manager};

use cooee_config::deeplink::{parse_join, profiles_manifest};
use cooee_config::{profile_for_link, Config, ProfilePick};

use crate::{meet, tray};

pub fn handle_url(app: &AppHandle, url: &str) {
    let Some(req) = parse_join(url) else {
        eprintln!("cooee: ignoring link {url}");
        return;
    };
    let cfg = app.state::<Config>();
    let named = req
        .profile
        .as_deref()
        .and_then(|n| cfg.profiles.iter().find(|p| p.name == n));
    let pick = named.map_or_else(|| profile_for_link(&cfg, &req.link), ProfilePick::Profile);
    match pick {
        ProfilePick::Profile(p) => meet::open_meet(app, p, &req.link.url),
        ProfilePick::Ask => tray::ask_profile(app, &cfg, req.link.url),
    }
}

/// `<app data>/profiles.json`: `{"version":1,"profiles":[{"name":…,"account":…|null}]}`.
pub fn write_manifest(app: &AppHandle) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("profiles.json");
    if let Err(e) = std::fs::write(&path, profiles_manifest(&app.state::<Config>())) {
        eprintln!("cooee: can't write {}: {e}", path.display());
    }
}
