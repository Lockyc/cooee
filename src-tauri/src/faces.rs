//! The faces window: the Meet page's `window.open` popup (inject/faces.js renders into it), made
//! an always-on-top Tauri window. It shares the Meet webview's WebKit configuration, which is what
//! lets the popup play the call's MediaStreams.

use tauri::webview::{NewWindowFeatures, NewWindowResponse};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

const LABEL_PREFIX: &str = "faces-";
const DEFAULT_SIZE: (f64, f64) = (560.0, 350.0);

/// A fresh label per popup: a destroyed window's label stays registered until its Destroyed
/// event, so reusing one label would fail a quick re-open.
fn next_label() -> String {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    format!(
        "{LABEL_PREFIX}{}",
        N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    )
}

fn is_faces_label(label: &str) -> bool {
    label.starts_with(LABEL_PREFIX)
}

fn label_number(label: &str) -> Option<usize> {
    label.strip_prefix(LABEL_PREFIX)?.parse().ok()
}

/// The current faces window: the newest, since a destroyed one stays registered until its
/// Destroyed event.
pub fn faces_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.webview_windows()
        .into_iter()
        .filter_map(|(label, w)| label_number(&label).map(|n| (n, w)))
        .max_by_key(|(n, _)| *n)
        .map(|(_, w)| w)
}

/// Materialize the Meet page's popup as the faces window. A faces window still registered is a
/// stale one (its page lost the popup), so it goes.
pub fn create(app: &AppHandle, features: NewWindowFeatures) -> NewWindowResponse<tauri::Wry> {
    close(app);
    let built = WebviewWindowBuilder::new(
        app,
        next_label(),
        WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .window_features(features)
    .title("cooee")
    .inner_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1)
    // Joining shouldn't take keyboard focus from Meet (its ⌘D/⌘E).
    .focused(false)
    .always_on_top(true)
    .visible_on_all_workspaces(true)
    .skip_taskbar(true)
    .build();
    match built {
        Ok(window) => {
            // Closing the faces window mid-call hides it; the toggle hotkey brings it back.
            let w = window.clone();
            window.on_window_event(move |e| {
                if let WindowEvent::CloseRequested { api, .. } = e {
                    api.prevent_close();
                    let _ = w.hide();
                }
            });
            crate::spots::place_at_talk(app, &window);
            NewWindowResponse::Create { window }
        }
        Err(e) => {
            eprintln!("cooee: can't create the faces window: {e}");
            NewWindowResponse::Deny
        }
    }
}

/// The call ended (or the Meet window went away): the faces window goes with it.
pub fn close(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if is_faces_label(&label) {
            let _ = w.destroy();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_popup_gets_a_fresh_label() {
        // A destroyed window's label stays registered until its Destroyed event, so reusing one
        // label made a quick re-open fail.
        let a = next_label();
        let b = next_label();
        assert_ne!(a, b);
        assert!(is_faces_label(&a) && is_faces_label(&b));
        assert!(!is_faces_label("meet"));
        assert!(label_number(&b) > label_number(&a), "newest wins");
    }
}
