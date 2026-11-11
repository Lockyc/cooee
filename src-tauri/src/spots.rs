//! Where popups go: the faces window's spots, the global hotkeys and tray actions that move and
//! toggle popups, and the controls and share windows' last places. Geometry is in logical points, the
//! one coordinate space macOS screens share; each monitor's physical rect is divided by its own
//! scale. A spot is the window's outer origin plus its inner size, matching what
//! `set_position`/`set_size` apply. Moving or resizing the faces window by hand writes nothing
//! (only Save does); moving the controls or share window is remembered.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use cooee_config::spots::{
    clamp_to_screens, cycle_size, default_aside, default_controls, default_share, default_talk,
    Rect, Spots,
};
use cooee_config::Config;

use crate::meet::{self, MeetState, HOME_URL};
use crate::popups::{self, Popup};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Action {
    Talk,
    Aside,
    CycleSize,
    ToggleFaces,
    ToggleControls,
    ToggleMeet,
    SaveTalk,
    SaveAside,
}

#[derive(Default)]
pub struct Hotkeys(Mutex<HashMap<u32, Action>>);

#[derive(Clone, Copy)]
struct Screen {
    /// The screen's work area, in points.
    rect: Rect,
}

/// A monitor's physical rect in points: tao reports each monitor's physical origin as its point
/// origin multiplied by that monitor's own scale.
fn screen_in_points(pos: PhysicalPosition<i32>, size: PhysicalSize<u32>, scale: f64) -> Rect {
    Rect {
        x: pos.x as f64 / scale,
        y: pos.y as f64 / scale,
        w: size.width as f64 / scale,
        h: size.height as f64 / scale,
    }
}

/// A window's spot rect in points: its outer origin with its inner (content) size.
fn window_rect(outer: PhysicalPosition<i32>, inner: PhysicalSize<u32>, scale: f64) -> Rect {
    screen_in_points(outer, inner, scale)
}

fn screens(app: &AppHandle) -> Vec<Screen> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let a = m.work_area();
            Screen {
                rect: screen_in_points(a.position, a.size, m.scale_factor()),
            }
        })
        .collect()
}

/// The screen spots default to: the largest in points (the TV).
fn main_screen(screens: &[Screen]) -> Option<Screen> {
    screens
        .iter()
        .copied()
        .max_by(|a, b| (a.rect.w * a.rect.h).total_cmp(&(b.rect.w * b.rect.h)))
}

fn spots_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("spots.json"))
}

fn load_spots(app: &AppHandle) -> Spots {
    spots_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| Spots::from_json(&s))
        .unwrap_or_default()
}

fn store_spots(app: &AppHandle, spots: &Spots) {
    if let Some(p) = spots_path(app) {
        let _ = p.parent().map(std::fs::create_dir_all);
        if let Err(e) = std::fs::write(&p, spots.to_json()) {
            eprintln!("cooee: can't save spots to {}: {e}", p.display());
        }
    }
}

/// The rect a spot snaps to: the saved one, else the default on the main screen; either way
/// clamped onto a connected screen.
fn spot_rect(app: &AppHandle, talk: bool) -> Option<Rect> {
    let all = screens(app);
    let main = main_screen(&all)?;
    let cfg = app.state::<Config>();
    let saved = load_spots(app);
    let r = if talk {
        saved
            .talk
            .unwrap_or_else(|| default_talk(main.rect, cfg.sizes[cfg.sizes.len() / 2]))
    } else {
        saved
            .aside
            .unwrap_or_else(|| default_aside(main.rect, cfg.sizes[0]))
    };
    let rects: Vec<Rect> = all.iter().map(|s| s.rect).collect();
    Some(clamp_to_screens(r, &rects))
}

/// Move first, then size, so the size applies with the target screen's scale.
fn place(w: &tauri::WebviewWindow, r: Rect) {
    let _ = w.set_position(LogicalPosition::new(r.x, r.y));
    let _ = w.set_size(LogicalSize::new(r.w, r.h));
}

fn current_rect(w: &tauri::WebviewWindow) -> Option<Rect> {
    Some(window_rect(
        w.outer_position().ok()?,
        w.inner_size().ok()?,
        w.scale_factor().ok()?,
    ))
}

