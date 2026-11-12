//! Diagnostics: one line per move, resize, scale change and focus change of the Meet window and
//! its popups, appended to `window-events.log` in the app data dir, so a popup that changes size
//! when dragged, or a window ⌘-Tab doesn't bring forward, can be traced after the call.

use std::io::Write;

use tauri::{Manager, WebviewWindow, WindowEvent};

/// Past this size the log starts over, so it never grows without bound.
const MAX_BYTES: u64 = 1 << 20;

fn what(e: &WindowEvent) -> Option<String> {
    Some(match e {
        WindowEvent::Moved(p) => format!("moved to {},{} (physical)", p.x, p.y),
        WindowEvent::Resized(s) => format!("resized to {}x{} (physical)", s.width, s.height),
        WindowEvent::ScaleFactorChanged {
            scale_factor,
            new_inner_size,
            ..
        } => format!(
            "scale → {scale_factor}, inner {}x{}",
            new_inner_size.width, new_inner_size.height
        ),
        WindowEvent::Focused(f) => format!("focused {f}"),
        _ => return None,
    })
}

pub fn record(w: &WebviewWindow, e: &WindowEvent) {
    let Some(event) = what(e) else {
        return;
    };
    let scale = w.scale_factor().unwrap_or(1.0);
    let rect = match (w.outer_position(), w.inner_size()) {
        (Ok(p), Ok(s)) => format!(
            "{:.0},{:.0} {:.0}x{:.0} pt",
            p.x as f64 / scale,
            p.y as f64 / scale,
            s.width as f64 / scale,
            s.height as f64 / scale
        ),
        _ => "?".into(),
    };
    let monitor = w
        .current_monitor()
        .ok()
        .flatten()
        .and_then(|m| m.name().cloned())
        .unwrap_or_default();
    note(
        w.app_handle(),
        &format!(
            "{} {event} | now {rect} scale {scale} on {monitor}",
            w.label()
        ),
    );
}

/// Append one line to the log.
pub fn note(app: &tauri::AppHandle, line: &str) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let path = dir.join("window-events.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::remove_file(&path);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{now} {line}");
    }
}
