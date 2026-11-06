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
const FACES: &str = "faces:"; // + spots action name
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
    // The hotkeys' actions, for when a shortcut clashes or is forgotten.
    let faces = Submenu::with_id(app, "faces", "Faces Window", true)?;
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
    } else if let Some(action) = id.strip_prefix(FACES) {
        crate::spots::run_named(app, action);
    } else if id == SNAPSHOT {
        match meet::meet_window(app) {
            Some(w) => {
                app.state::<meet::MeetState>().request_snapshot();
                let _ = w.eval("window.__cooee && window.__cooee.dump()");
            }
            None => tell(app, "Open a Meet call first, then save the snapshot."),
        }
    }
}

fn set_clipboard(text: &str) {
    use std::io::Write;
    if let Ok(mut child) = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
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

/// The dialog's cancel button: "Cancel", unless a profile has that name.
fn cancel_label(names: &[String]) -> String {
    ["Cancel", "Don't Join", "Not Now"]
        .into_iter()
        .find(|l| !names.iter().any(|n| n == l))
        .unwrap_or("Cancel")
        .to_string()
}

/// The profile a two-profile dialog's answer picks; the cancel button picks none.
fn chosen_profile(res: MessageDialogResult, names: &[String]) -> Option<String> {
    match res {
        MessageDialogResult::Custom(label) => names.iter().find(|n| **n == label).cloned(),
        MessageDialogResult::Yes => names.first().cloned(),
        MessageDialogResult::No => names.get(1).cloned(),
        _ => None,
    }
}

/// Ask which profile joins `url` (a link whose account matched no single profile).
pub fn ask_profile(app: &AppHandle, cfg: &Config, url: Url) {
    let names: Vec<String> = cfg.profiles.iter().map(|p| p.name.clone()).collect();
    let buttons = match names.as_slice() {
        [a, b] => {
            MessageDialogButtons::YesNoCancelCustom(a.clone(), b.clone(), cancel_label(&names))
        }
        _ => {
            // A dialog holds at most two choices: hand the link to the menu's per-profile join.
            set_clipboard(url.as_str());
            tell(
                app,
                "The link is on the clipboard: pick a profile under Join Clipboard Link In.",
            );
            return;
        }
    };
    let app2 = app.clone();
    app.dialog()
        .message("Which profile should join this meeting?")
        .title("cooee")
        .buttons(buttons)
        .show_with_result(move |res| {
            let Some(name) = chosen_profile(res, &names) else {
                return;
            };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn names(a: &str, b: &str) -> Vec<String> {
        vec![a.into(), b.into()]
    }

    #[test]
    fn cancel_button_never_shares_a_profile_name() {
        assert_eq!(cancel_label(&names("work", "home")), "Cancel");
        assert_ne!(cancel_label(&names("Cancel", "home")), "Cancel");
    }

    #[test]
    fn a_profile_named_cancel_is_not_joined_by_cancelling() {
        let n = names("Cancel", "home");
        let cancel = cancel_label(&n);
        assert_eq!(
            chosen_profile(MessageDialogResult::Custom(cancel), &n),
            None
        );
        assert_eq!(
            chosen_profile(MessageDialogResult::Custom("Cancel".into()), &n).as_deref(),
            Some("Cancel")
        );
        assert_eq!(
            chosen_profile(MessageDialogResult::Custom("home".into()), &n).as_deref(),
            Some("home")
        );
        assert_eq!(chosen_profile(MessageDialogResult::Cancel, &n), None);
    }
}
