//! The menu-bar item: cooee's only always-present surface.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogResult};
use url::Url;

use cooee_config::{parse_meet_link, profile_for_link, Config, Profile, ProfilePick};

use crate::meet::{self, HOME_URL};

const JOIN: &str = "join-clipboard";
const OPEN: &str = "open:"; // + profile name
const JOIN_IN: &str = "join-in:"; // + profile name
const SNAPSHOT: &str = "snapshot";
const QUIT: &str = "quit";

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let cfg = app.state::<Config>();
    let join = MenuItem::with_id(
        app,
        JOIN,
        "Join Meet Link from Clipboard",
        true,
        None::<&str>,
    )?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(app, &[&join])?;
    if cfg.profiles.len() > 1 {
        let join_in = Submenu::with_id(app, "join-in", "Join Clipboard Link In", true)?;
        for p in &cfg.profiles {
            join_in.append(&MenuItem::with_id(
                app,
                format!("{JOIN_IN}{}", p.name),
                &p.name,
                true,
                None::<&str>,
            )?)?;
        }
        menu.append(&join_in)?;
    }
    menu.append(&sep()?)?;
    for p in &cfg.profiles {
        let label = if cfg.profiles.len() > 1 {
            format!("Open Meet — {}", p.name)
        } else {
            "Open Meet".into()
        };
        menu.append(&MenuItem::with_id(
            app,
            format!("{OPEN}{}", p.name),
            label,
            true,
            None::<&str>,
        )?)?;
    }
    menu.append(&sep()?)?;
    let diagnostics = Submenu::with_id(app, "diagnostics", "Diagnostics", true)?;
    diagnostics.append(&MenuItem::with_id(
        app,
        SNAPSHOT,
        "Save Meet Layout Snapshot",
        true,
        None::<&str>,
    )?)?;
    menu.append(&diagnostics)?;
    menu.append(&MenuItem::with_id(
        app,
        QUIT,
        "Quit cooee",
        true,
        None::<&str>,
    )?)?;

    TrayIconBuilder::with_id("cooee")
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .tooltip("cooee")
        .menu(&menu)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .build(app)?;
    Ok(())
}

fn on_menu(app: &AppHandle, id: &str) {
    let cfg = app.state::<Config>();
    if id == QUIT {
        app.exit(0);
    } else if id == JOIN {
        join_from_clipboard(app, &cfg, None);
    } else if let Some(name) = id.strip_prefix(JOIN_IN) {
        join_from_clipboard(app, &cfg, cfg.profiles.iter().find(|p| p.name == name));
    } else if let Some(name) = id.strip_prefix(OPEN) {
        if let Some(p) = cfg.profiles.iter().find(|p| p.name == name) {
            meet::open_meet(app, p, &Url::parse(HOME_URL).unwrap());
        }
    } else if id == SNAPSHOT {
        match meet::meet_window(app) {
            Some(w) => {
                let _ = w.eval("window.__cooee && window.__cooee.dump()");
            }
            None => tell(app, "Open a Meet call first, then save the snapshot."),
        }
    }
}

fn clipboard_text() -> String {
    std::process::Command::new("pbpaste")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Join the clipboard's Meet link in `forced`, else the profile its account matches, else ask.
fn join_from_clipboard(app: &AppHandle, cfg: &Config, forced: Option<&Profile>) {
    let Some(link) = parse_meet_link(&clipboard_text()) else {
        tell(app, "There's no Google Meet link on the clipboard.");
        return;
    };
    let pick = forced
        .map(ProfilePick::Profile)
        .unwrap_or_else(|| profile_for_link(cfg, &link));
    match pick {
        ProfilePick::Profile(p) => meet::open_meet(app, p, &link.url),
        ProfilePick::Ask => ask_profile(app, cfg, link.url),
    }
}

fn ask_profile(app: &AppHandle, cfg: &Config, url: Url) {
    let names: Vec<String> = cfg.profiles.iter().map(|p| p.name.clone()).collect();
    let buttons = match names.as_slice() {
        [a, b] => MessageDialogButtons::YesNoCancelCustom(a.clone(), b.clone(), "Cancel".into()),
        _ => {
            tell(app, "Pick a profile under Join Clipboard Link In.");
            return;
        }
    };
    let app2 = app.clone();
    app.dialog()
        .message("Which profile should join this meeting?")
        .title("cooee")
        .buttons(buttons)
        .show_with_result(move |res| {
            let chosen = match res {
                MessageDialogResult::Custom(label) => Some(label),
                MessageDialogResult::Yes => names.first().cloned(),
                MessageDialogResult::No => names.get(1).cloned(),
                _ => None,
            };
            let Some(name) = chosen else { return };
            let app3 = app2.clone();
            let _ = app2.run_on_main_thread(move || {
                let cfg = app3.state::<Config>();
                if let Some(p) = cfg.profiles.iter().find(|p| p.name == name) {
                    meet::open_meet(&app3, p, &url);
                }
            });
        });
}

fn tell(app: &AppHandle, message: &str) {
    app.dialog().message(message).title("cooee").show(|_| {});
}
