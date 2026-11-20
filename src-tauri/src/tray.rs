//! The menu-bar item: cooee's only always-present surface.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use url::Url;

use cooee_config::link::MEET_HOST;
use cooee_config::{parse_meet_link, profile_for_link, Config, Profile, ProfilePick};

use crate::meet::{self, HOME_URL};
use crate::screenshare::Target;

const JOIN: &str = "join-clipboard";
const OPEN: &str = "open:"; // + profile name
const JOIN_IN: &str = "join-in:"; // + profile name
const SNAPSHOT: &str = "snapshot";
const FACES: &str = "faces:"; // + spots action name
const QUIT: &str = "quit";
const SHARE_WINDOW: &str = "share-opens:window";
const SHARE_SCREEN: &str = "share-opens:screen";

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
    // The hotkeys' actions, for when a shortcut clashes or is forgotten.
    let faces = Submenu::with_id(app, "faces", "Windows", true)?;
    for (name, label) in crate::spots::MENU_ACTIONS {
        faces.append(&MenuItem::with_id(
            app,
            format!("{FACES}{name}"),
            label,
            true,
            None::<&str>,
        )?)?;
    }
    menu.append(&faces)?;
    // What Meet's Share button opens: a check on the current choice.
    let target = crate::screenshare::target();
    let share = Submenu::with_id(app, "share-opens", "Share Opens", true)?;
    let window = CheckMenuItem::with_id(
        app,
        SHARE_WINDOW,
        "Window Picker",
        true,
        target == Target::Window,
        None::<&str>,
    )?;
    let screen = CheckMenuItem::with_id(
        app,
        SHARE_SCREEN,
        "Entire Screen Picker",
        true,
        target == Target::Screen,
        None::<&str>,
    )?;
    share.append(&window)?;
    share.append(&screen)?;
    menu.append(&share)?;
    app.manage(ShareItems { window, screen });
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

/// The Share Opens items, kept to move the check when the choice changes.
struct ShareItems {
    window: CheckMenuItem<tauri::Wry>,
    screen: CheckMenuItem<tauri::Wry>,
}

fn set_share_target(app: &AppHandle, t: Target) {
    crate::screenshare::set_target(app, t);
    let items = app.state::<ShareItems>();
    let _ = items.window.set_checked(t == Target::Window);
    let _ = items.screen.set_checked(t == Target::Screen);
}

fn on_menu(app: &AppHandle, id: &str) {
    let cfg = app.state::<Config>();
    if id == QUIT {
        app.exit(0);
    } else if id == SHARE_WINDOW {
        set_share_target(app, Target::Window);
    } else if id == SHARE_SCREEN {
        set_share_target(app, Target::Screen);
    } else if id == JOIN {
        join_from_clipboard(app, &cfg, None);
    } else if let Some(name) = id.strip_prefix(JOIN_IN) {
        join_from_clipboard(app, &cfg, cfg.profiles.iter().find(|p| p.name == name));
    } else if let Some(name) = id.strip_prefix(OPEN) {
        if let Some(p) = cfg.profiles.iter().find(|p| p.name == name) {
            meet::open_meet(app, p, &Url::parse(HOME_URL).unwrap());
        }
    } else if let Some(action) = id.strip_prefix(FACES) {
        crate::spots::run_named(app, action);
    } else if id == SNAPSHOT {
        // The adapter runs only on Meet's own host; anywhere else the dump would never come
        // and the request would stay armed for an unrequested one.
        let on_meet = |w: &tauri::WebviewWindow| {
            w.url().ok().and_then(|u| u.host_str().map(|h| h == MEET_HOST)) == Some(true)
        };
        match meet::meet_window(app).filter(on_meet) {
            Some(w) => {
                app.state::<meet::MeetState>().request_snapshot();
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
        ProfilePick::Ask => crate::chooser::ask(app, cfg, link.url),
    }
}

fn tell(app: &AppHandle, message: &str) {
    app.dialog().message(message).title("cooee").show(|_| {});
}
