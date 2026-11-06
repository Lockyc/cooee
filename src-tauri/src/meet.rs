//! The Meet window: one at a time, in one profile's WebKit data store.

use std::sync::Mutex;
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use url::Url;

use cooee_config::Profile;

pub const MEET_LABEL: &str = "meet";

/// Where a profile's Meet window starts and returns to after a call: Google's sign-in, which
/// continues straight to the Meet home when the profile is already signed in. (Meet's own `/`
/// sends a signed-out profile to the Workspace marketing page, which [`route_navigation`]
/// redirects here — so landing on `/` directly could loop.)
pub const HOME_URL: &str =
    "https://accounts.google.com/ServiceLogin?continue=https%3A%2F%2Fmeet.google.com%2F";

/// The injected adapter: the only code that knows Meet's DOM.
const ADAPTER_JS: &str = include_str!("../inject/meet-adapter.js");
/// The faces UI: consumes the adapter's neutral model only.
const FACES_JS: &str = include_str!("../inject/faces.js");

#[derive(Debug, PartialEq)]
pub enum NavRoute {
    Stay,
    Redirect(Url),
}

/// Decide a navigation in the Meet window. `on_navigation` sees every frame's navigations and
/// can't tell a subframe from the main frame, so everything loads in place (add-on iframes,
/// federated sign-in) except the Workspace marketing page, where Meet sends a signed-out profile.
/// Links Meet opens in a new window go to the default browser from `on_new_window` instead.
pub fn route_navigation(url: &Url) -> NavRoute {
    match url.host_str() {
        Some("workspace.google.com") => NavRoute::Redirect(Url::parse(HOME_URL).unwrap()),
        _ => NavRoute::Stay,
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, serde::Serialize)]
pub struct CallState {
    pub in_call: bool,
    pub others: usize,
    pub adapter_ok: bool,
    /// The page holds a live camera or mic track — read from getUserMedia, not Meet's DOM.
    pub capturing: bool,
}

impl CallState {
    /// The user just left a call (not merely: the adapter's first report says "not in one").
    pub fn left(prev: CallState, next: CallState) -> bool {
        prev.in_call && !next.in_call
    }
}

/// Whether replacing the Meet window's page needs the user's OK. A live camera or mic capture
/// counts as a call even when the adapter reports none, so a Meet redesign that hides the
/// adapter's in-call marker can't make a join drop a live call silently.
pub fn must_ask_before_replacing(call: CallState, capture_active: bool) -> bool {
    call.in_call || capture_active
}

#[derive(Default)]
pub struct MeetState {
    inner: Mutex<Inner>,
    /// Set by the tray's snapshot item; the adapter's dump is honoured only once per request,
    /// so the page itself can't write files or pop Finder.
    snapshot_requested: std::sync::atomic::AtomicBool,
}

#[derive(Default)]
struct Inner {
    profile: Option<String>,
    call: CallState,
    /// An open waiting for the current window to finish closing: a window's label stays taken
    /// until its `Destroyed` event, so the replacement is built from there.
    pending: Option<(Profile, Url)>,
}

impl MeetState {
    pub fn request_snapshot(&self) {
        self.snapshot_requested
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn take_snapshot_request(&self) -> bool {
        self.snapshot_requested
            .swap(false, std::sync::atomic::Ordering::SeqCst)
    }
    pub fn call(&self) -> CallState {
        self.inner.lock().unwrap().call
    }
    pub fn set_call(&self, call: CallState) {
        self.inner.lock().unwrap().call = call;
    }
    /// Record the call state and report the previous one.
    pub fn swap_call(&self, call: CallState) -> CallState {
        std::mem::replace(&mut self.inner.lock().unwrap().call, call)
    }
    fn set_pending(&self, profile: Profile, url: Url) {
        self.inner.lock().unwrap().pending = Some((profile, url));
    }
    /// The Meet window is gone: no call, and hand over any open that was waiting for it.
    fn on_destroyed(&self) -> Option<(Profile, Url)> {
        let mut g = self.inner.lock().unwrap();
        g.call = CallState::default();
        g.pending.take()
    }
    fn profile(&self) -> Option<String> {
        self.inner.lock().unwrap().profile.clone()
    }
    fn set_profile(&self, name: &str) {
        let mut g = self.inner.lock().unwrap();
        g.profile = Some(name.to_string());
        g.call = CallState::default();
    }
}

pub fn meet_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MEET_LABEL)
}

