//! The faces window: the Meet page's `window.open` popup (inject/faces.js renders into it), made
//! an always-on-top Tauri window. It shares the Meet webview's WebKit configuration, which is what
//! lets the popup play the call's MediaStreams.

use tauri::webview::{NewWindowFeatures, NewWindowResponse};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

pub const FACES_LABEL: &str = "faces";
const DEFAULT_SIZE: (f64, f64) = (560.0, 350.0);

pub fn faces_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(FACES_LABEL)
}

/// Materialize the Meet page's popup as the faces window.
pub fn create(app: &AppHandle, features: NewWindowFeatures) -> NewWindowResponse<tauri::Wry> {
    if let Some(w) = faces_window(app) {
        let _ = w.destroy();
    }
    let built = WebviewWindowBuilder::new(
        app,
        FACES_LABEL,
        WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .window_features(features)
    .title("cooee")
    .inner_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1)
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
            crate::spots::place_at_talk(app);
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
    if let Some(w) = faces_window(app) {
        let _ = w.destroy();
    }
}