/// Size and place a newly opened popup: the faces window at Talk, the others where they were
/// last left (else their default on the main screen), clamped onto a connected screen.
pub fn place_popup(app: &AppHandle, w: &tauri::WebviewWindow, kind: Popup) {
    let r = match kind {
        Popup::Faces => spot_rect(app, true),
        Popup::Controls | Popup::Share => {
            let all = screens(app);
            main_screen(&all).map(|main| {
                let saved = load_spots(app);
                let r = if kind == Popup::Controls {
                    saved
                        .controls
                        .unwrap_or_else(|| default_controls(main.rect))
                } else {
                    saved.share.unwrap_or_else(|| default_share(main.rect))
                };
                let rects: Vec<Rect> = all.iter().map(|s| s.rect).collect();
                clamp_to_screens(r, &rects)
            })
        }
    };
    if let Some(r) = r {
        place(w, r);
    }
}

/// The controls or share window moved or resized: it reopens there. The faces window keeps its
/// explicit Save Talk / Save Aside.
pub fn remember_popup(app: &AppHandle, w: &tauri::WebviewWindow, kind: Popup) {
    if kind == Popup::Faces {
        return;
    }
    let Some(r) = current_rect(w) else {
        return;
    };
    let mut spots = load_spots(app);
    if kind == Popup::Controls {
        spots.controls = Some(r);
    } else {
        spots.share = Some(r);
    }
    store_spots(app, &spots);
}

/// Show a hidden popup, hide a shown one, or — when its window is gone mid-call — have the page
/// open it again.
fn toggle_popup(app: &AppHandle, kind: Popup) {
    match popups::window(app, kind) {
        Some(w) if w.is_visible().unwrap_or(false) => {
            let _ = w.hide();
        }
        Some(w) => {
            let _ = w.show();
        }
        None if app.state::<MeetState>().call().in_call => {
            if let Some(m) = meet::meet_window(app) {
                let _ = m.eval(format!(
                    "window.__cooeePopups && window.__cooeePopups.open('{}')",
                    kind.name()
                ));
            }
        }
        None => {}
    }
}

/// The tray's Windows items: (action name, label).
pub const MENU_ACTIONS: [(&str, &str); 8] = [
    ("talk", "Snap to Talk"),
    ("aside", "Snap to Aside"),
    ("cycle_size", "Cycle Size"),
    ("toggle_faces", "Show/Hide Faces"),
    ("toggle_controls", "Show/Hide Controls"),
    ("toggle_meet", "Show/Hide Meet"),
    ("save_talk", "Save as Talk"),
    ("save_aside", "Save as Aside"),
];

fn action_named(name: &str) -> Option<Action> {
    Some(match name {
        "talk" => Action::Talk,
        "aside" => Action::Aside,
        "cycle_size" => Action::CycleSize,
        "toggle_faces" => Action::ToggleFaces,
        "toggle_controls" => Action::ToggleControls,
        "toggle_meet" => Action::ToggleMeet,
        "save_talk" => Action::SaveTalk,
        "save_aside" => Action::SaveAside,
        _ => return None,
    })
}

/// Run a [`MENU_ACTIONS`] action by name.
pub fn run_named(app: &AppHandle, name: &str) {
    if let Some(a) = action_named(name) {
        run(app, a);
    }
}

fn run(app: &AppHandle, action: Action) {
    let faces = popups::window(app, Popup::Faces);
    match action {
        Action::Talk | Action::Aside => {
            if let (Some(w), Some(r)) = (&faces, spot_rect(app, action == Action::Talk)) {
                place(w, r);
                let _ = w.show();
            }
        }
        Action::CycleSize => {
            if let (Some(w), Some(r)) = (&faces, faces.as_ref().and_then(current_rect)) {
                let sizes = &app.state::<Config>().sizes;
                let rects: Vec<Rect> = screens(app).iter().map(|s| s.rect).collect();
                place(w, clamp_to_screens(cycle_size(r, sizes), &rects));
            }
        }
        Action::SaveTalk | Action::SaveAside => {
            if let Some(r) = faces.as_ref().and_then(current_rect) {
                let mut spots = load_spots(app);
                if action == Action::SaveTalk {
                    spots.talk = Some(r);
                } else {
                    spots.aside = Some(r);
                }
                store_spots(app, &spots);
            }
        }
        Action::ToggleFaces => toggle_popup(app, Popup::Faces),
        Action::ToggleControls => toggle_popup(app, Popup::Controls),
        Action::ToggleMeet => match meet::meet_window(app) {
            Some(m) if m.is_visible().unwrap_or(false) => {
                let _ = m.hide();
            }
            Some(m) => {
                let _ = m.show();
                let _ = m.set_focus();
            }
            None => {
                let profile = app.state::<Config>().profiles[0].clone();
                meet::open_meet(app, &profile, &url::Url::parse(HOME_URL).unwrap());
            }
        },
    }
}

