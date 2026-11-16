//! The account chooser: a small always-on-top window near the cursor listing every profile, for a
//! Meet link whose account matched no single profile. The page (`src/chooser.html`) reports the
//! pick through `choose_profile`; the link waits in `Pending` until then.

use std::sync::Mutex;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use url::Url;

use cooee_config::Config;

use crate::meet;

/// Every chooser window's label starts with this; capabilities/chooser.json grants the pick to it.
const LABEL: &str = "chooser";
const TITLE: &str = "Join with which account?";
const WIDTH: f64 = 320.0;
/// The heading band above the list, in points.
const HEADER_HEIGHT: f64 = 48.0;
/// One profile's button: its name, and its account beneath.
const ROW_HEIGHT: f64 = 52.0;
/// Rows shown before the list scrolls.
const MAX_VISIBLE_ROWS: usize = 8;

/// The link waiting for a pick.
#[derive(Default)]
pub struct Pending(Mutex<Option<Url>>);

impl Pending {
    pub fn set(&self, url: Url) {
        *self.0.lock().unwrap() = Some(url);
    }

    pub fn take(&self) -> Option<Url> {
        self.0.lock().unwrap().take()
    }
}

/// The window's inner height for `rows` profiles; past `MAX_VISIBLE_ROWS` the list scrolls.
pub(crate) fn chooser_height(rows: usize) -> f64 {
    HEADER_HEIGHT + rows.min(MAX_VISIBLE_ROWS) as f64 * ROW_HEIGHT
}

/// The window's origin: at the cursor, extending below and right, shifted up and left to stay
/// inside `area` (x, y, width, height), the cursor's monitor.
pub(crate) fn chooser_origin(
    cursor: (f64, f64),
    size: (f64, f64),
    area: (f64, f64, f64, f64),
) -> (f64, f64) {
    let (ax, ay, aw, ah) = area;
    let fit = |c: f64, len: f64, start: f64, span: f64| c.min(start + span - len).max(start);
    (fit(cursor.0, size.0, ax, aw), fit(cursor.1, size.1, ay, ah))
}

/// A fresh label per chooser: a destroyed window's label stays registered until its Destroyed
/// event, so reusing one label would fail a quick re-ask.
fn next_label() -> String {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    format!(
        "{LABEL}-{}",
        N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    )
}

fn close(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if label.starts_with(LABEL) {
            let _ = w.destroy();
        }
    }
}

/// Where a chooser of `size` opens: by the cursor, inside its monitor's work area, in points.
/// tao reports the cursor as its point position times the primary monitor's scale, and each
/// monitor's rect as points times that monitor's own scale.
fn origin_at_cursor(app: &AppHandle, size: (f64, f64)) -> Option<(f64, f64)> {
    let primary_scale = app.primary_monitor().ok()??.scale_factor();
    let c = app.cursor_position().ok()?.to_logical::<f64>(primary_scale);
    let m = app.monitor_from_point(c.x, c.y).ok()??;
    let (a, s) = (m.work_area(), m.scale_factor());
    let area = (
        a.position.x as f64 / s,
        a.position.y as f64 / s,
        a.size.width as f64 / s,
        a.size.height as f64 / s,
    );
    Some(chooser_origin((c.x, c.y), size, area))
}

/// Ask which profile joins `url`: open the chooser by the cursor, replacing any open one.
pub fn ask(app: &AppHandle, cfg: &Config, url: Url) {
    app.state::<Pending>().set(url);
    close(app);
    let choices: Vec<serde_json::Value> = cfg
        .profiles
        .iter()
        .map(|p| serde_json::json!({ "name": p.name, "account": p.account }))
        .collect();
    let script = format!(
        "window.__COOEE_CHOICES__ = {};\nwindow.__COOEE_CHOOSER_LAYOUT__ = {};",
        serde_json::Value::from(choices),
        serde_json::json!({ "header": HEADER_HEIGHT, "row": ROW_HEIGHT }),
    );
    let height = chooser_height(cfg.profiles.len());
    let mut builder =
        WebviewWindowBuilder::new(app, next_label(), WebviewUrl::App("chooser.html".into()))
            .title(TITLE)
            .inner_size(WIDTH, height)
            .resizable(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .focused(true)
            .initialization_script(&script);
    match origin_at_cursor(app, (WIDTH, height)) {
        Some((x, y)) => builder = builder.position(x, y),
        None => builder = builder.center(),
    }
    match builder.build() {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("cooee: can't open the account chooser: {e}"),
    }
}

/// The chooser page's pick: join the pending link as `name`. `None`, or a name no profile has,
/// cancels.
#[tauri::command]
pub fn choose_profile(app: AppHandle, name: Option<String>) {
    let url = app.state::<Pending>().take();
    close(&app);
    let (Some(url), Some(name)) = (url, name) else {
        return;
    };
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        let cfg = a.state::<Config>();
        if let Some(p) = cfg.profiles.iter().find(|p| p.name == name) {
            meet::open_meet(&a, p, &url);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chooser_height_grows_per_row() {
        assert!(chooser_height(3) > chooser_height(2));
    }

    #[test]
    fn chooser_height_caps_and_scrolls() {
        assert_eq!(chooser_height(12), chooser_height(MAX_VISIBLE_ROWS));
        assert!(chooser_height(MAX_VISIBLE_ROWS) > chooser_height(MAX_VISIBLE_ROWS - 1));
    }

    #[test]
    fn chooser_opens_below_right_of_the_cursor() {
        assert_eq!(
            chooser_origin((100.0, 100.0), (320.0, 200.0), (0.0, 0.0, 1920.0, 1080.0)),
            (100.0, 100.0)
        );
    }

    #[test]
    fn chooser_stays_on_the_cursors_monitor() {
        // Near the bottom-right corner it shifts up and left to fit.
        assert_eq!(
            chooser_origin((1900.0, 1070.0), (320.0, 200.0), (0.0, 0.0, 1920.0, 1080.0)),
            (1600.0, 880.0)
        );
        // A second monitor to the left (negative x).
        assert_eq!(
            chooser_origin((-10.0, 5.0), (320.0, 200.0), (-1920.0, 0.0, 1920.0, 1080.0)),
            (-320.0, 5.0)
        );
    }

    #[test]
    fn pending_link_is_taken_once() {
        let p = Pending::default();
        p.set(url::Url::parse("https://meet.google.com/abc-defg-hij").unwrap());
        assert!(p.take().is_some());
        assert!(p.take().is_none());
    }
}
