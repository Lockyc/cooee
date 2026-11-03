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
    Browser,
}

/// Google's own hosts, which Meet's frames and sign-in move between.
fn is_google_family(host: &str) -> bool {
    [
        "google.com",
        "gstatic.com",
        "youtube.com",
        "googleusercontent.com",
        "googleapis.com",
    ]
    .iter()
    .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Decide a navigation in the Meet window (every frame's, not only the main frame's).
pub fn route_navigation(url: &Url) -> NavRoute {
    match url.host_str() {
        None => NavRoute::Stay, // about:blank, data:, the faces popup
        Some("workspace.google.com") => NavRoute::Redirect(Url::parse(HOME_URL).unwrap()),
        Some(h) if is_google_family(h) => NavRoute::Stay,
        Some(_) => NavRoute::Browser,
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, serde::Serialize)]
pub struct CallState {
    pub in_call: bool,
    pub others: usize,
    pub adapter_ok: bool,
}

#[derive(Default)]
pub struct MeetState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    profile: Option<String>,
    call: CallState,
}

impl MeetState {
    pub fn call(&self) -> CallState {
        self.inner.lock().unwrap().call
    }
    pub fn set_call(&self, call: CallState) {
        self.inner.lock().unwrap().call = call;
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
        if state.call().in_call {
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
        let _ = w.destroy();
    }
    state.set_profile(&profile.name);
    if let Err(e) = build(app, profile, url) {
        eprintln!("cooee: can't open the Meet window: {e}");
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
            NavRoute::Browser => {
                shell_core::web::escape::escape_to_default_browser(url.as_str());
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
                let app = w.app_handle();
                app.state::<MeetState>().set_call(CallState::default());
                crate::faces::close(app);
            }
        })
        .build()?;
    allow_scripted_popups(&w)?;
    let app = app.clone();
    w.on_window_event(move |e| {
        if let tauri::WindowEvent::Destroyed = e {
            app.state::<MeetState>().set_call(CallState::default());
            crate::faces::close(&app);
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
    fn other_navigations_escape() {
        assert_eq!(
            route_navigation(&u("https://example.com/doc")),
            NavRoute::Browser
        );
        assert_eq!(
            route_navigation(&u("https://google.com.evil.example/")),
            NavRoute::Browser
        );
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