/// Open `url` in `profile`'s Meet window. During a call this asks before leaving it.
pub fn open_meet(app: &AppHandle, profile: &Profile, url: &Url) {
    let state = app.state::<MeetState>();
    if let Some(w) = meet_window(app) {
        let _ = w.show();
        let _ = w.set_focus();
        let call = state.call();
        if must_ask_before_replacing(call, call.capturing) {
            let (app, profile, url) = (app.clone(), profile.clone(), url.clone());
            app.dialog()
                .message("Leave this call and join the new one?")
                .title("cooee")
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "Leave and join".into(),
                    "Stay".into(),
                ))
                .show(move |leave| {
                    if leave {
                        let app2 = app.clone();
                        let _ = app.run_on_main_thread(move || replace(&app2, &profile, &url));
                    }
                });
            return;
        }
    }
    replace(app, profile, url);
}

/// Navigate the existing window when the profile matches; otherwise rebuild it in the new
/// profile's data store (a webview's store is fixed at creation).
fn replace(app: &AppHandle, profile: &Profile, url: &Url) {
    let state = app.state::<MeetState>();
    if let Some(w) = meet_window(app) {
        if state.profile().as_deref() == Some(profile.name.as_str()) {
            let _ = w.navigate(url.clone());
            state.set_call(CallState::default());
            return;
        }
        // The label stays taken until the old window's Destroyed event, which builds this one.
        state.set_pending(profile.clone(), url.clone());
        let _ = w.destroy();
        return;
    }
    open_new(app, profile, url);
}

fn open_new(app: &AppHandle, profile: &Profile, url: &Url) {
    app.state::<MeetState>().set_profile(&profile.name);
    if let Err(e) = build(app, profile, url) {
        eprintln!("cooee: can't open the Meet window: {e}");
    }
}

/// Return the Meet window to its profile's home after the user leaves a call.
pub fn go_home(app: &AppHandle) {
    if let Some(w) = meet_window(app) {
        let _ = w.navigate(Url::parse(HOME_URL).unwrap());
    }
}

fn build(app: &AppHandle, profile: &Profile, url: &Url) -> tauri::Result<()> {
    let nav_app = app.clone();
    let popup_app = app.clone();
    let w = WebviewWindowBuilder::new(app, MEET_LABEL, WebviewUrl::External(url.clone()))
        .title(format!("Meet — {}", profile.name))
        .inner_size(1200.0, 820.0)
        .user_agent(shell_core::web::desktop_ua())
        .data_store_identifier(shell_core::web::session::data_store_id(&profile.name))
        .initialization_script(ADAPTER_JS)
        .initialization_script(FACES_JS)
        .on_navigation(move |url| match route_navigation(url) {
            NavRoute::Stay => true,
            NavRoute::Redirect(to) => {
                let app = nav_app.clone();
                let _ = nav_app.run_on_main_thread(move || {
                    if let Some(w) = meet_window(&app) {
                        let _ = w.navigate(to);
                    }
                });
                false
            }
        })
        .on_new_window(move |url, features| {
            if url.host_str().is_none() {
                // faces.js's `window.open("")`: the faces window.
                return crate::faces::create(&popup_app, features);
            }
            shell_core::web::escape::escape_to_default_browser(url.as_str());
            NewWindowResponse::Deny
        })
        .on_page_load(|w, payload| {
            if payload.event() == PageLoadEvent::Started {
                let app = w.app_handle().clone();
                let prev = app.state::<MeetState>().swap_call(CallState::default());
                crate::faces::close(&app);
                // Leaving a call by navigating away (Meet's "you left" page): back to home.
                if CallState::left(prev, CallState::default()) {
                    let a = app.clone();
                    let _ = app.run_on_main_thread(move || go_home(&a));
                }
            }
        })
        .build()?;
    allow_scripted_popups(&w)?;
    let app = app.clone();
    w.on_window_event(move |e| {
        if let tauri::WindowEvent::Destroyed = e {
            let pending = app.state::<MeetState>().on_destroyed();
            crate::faces::close(&app);
            if let Some((profile, url)) = pending {
                let app2 = app.clone();
                let _ = app.run_on_main_thread(move || open_new(&app2, &profile, &url));
            }
        }
    });
    Ok(())
}

