//! Where popups go: the faces window's spots, the global hotkeys and tray actions that move and
//! toggle popups, the controls window's dock beside the faces window, and the share window's last
//! place. Geometry is in logical points, the one coordinate space macOS screens share; each
//! monitor's physical rect is divided by its own scale. A spot is the window's outer origin plus
//! its inner size, matching what `set_position`/`set_size` apply. Moving or resizing the faces
//! window by hand writes nothing (only Save does); moving the share window, and resizing the
//! controls window, is remembered.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use cooee_config::spots::{
    clamp_to_screens, cycle_size, default_aside, default_controls, default_share, default_talk,
    dock_controls, moved_by_hand, share_sized, share_tile, Rect, ShareSize, Spots, CONTROLS_SIZE,
};
use cooee_config::Config;

use crate::meet::{self, MeetState, HOME_URL};
use crate::popups::{self, Popup};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Talk,
    Aside,
    CycleSize,
    ToggleFaces,
    ToggleControls,
    ToggleShare,
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
    /// The macOS main display: the one carrying the menu bar, set in System Settings → Displays.
    primary: bool,
}

/// A monitor's physical rect in points: tao reports each monitor's physical origin as its point
/// origin multiplied by that monitor's own scale.
pub(crate) fn screen_in_points(
    pos: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    scale: f64,
) -> Rect {
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
    let primary = app.primary_monitor().ok().flatten().map(|m| *m.position());
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let a = m.work_area();
            Screen {
                rect: screen_in_points(a.position, a.size, m.scale_factor()),
                primary: primary == Some(*m.position()),
            }
        })
        .collect()
}

