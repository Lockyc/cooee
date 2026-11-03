//! Moving the faces window between spots, and the global hotkeys + hover-button commands that do
//! it. Geometry is in physical pixels of the chosen screen; the configured sizes are points,
//! scaled by that screen's factor. Moving or resizing by hand writes nothing: only Save does.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use cooee_config::spots::{clamp_to_screens, cycle_size, default_aside, default_talk, Rect, Spots};
use cooee_config::{Config, Size};

use crate::faces::faces_window;
use crate::meet::{self, MeetState, HOME_URL};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Action {
    Talk,
    Aside,
    CycleSize,
    ToggleFaces,
    ToggleMeet,
    SaveTalk,
    SaveAside,
}

#[derive(Default)]
pub struct Hotkeys(Mutex<HashMap<u32, Action>>);

#[derive(Clone, Copy)]
struct Screen {
    rect: Rect,
    scale: f64,
}

fn screens(app: &AppHandle) -> Vec<Screen> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let a = m.work_area();
            Screen {
                rect: Rect {
                    x: a.position.x as f64,
                    y: a.position.y as f64,
                    w: a.size.width as f64,
                    h: a.size.height as f64,
                },
                scale: m.scale_factor(),
            }
        })
        .collect()
}

/// The screen spots default to: the largest (the TV).
fn main_screen(screens: &[Screen]) -> Option<Screen> {
    screens
        .iter()
        .copied()
        .max_by(|a, b| (a.rect.w * a.rect.h).total_cmp(&(b.rect.w * b.rect.h)))
}

fn scaled(size: Size, scale: f64) -> Size {
    Size {
        width: (size.width as f64 * scale).round() as u32,
        height: (size.height as f64 * scale).round() as u32,
    }
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
        saved.talk.unwrap_or_else(|| {
            let mid = cfg.sizes[cfg.sizes.len() / 2];
            default_talk(main.rect, scaled(mid, main.scale))
        })
    } else {
        saved
            .aside
            .unwrap_or_else(|| default_aside(main.rect, scaled(cfg.sizes[0], main.scale)))
    };
    let rects: Vec<Rect> = all.iter().map(|s| s.rect).collect();
    Some(clamp_to_screens(r, &rects))
}

fn place(w: &tauri::WebviewWindow, r: Rect) {
    let _ = w.set_size(PhysicalSize::new(r.w.round() as u32, r.h.round() as u32));
    let _ = w.set_position(PhysicalPosition::new(
        r.x.round() as i32,
        r.y.round() as i32,
    ));
}

fn current_rect(w: &tauri::WebviewWindow) -> Option<Rect> {
    let p = w.outer_position().ok()?;
    let s = w.outer_size().ok()?;
    Some(Rect {
        x: p.x as f64,
        y: p.y as f64,
        w: s.width as f64,
        h: s.height as f64,
    })
}

/// Put a newly opened faces window at Talk.
pub fn place_at_talk(app: &AppHandle) {
    if let (Some(w), Some(r)) = (faces_window(app), spot_rect(app, true)) {
        place(&w, r);
    }
}

fn run(app: &AppHandle, action: Action) {
    let faces = faces_window(app);
    match action {
        Action::Talk | Action::Aside => {
            if let (Some(w), Some(r)) = (&faces, spot_rect(app, action == Action::Talk)) {
                place(w, r);
                let _ = w.show();
            }
        }
        Action::CycleSize => {
            if let Some(w) = &faces {
                let scale = w.scale_factor().unwrap_or(1.0);
                let sizes: Vec<Size> = app
                    .state::<Config>()
                    .sizes
                    .iter()
                    .map(|s| scaled(*s, scale))
                    .collect();
                if let Some(r) = current_rect(w) {
                    let rects: Vec<Rect> = screens(app).iter().map(|s| s.rect).collect();
                    place(w, clamp_to_screens(cycle_size(r, &sizes), &rects));
                }
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
        Action::ToggleFaces => match &faces {
            Some(w) if w.is_visible().unwrap_or(false) => {
                let _ = w.hide();
            }
            Some(w) => {
                let _ = w.show();
            }
            None if app.state::<MeetState>().call().in_call => {
                if let Some(m) = meet::meet_window(app) {
                    let _ = m.eval("window.__cooeeFaces && window.__cooeeFaces.open()");
                }
            }
            None => {}
        },
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

macro_rules! commands {
    ($($name:ident => $action:ident),* $(,)?) => {
        $(
            #[tauri::command]
            pub fn $name(app: AppHandle) {
                let a = app.clone();
                let _ = app.run_on_main_thread(move || run(&a, Action::$action));
            }
        )*
    };
}

commands! {
    snap_talk => Talk,
    snap_aside => Aside,
    cycle_faces_size => CycleSize,
    save_talk => SaveTalk,
    save_aside => SaveAside,
}