/// Register the config's hotkeys. One that can't be registered (a clash, a bad accelerator) is
/// reported by name; the rest still work.
pub fn register_hotkeys(app: &AppHandle) -> tauri::Result<()> {
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, shortcut, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                let action = app
                    .state::<Hotkeys>()
                    .0
                    .lock()
                    .unwrap()
                    .get(&shortcut.id())
                    .copied();
                if let Some(a) = action {
                    run(app, a);
                }
            })
            .build(),
    )?;
    let hk = app.state::<Config>().hotkeys.clone();
    let wanted = [
        (Action::Talk, "talk", hk.talk),
        (Action::Aside, "aside", hk.aside),
        (Action::CycleSize, "cycle_size", hk.cycle_size),
        (Action::ToggleFaces, "toggle_faces", hk.toggle_faces),
        (
            Action::ToggleControls,
            "toggle_controls",
            hk.toggle_controls,
        ),
        (Action::ToggleMeet, "toggle_meet", hk.toggle_meet),
        (Action::SaveTalk, "save_talk", hk.save_talk),
        (Action::SaveAside, "save_aside", hk.save_aside),
    ];
    let mut failed = Vec::new();
    for (action, name, accel) in wanted {
        let ok = accel
            .parse::<Shortcut>()
            .ok()
            .filter(|s| app.global_shortcut().register(*s).is_ok())
            .map(|s| {
                app.state::<Hotkeys>()
                    .0
                    .lock()
                    .unwrap()
                    .insert(s.id(), action)
            });
        if ok.is_none() {
            failed.push(format!("{name} ({accel})"));
        }
    }
    if !failed.is_empty() {
        app.dialog()
            .message(format!(
                "These shortcuts couldn't be set up: {}",
                failed.join(", ")
            ))
            .title("cooee")
            .show(|_| {});
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_dpi_screens_share_one_point_space() {
        // A retina laptop (scale 2) with a TV (scale 1.5) to its right. tao reports each
        // monitor's physical origin as its point origin × that monitor's own scale.
        let laptop = screen_in_points(
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(3024, 1964),
            2.0,
        );
        let tv = screen_in_points(
            PhysicalPosition::new(2268, 0),
            PhysicalSize::new(3840, 2160),
            1.5,
        );
        assert_eq!(
            laptop,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 1512.0,
                h: 982.0
            }
        );
        assert_eq!(
            tv,
            Rect {
                x: 1512.0,
                y: 0.0,
                w: 2560.0,
                h: 1440.0
            }
        );
    }

    #[test]
    fn main_screen_is_the_largest_in_points() {
        // In physical pixels the retina laptop (3024×1964) outranks a 1080p TV; in points it doesn't.
        let laptop = Screen {
            rect: screen_in_points(
                PhysicalPosition::new(0, 0),
                PhysicalSize::new(3024, 1964),
                2.0,
            ),
        };
        let tv = Screen {
            rect: screen_in_points(
                PhysicalPosition::new(1512, 0),
                PhysicalSize::new(1920, 1080),
                1.0,
            ),
        };
        assert_eq!(main_screen(&[laptop, tv]).unwrap().rect.w, 1920.0);
    }

    #[test]
    fn a_saved_rect_is_the_outer_origin_with_the_inner_size() {
        // Saving the outer size and restoring it as the inner size grew the window by its
        // title bar on every save-then-snap.
        let r = window_rect(
            PhysicalPosition::new(300, 200),
            PhysicalSize::new(1120, 700),
            2.0,
        );
        assert_eq!(
            r,
            Rect {
                x: 150.0,
                y: 100.0,
                w: 560.0,
                h: 350.0
            }
        );
    }
}

#[cfg(test)]
mod menu_tests {
    use super::*;

    #[test]
    fn every_menu_action_resolves() {
        for (name, _) in MENU_ACTIONS {
            assert!(action_named(name).is_some(), "{name}");
        }
    }

    #[test]
    fn the_controls_window_toggles_from_the_menu() {
        assert!(MENU_ACTIONS.contains(&("toggle_controls", "Show/Hide Controls")));
        assert_eq!(
            action_named("toggle_controls"),
            Some(Action::ToggleControls)
        );
    }
}