/// The screen spots default to: the macOS main display, else the first.
fn main_screen(screens: &[Screen]) -> Option<Screen> {
    screens
        .iter()
        .find(|s| s.primary)
        .or_else(|| screens.first())
        .copied()
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

/// Size and place a newly opened popup, clamped onto a connected screen: the faces window at
/// Talk, the controls window docked to it (else at its default, until the faces window opens),
/// the share window where it was last left (else centred on the main screen).
pub fn place_popup(app: &AppHandle, w: &tauri::WebviewWindow, kind: Popup) {
    let all = screens(app);
    let rects: Vec<Rect> = all.iter().map(|s| s.rect).collect();
    match kind {
        Popup::Faces => {
            if let Some(r) = spot_rect(app, true) {
                place(w, r);
            }
            dock_to_faces(app);
        }
        Popup::Controls => {
            let size = load_spots(app).controls_size.unwrap_or(CONTROLS_SIZE);
            let faces = popups::window(app, Popup::Faces).and_then(|f| current_rect(&f));
            let r = match faces {
                Some(f) => Some(dock_controls(f, size, &rects)),
                None => main_screen(&all)
                    .map(|m| clamp_to_screens(default_controls(m.rect, size), &rects)),
            };
            if let Some(r) = r {
                place_tracked(app, w, r);
            }
        }
        Popup::Share => {
            if let Some(main) = main_screen(&all) {
                let saved = load_spots(app).share;
                let r = clamp_to_screens(saved.unwrap_or_else(|| default_share(main.rect)), &rects);
                place_tracked(app, w, r);
            }
        }
    }
}

/// The faces window moved or resized: the controls window keeps its dock beside it, at whatever
/// size it has.
pub fn dock_to_faces(app: &AppHandle) {
    let Some(faces) = popups::window(app, Popup::Faces).and_then(|f| current_rect(&f)) else {
        return;
    };
    let Some(controls) = popups::window(app, Popup::Controls) else {
        return;
    };
    let size = current_rect(&controls).map_or(CONTROLS_SIZE, |r| (r.w, r.h));
    let rects: Vec<Rect> = screens(app).iter().map(|s| s.rect).collect();
    place_tracked(app, &controls, dock_controls(faces, size, &rects));
}

/// Where cooee last placed each controls and share window (by label), so the Moved/Resized events
/// that placement fires aren't remembered as the user's choice.
#[derive(Default)]
pub struct Placed(Mutex<HashMap<String, Rect>>);

fn place_tracked(app: &AppHandle, w: &tauri::WebviewWindow, r: Rect) {
    app.state::<Placed>()
        .0
        .lock()
        .unwrap()
        .insert(w.label().to_string(), r);
    place(w, r);
}

/// The window's current rect, when it differs from where cooee last placed it.
fn changed_by_hand(app: &AppHandle, w: &tauri::WebviewWindow) -> Option<Rect> {
    let r = current_rect(w)?;
    let placed = app
        .state::<Placed>()
        .0
        .lock()
        .unwrap()
        .get(w.label())
        .copied();
    moved_by_hand(r, placed).then_some(r)
}

/// The minimised share window (by label) and the rect Restore puts it back at. A new share's
/// window has a new label, so it opens unminimised.
#[derive(Default)]
pub struct ShareMinimised(Mutex<Option<(String, Rect)>>);

fn minimised_restore(app: &AppHandle, w: &tauri::WebviewWindow) -> Option<Rect> {
    match &*app.state::<ShareMinimised>().0.lock().unwrap() {
        Some((label, r)) if label == w.label() => Some(*r),
        _ => None,
    }
}

/// The share window's toolbar: `minimise` tucks it into its screen's corner as a thumbnail,
/// `restore` puts it back and focuses it, and a size name (`small`, `medium`, `large`, `fill`)
/// sizes it, restoring it first. Returns whether it is now minimised.
pub fn share_window(app: &AppHandle, action: &str) -> Result<bool, String> {
    let Some(w) = popups::window(app, Popup::Share) else {
        return Ok(false);
    };
    let current = current_rect(&w).ok_or("the share window has no position")?;
    let restore = minimised_restore(app, &w);
    let rects: Vec<Rect> = screens(app).iter().map(|s| s.rect).collect();
    let state = app.state::<ShareMinimised>();
    if action == "minimise" {
        if restore.is_none() {
            *state.0.lock().unwrap() = Some((w.label().to_string(), current));
            place_tracked(app, &w, share_tile(current, &rects));
        }
        return Ok(true);
    }
    let r = match (action, ShareSize::named(action)) {
        ("restore", _) => restore.unwrap_or(current),
        (_, Some(size)) => {
            let r = share_sized(restore.unwrap_or(current), size, &rects);
            let mut spots = load_spots(app);
            spots.share = Some(r);
            store_spots(app, &spots);
            r
        }
        _ => return Err(format!("unknown share window action {action:?}")),
    };
    *state.0.lock().unwrap() = None;
    place_tracked(app, &w, r);
    shell_core::focus::bring_forward(&w);
    Ok(false)
}

/// The share window moved or resized by hand: it reopens there. Dragging the minimised
/// thumbnail isn't a place to reopen at.
pub fn remember_share(app: &AppHandle, w: &tauri::WebviewWindow) {
    if minimised_restore(app, w).is_some() {
        return;
    }
    if let Some(r) = changed_by_hand(app, w) {
        let mut spots = load_spots(app);
        spots.share = Some(r);
        store_spots(app, &spots);
    }
}

/// The controls window resized by hand: it reopens at that size. Where it goes stays the dock's.
pub fn remember_controls_size(app: &AppHandle, w: &tauri::WebviewWindow) {
    if let Some(r) = changed_by_hand(app, w) {
        let mut spots = load_spots(app);
        spots.controls_size = Some((r.w, r.h));
        store_spots(app, &spots);
    }
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

/// Every action, in the tray's Windows order: (action, name, menu label). The name is the tray
/// item's id and what the hotkeys' error dialog calls it.
pub const ACTIONS: [(Action, &str, &str); 9] = [
    (Action::Talk, "talk", "Snap to Talk"),
    (Action::Aside, "aside", "Snap to Aside"),
    (Action::CycleSize, "cycle_size", "Cycle Size"),
    (Action::ToggleFaces, "toggle_faces", "Show/Hide Faces"),
    (
        Action::ToggleControls,
        "toggle_controls",
        "Show/Hide Controls",
    ),
    (Action::ToggleShare, "toggle_share", "Show/Hide Share"),
    (Action::ToggleMeet, "toggle_meet", "Show/Hide Meet"),
    (Action::SaveTalk, "save_talk", "Save as Talk"),
    (Action::SaveAside, "save_aside", "Save as Aside"),
];

fn action_named(name: &str) -> Option<Action> {
    ACTIONS
        .into_iter()
        .find(|&(_, n, _)| n == name)
        .map(|(a, _, _)| a)
}

/// The config's accelerator for `action`; `None` for an action with no hotkey.
fn accelerator(hk: &cooee_config::Hotkeys, action: Action) -> Option<&str> {
    Some(match action {
        Action::Talk => &hk.talk,
        Action::Aside => &hk.aside,
        Action::CycleSize => &hk.cycle_size,
        Action::ToggleFaces => &hk.toggle_faces,
        Action::ToggleControls => &hk.toggle_controls,
        Action::ToggleShare => return None,
        Action::ToggleMeet => &hk.toggle_meet,
        Action::SaveTalk => &hk.save_talk,
        Action::SaveAside => &hk.save_aside,
    })
}

/// Run an [`ACTIONS`] action by name.
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
        Action::ToggleShare => toggle_popup(app, Popup::Share),
        Action::ToggleMeet => match meet::meet_window(app) {
            Some(m) if m.is_visible().unwrap_or(false) => {
                let _ = m.hide();
            }
            Some(m) => shell_core::focus::bring_forward(&m),
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
    let mut failed = Vec::new();
    for (action, name, _) in ACTIONS {
        let Some(accel) = accelerator(&hk, action) else {
            continue;
        };
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
    fn main_screen_is_the_macos_main_display() {
        // Two identical TVs: size can't pick between them, and "largest" landed on the other one.
        let tv = |x: f64, primary| Screen {
            rect: Rect {
                x,
                y: 0.0,
                w: 3840.0,
                h: 2160.0,
            },
            primary,
        };
        let laptop = Screen {
            rect: Rect {
                x: -2056.0,
                y: 0.0,
                w: 2056.0,
                h: 1329.0,
            },
            primary: false,
        };
        let screens = [tv(0.0, true), laptop, tv(3840.0, false)];
        assert_eq!(main_screen(&screens).unwrap().rect.x, 0.0);
        assert_eq!(
            main_screen(&[laptop, tv(3840.0, false)]).unwrap().rect.x,
            -2056.0,
            "no main display reported: the first"
        );
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
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    const SCALES: [f64; 3] = [1.0, 1.5, 2.0];

    fn rect() -> impl Strategy<Value = Rect> {
        (-3000i32..6000, -1500i32..3000, 800i32..4000, 600i32..2400).prop_map(|(x, y, w, h)| Rect {
            x: x.into(),
            y: y.into(),
            w: w.into(),
            h: h.into(),
        })
    }

    /// 1 to 4 screens (identical ones included), exactly one of them the main display, listed
    /// in two independent orders.
    fn screens_in_two_orders() -> impl Strategy<Value = (Vec<(Rect, bool)>, Vec<(Rect, bool)>)> {
        prop::collection::vec(
            prop_oneof![
                rect(),
                Just(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 3840.0,
                    h: 2160.0
                })
            ],
            1..=4,
        )
        .prop_flat_map(|rects| {
            let n = rects.len();
            (Just(rects), 0..n)
        })
        .prop_map(|(rects, main)| {
            rects
                .into_iter()
                .enumerate()
                .map(|(i, rect)| (rect, i == main))
                .collect::<Vec<_>>()
        })
        .prop_flat_map(|s| (Just(s.clone()).prop_shuffle(), Just(s).prop_shuffle()))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn main_screen_ignores_monitor_order((a, b) in screens_in_two_orders()) {
            let screens = |v: &[(Rect, bool)]| {
                v.iter().map(|&(rect, primary)| Screen { rect, primary }).collect::<Vec<_>>()
            };
            let main = a.iter().find(|s| s.1).unwrap().0;
            for order in [screens(&a), screens(&b)] {
                let got = main_screen(&order).unwrap();
                prop_assert!(got.primary);
                prop_assert_eq!(got.rect, main);
            }
        }

        /// A rect placed in points, applied in a monitor's physical pixels (tao scales by that
        /// monitor's own factor) and read back through `window_rect`, is the rect placed.
        #[test]
        fn a_placed_rect_round_trips_through_physical_pixels(
            (x, y, w, h) in (-3000i32..6000, -1500i32..3000, 100i32..4000, 100i32..2400),
            scale in prop::sample::select(SCALES.to_vec()),
        ) {
            // Even points, so a 1.5 scale lands on whole pixels, as macOS's do.
            let placed = Rect { x: (2 * x).into(), y: (2 * y).into(), w: (2 * w).into(), h: (2 * h).into() };
            let px = |v: f64| (v * scale).round();
            let back = window_rect(
                PhysicalPosition::new(px(placed.x) as i32, px(placed.y) as i32),
                PhysicalSize::new(px(placed.w) as u32, px(placed.h) as u32),
                scale,
            );
            prop_assert_eq!(back, placed);
        }
    }
}

#[cfg(test)]
mod menu_tests {
    use super::*;

    #[test]
    fn the_controls_window_toggles_from_the_menu() {
        assert!(ACTIONS.contains(&(
            Action::ToggleControls,
            "toggle_controls",
            "Show/Hide Controls"
        )));
        assert_eq!(
            action_named("toggle_controls"),
            Some(Action::ToggleControls)
        );
    }

    #[test]
    fn a_hidden_share_window_comes_back_from_the_menu() {
        assert!(ACTIONS.contains(&(Action::ToggleShare, "toggle_share", "Show/Hide Share")));
        assert_eq!(action_named("toggle_share"), Some(Action::ToggleShare));
    }
}