/// The adapter opens the faces popup from a DOM observer, not a click, so WebKit must allow
/// script-initiated `window.open`.
fn allow_scripted_popups(w: &WebviewWindow) -> tauri::Result<()> {
    w.with_webview(|pw| unsafe {
        use objc2::{msg_send, runtime::AnyObject};
        let wv = pw.inner() as *mut AnyObject;
        let config: *mut AnyObject = msg_send![&*wv, configuration];
        let prefs: *mut AnyObject = msg_send![&*config, preferences];
        let _: () = msg_send![&*prefs, setJavaScriptCanOpenWindowsAutomatically: true];
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn navigation_to_marketing_page_redirects_home() {
        assert_eq!(
            route_navigation(&u("https://workspace.google.com/products/meet/")),
            NavRoute::Redirect(u(HOME_URL))
        );
    }

    #[test]
    fn meet_and_accounts_navigations_stay() {
        for s in [
            "https://meet.google.com/abc-defg-hij",
            "https://accounts.google.com/v3/signin/identifier",
            "https://accounts.youtube.com/accounts/SetSID",
            "https://ogs.google.com/widget/app",
            "https://www.gstatic.com/x",
        ] {
            assert_eq!(route_navigation(&u(s)), NavRoute::Stay, "{s}");
        }
    }

    #[test]
    fn about_blank_stays() {
        assert_eq!(route_navigation(&u("about:blank")), NavRoute::Stay);
    }

    #[test]
    fn other_hosts_load_in_place() {
        // on_navigation can't tell a subframe from the main frame: a Meet add-on iframe or a
        // federated sign-in (Okta, Entra) must load, not be bounced to the browser.
        assert_eq!(
            route_navigation(&u("https://example.com/doc")),
            NavRoute::Stay
        );
        assert_eq!(
            route_navigation(&u("https://login.microsoftonline.com/saml")),
            NavRoute::Stay
        );
    }

    fn profile(name: &str) -> Profile {
        Profile {
            name: name.into(),
            account: None,
        }
    }

    #[test]
    fn destroyed_window_hands_over_the_pending_open() {
        let state = MeetState::default();
        state.set_call(CallState {
            in_call: true,
            others: 2,
            adapter_ok: true,
            capturing: true,
        });
        state.set_pending(profile("home"), u(HOME_URL));
        let pending = state.on_destroyed().expect("pending open");
        assert_eq!(pending.0.name, "home");
        assert_eq!(pending.1, u(HOME_URL));
        assert_eq!(state.call(), CallState::default());
        assert!(state.on_destroyed().is_none(), "handed over once");
    }

    #[test]
    fn leaving_is_the_in_call_to_not_in_call_transition() {
        let in_call = CallState {
            in_call: true,
            ..CallState::default()
        };
        let out = CallState::default();
        assert!(CallState::left(in_call, out));
        assert!(!CallState::left(out, out), "green room's first report");
        assert!(!CallState::left(out, in_call));
        assert!(!CallState::left(in_call, in_call));
    }

    #[test]
    fn a_live_capture_counts_as_a_call_for_the_join_guard() {
        let out = CallState::default();
        assert!(!must_ask_before_replacing(out, false));
        assert!(
            must_ask_before_replacing(out, true),
            "adapter lost the call"
        );
        assert!(must_ask_before_replacing(
            CallState {
                in_call: true,
                ..out
            },
            false
        ));
    }

    #[test]
    fn home_url_signs_in_then_lands_on_meet() {
        let home = u(HOME_URL);
        assert_eq!(home.host_str(), Some("accounts.google.com"));
        assert!(home
            .query_pairs()
            .any(|(k, v)| k == "continue" && v == "https://meet.google.com/"));
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    #[test]
    fn a_snapshot_is_saved_only_when_the_tray_asked_once() {
        let state = MeetState::default();
        assert!(!state.take_snapshot_request(), "the page can't trigger it");
        state.request_snapshot();
        assert!(state.take_snapshot_request());
        assert!(!state.take_snapshot_request(), "one save per request");
    }
}
