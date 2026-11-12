//! The Meet page's popups — the faces, controls and share windows. Each is a `window.open` of
//! `about:blank#<kind>` from an injected script (inject/popup.js), made an always-on-top Tauri
//! window here. They share the Meet webview's WebKit configuration, which is what lets them play
//! the call's MediaStreams. Rust decides when they close, from the adapter's `meet_state`.

use tauri::webview::{NewWindowFeatures, NewWindowResponse};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::meet::CallState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popup {
    /// The other people's faces.
    Faces,
    /// Your camera preview and Mute / Camera / Leave.
    Controls,
    /// Another participant's screen share.
    Share,
}

impl Popup {
    pub const ALL: [Popup; 3] = [Popup::Faces, Popup::Controls, Popup::Share];

    /// The kind an injected script names in its popup URL's fragment (`about:blank#controls`).
    pub fn from_url(url: &url::Url) -> Option<Popup> {
        Popup::ALL
            .into_iter()
            .find(|k| url.fragment() == Some(k.name()))
    }

    /// The fragment, label stem and page-side name.
    pub fn name(self) -> &'static str {
        match self {
            Popup::Faces => "faces",
            Popup::Controls => "controls",
            Popup::Share => "share",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Popup::Faces => "cooee",
            Popup::Controls => "cooee — controls",
            Popup::Share => "cooee — share",
        }
    }
}

/// A fresh label per popup: a destroyed window's label stays registered until its Destroyed
/// event, so reusing one label would fail a quick re-open.
fn next_label(kind: Popup) -> String {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    format!(
        "{}-{}",
        kind.name(),
        N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    )
}

fn label_number(kind: Popup, label: &str) -> Option<usize> {
    label
        .strip_prefix(kind.name())?
        .strip_prefix('-')?
        .parse()
        .ok()
}

fn label_kind(label: &str) -> Option<Popup> {
    Popup::ALL
        .into_iter()
        .find(|&k| label_number(k, label).is_some())
}

/// Whether `kind` may not outlive this call state: every popup ends with the call, and the share
/// window also ends with the share — destroyed even when hidden, so the next share opens fresh.
pub fn must_close(call: CallState, kind: Popup) -> bool {
    !call.in_call || (kind == Popup::Share && !call.sharing)
}

/// The current window of `kind`: the newest, since a destroyed one stays registered until its
/// Destroyed event.
pub fn window(app: &AppHandle, kind: Popup) -> Option<WebviewWindow> {
    app.webview_windows()
        .into_iter()
        .filter_map(|(label, w)| label_number(kind, &label).map(|n| (n, w)))
        .max_by_key(|(n, _)| *n)
        .map(|(_, w)| w)
}

/// Materialize the Meet page's popup as a `kind` window. One still registered is a stale one (its
/// page lost the popup), so it goes.
pub fn create(
    app: &AppHandle,
    kind: Popup,
    features: NewWindowFeatures,
) -> NewWindowResponse<tauri::Wry> {
    close(app, kind);
    let built = WebviewWindowBuilder::new(
        app,
        next_label(kind),
        WebviewUrl::External(format!("about:blank#{}", kind.name()).parse().unwrap()),
    )
    .window_features(features)
    .title(kind.title())
    // Opening shouldn't take keyboard focus from Meet (its ⌘D/⌘E).
    .focused(false)
    .always_on_top(true)
    .visible_on_all_workspaces(true)
    .skip_taskbar(true)
    .build();
    match built {
        Ok(window) => {
            // Closing a popup mid-call hides it; its toggle brings it back. Rust destroys it when
            // the call (or the share) ends.
            let w = window.clone();
            let a = app.clone();
            window.on_window_event(move |e| {
                crate::winlog::record(&w, e);
                match e {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                    WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                        crate::spots::remember_popup(&a, &w, kind)
                    }
                    _ => {}
                }
            });
            crate::spots::place_popup(app, &window, kind);
            NewWindowResponse::Create { window }
        }
        Err(e) => {
            eprintln!("cooee: can't create the {} window: {e}", kind.name());
            NewWindowResponse::Deny
        }
    }
}

/// Destroy every window of `kind`.
pub fn close(app: &AppHandle, kind: Popup) {
    for (label, w) in app.webview_windows() {
        if label_kind(&label) == Some(kind) {
            let _ = w.destroy();
        }
    }
}

/// Destroy the popups `call` no longer allows.
pub fn close_disallowed(app: &AppHandle, call: CallState) {
    for kind in Popup::ALL {
        if must_close(call, kind) {
            close(app, kind);
        }
    }
}

/// The call ended (or the Meet window went away): every popup goes with it.
pub fn close_all(app: &AppHandle) {
    for kind in Popup::ALL {
        close(app, kind);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> url::Url {
        url::Url::parse(s).unwrap()
    }

    #[test]
    fn fragment_picks_the_kind() {
        assert_eq!(Popup::from_url(&u("about:blank#faces")), Some(Popup::Faces));
        assert_eq!(
            Popup::from_url(&u("about:blank#controls")),
            Some(Popup::Controls)
        );
        assert_eq!(Popup::from_url(&u("about:blank#share")), Some(Popup::Share));
        assert_eq!(Popup::from_url(&u("about:blank")), None);
        assert_eq!(Popup::from_url(&u("about:blank#other")), None);
    }

    #[test]
    fn each_popup_gets_a_fresh_label_per_kind() {
        // A destroyed window's label stays registered until its Destroyed event, so reusing one
        // label made a quick re-open (a re-share, a rejoin) fail.
        for kind in Popup::ALL {
            let a = next_label(kind);
            let b = next_label(kind);
            assert_ne!(a, b);
            assert_eq!(label_kind(&a), Some(kind));
            assert!(
                label_number(kind, &b) > label_number(kind, &a),
                "newest wins"
            );
        }
        assert_eq!(label_kind("meet"), None);
        assert_eq!(
            label_kind(&next_label(Popup::Controls)),
            Some(Popup::Controls)
        );
        assert_eq!(
            label_number(Popup::Faces, &next_label(Popup::Controls)),
            None
        );
    }

    #[test]
    fn closing_rules() {
        let call = CallState {
            in_call: true,
            sharing: true,
            ..CallState::default()
        };
        for kind in Popup::ALL {
            assert!(
                must_close(CallState::default(), kind),
                "{kind:?} ends with the call"
            );
            assert!(!must_close(call, kind), "{kind:?} stays during a share");
        }
        let no_share = CallState {
            sharing: false,
            ..call
        };
        assert!(must_close(no_share, Popup::Share), "the share ended");
        assert!(!must_close(no_share, Popup::Faces));
        assert!(!must_close(no_share, Popup::Controls));
    }
}
