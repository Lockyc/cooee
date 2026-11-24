//! The Meet page's popups — the faces, controls and share windows. Each is a `window.open` of
//! `about:blank#<kind>` from an injected script (inject/popup.js), made an always-on-top Tauri
//! window here. They share the Meet webview's WebKit configuration, which is what lets them play
//! the call's MediaStreams. Rust decides when they close, from the adapter's `meet_state`.

use tauri::webview::{NewWindowFeatures, NewWindowResponse};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::meet::{CallState, MeetState};

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

/// A fresh `<stem>-<n>` label per popup or chooser: a destroyed window's label stays registered
/// until its Destroyed event, so reusing one label would fail a quick re-open.
pub(crate) fn fresh_label(stem: &str) -> String {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    format!(
        "{stem}-{}",
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

/// Whether `kind` may not exist while `in_call` ([`MeetState::in_call`]) and the adapter's last
/// report `call` hold: every popup ends with the call, and the share window also ends with the
/// share — destroyed even when hidden, so the next share opens fresh. A report without Meet's
/// in-call marker reads no presentation, so only one with the marker ends a share.
pub fn must_close(in_call: bool, call: CallState, kind: Popup) -> bool {
    !in_call || (kind == Popup::Share && call.in_call && !call.sharing)
}

/// [`must_close`] against the Meet window's current state.
fn disallowed(app: &AppHandle, kind: Popup) -> bool {
    let state = app.state::<MeetState>();
    must_close(state.in_call(), state.call(), kind)
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
/// page lost the popup), so it goes. Any frame in the Meet window can ask, add-on iframes
/// included, so a kind the call state doesn't allow is refused.
pub fn create(
    app: &AppHandle,
    kind: Popup,
    features: NewWindowFeatures,
) -> NewWindowResponse<tauri::Wry> {
    if disallowed(app, kind) {
        return NewWindowResponse::Deny;
    }
    close(app, kind);
    let built = WebviewWindowBuilder::new(
        app,
        fresh_label(kind.name()),
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
                    WindowEvent::Moved(_) | WindowEvent::Resized(_) => match kind {
                        Popup::Faces => crate::spots::dock_to_faces(&a),
                        Popup::Controls => crate::spots::remember_controls_size(&a, &w),
                        Popup::Share => crate::spots::remember_share(&a, &w),
                    },
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

/// Destroy the popups the Meet window's state no longer allows.
pub fn close_disallowed(app: &AppHandle) {
    for kind in Popup::ALL {
        if disallowed(app, kind) {
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
            let a = fresh_label(kind.name());
            let b = fresh_label(kind.name());
            assert_ne!(a, b);
            assert_eq!(label_kind(&a), Some(kind));
            assert!(
                label_number(kind, &b) > label_number(kind, &a),
                "newest wins"
            );
        }
        assert_eq!(label_kind("meet"), None);
        assert_eq!(
            label_kind(&fresh_label(Popup::Controls.name())),
            Some(Popup::Controls)
        );
        assert_eq!(
            label_number(Popup::Faces, &fresh_label(Popup::Controls.name())),
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
                must_close(false, CallState::default(), kind),
                "{kind:?} ends with the call"
            );
            assert!(
                !must_close(true, call, kind),
                "{kind:?} stays during a share"
            );
        }
        let no_share = CallState {
            sharing: false,
            ..call
        };
        assert!(must_close(true, no_share, Popup::Share), "the share ended");
        assert!(!must_close(true, no_share, Popup::Faces));
        assert!(!must_close(true, no_share, Popup::Controls));
    }

    #[test]
    fn every_popup_survives_a_re_render_that_drops_the_in_call_marker() {
        // Mid-share, Meet drops its Leave button while the camera is live: the latch holds, and
        // the marker-less report reads no presentation.
        let state = MeetState::default();
        state.report(CallState {
            in_call: true,
            capturing: true,
            sharing: true,
        });
        state.report(CallState {
            in_call: false,
            capturing: true,
            sharing: false,
        });
        for kind in Popup::ALL {
            assert!(!must_close(state.in_call(), state.call(), kind), "{kind:?}");
        }
        // Capture stops too: a real leave.
        state.report(CallState::default());
        for kind in Popup::ALL {
            assert!(must_close(state.in_call(), state.call(), kind), "{kind:?}");
        }
    }
}
