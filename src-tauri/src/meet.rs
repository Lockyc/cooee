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

/// The scripts injected into every Meet page, in order: the adapter (the only code that knows
/// Meet's DOM), the popup lifecycle, then one script per popup, each consuming the adapter's
/// neutral model only.
const INJECTED_JS: [&str; 5] = [
    include_str!("../inject/meet-adapter.js"),
    include_str!("../inject/popup.js"),
    include_str!("../inject/faces.js"),
    include_str!("../inject/controls.js"),
    include_str!("../inject/share.js"),
];

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
    /// The page holds a live camera or mic track — read from getUserMedia, not Meet's DOM.
    pub capturing: bool,
    /// Another participant is presenting (your own share doesn't count).
    pub sharing: bool,
}

impl CallState {
    /// One adapter report against `latched` (this page has been in a call it hasn't left):
    /// returns the new latch and whether the user just left. The adapter's first "not in one"
    /// report is no leave. Like the join guard (`must_ask_before_replacing`), a live capture
    /// counts as still in the call, so a Meet re-render that drops the in-call marker mid-call
    /// can't send the window home; the latch holds until capture stops too, which after a real
    /// leave comes a beat after the marker goes.
    pub fn leave_step(latched: bool, next: CallState) -> (bool, bool) {
        if next.in_call {
            (true, false)
        } else if latched && !next.capturing {
            (false, true)
        } else {
            (latched, false)
        }
    }
}

/// Whether replacing the Meet window's page needs the user's OK. A live camera or mic capture
/// counts as a call even when the adapter reports none, so a Meet redesign that hides the
/// adapter's in-call marker can't make a join drop a live call silently.
pub fn must_ask_before_replacing(call: CallState) -> bool {
    call.in_call || call.capturing
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
    /// This page has been in a call it hasn't left (`CallState::leave_step`).
    in_call_latch: bool,
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
    /// Whether this page is in a call: the `leave_step` latch, which holds through a re-render
    /// that drops Meet's in-call marker. Leaving, the popups and their toggles all read this.
    pub fn in_call(&self) -> bool {
        self.inner.lock().unwrap().in_call_latch
    }
    pub fn set_call(&self, call: CallState) {
        let mut g = self.inner.lock().unwrap();
        g.call = call;
        g.in_call_latch = call.in_call;
    }
    /// Record an adapter report; returns whether the user just left the call.
    pub fn report(&self, next: CallState) -> bool {
        let mut g = self.inner.lock().unwrap();
        g.call = next;
        let (latch, left) = CallState::leave_step(g.in_call_latch, next);
        g.in_call_latch = latch;
        left
    }
    /// The page is navigating away, which ends any call on it; returns whether it was in one.
    pub fn page_started(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        g.call = CallState::default();
        std::mem::take(&mut g.in_call_latch)
    }
    fn set_pending(&self, profile: Profile, url: Url) {
        self.inner.lock().unwrap().pending = Some((profile, url));
    }
    /// The Meet window is gone: no call, and hand over any open that was waiting for it.
    fn on_destroyed(&self) -> Option<(Profile, Url)> {
        let mut g = self.inner.lock().unwrap();
        g.call = CallState::default();
        g.in_call_latch = false;
        g.pending.take()
    }
    fn profile(&self) -> Option<String> {
        self.inner.lock().unwrap().profile.clone()
    }
    fn set_profile(&self, name: &str) {
        let mut g = self.inner.lock().unwrap();
        g.profile = Some(name.to_string());
        g.call = CallState::default();
        g.in_call_latch = false;
    }
}

pub fn meet_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MEET_LABEL)
}

/// Open `url` in `profile`'s Meet window. During a call this asks before leaving it.
pub fn open_meet(app: &AppHandle, profile: &Profile, url: &Url) {
    let state = app.state::<MeetState>();
    if let Some(w) = meet_window(app) {
        shell_core::focus::bring_forward(&w);
        let call = state.call();
        if must_ask_before_replacing(call) {
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
    let mut builder = WebviewWindowBuilder::new(app, MEET_LABEL, WebviewUrl::External(url.clone()))
        .title(format!("Meet — {}", profile.name))
        .inner_size(1200.0, 820.0)
        .focused(shell_core::focus::focused())
        .user_agent(shell_core::web::desktop_ua())
        .data_store_identifier(shell_core::web::session::data_store_id(&profile.name));
    for js in INJECTED_JS {
        builder = builder.initialization_script(js);
    }
    let w = builder
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
                // popup.js's `window.open("about:blank#<kind>")`: one of cooee's popups.
                return match crate::popups::Popup::from_url(&url) {
                    Some(kind) => crate::popups::create(&popup_app, kind, features),
                    None => NewWindowResponse::Deny,
                };
            }
            shell_core::web::escape::escape_to_default_browser(url.as_str());
            NewWindowResponse::Deny
        })
        .on_page_load(|w, payload| {
            if payload.event() == PageLoadEvent::Started {
                let app = w.app_handle().clone();
                let was_in_call = app.state::<MeetState>().page_started();
                crate::popups::close_all(&app);
                // Leaving a call by navigating away (Meet's "you left" page): back to home.
                if was_in_call {
                    let a = app.clone();
                    let _ = app.run_on_main_thread(move || go_home(&a));
                }
            }
        })
        .build()?;
    allow_scripted_popups(&w)?;
    crate::screenshare::install(app, &w)?;
    set_regular_app(app, true);
    shell_core::focus::bring_forward(&w);
    let app = app.clone();
    let logged = w.clone();
    w.on_window_event(move |e| {
        crate::winlog::record(&logged, e);
        if let tauri::WindowEvent::Destroyed = e {
            let pending = app.state::<MeetState>().on_destroyed();
            crate::popups::close_all(&app);
            if let Some((profile, url)) = pending {
                let app2 = app.clone();
                let _ = app.run_on_main_thread(move || open_new(&app2, &profile, &url));
            } else {
                set_regular_app(&app, false);
            }
        }
    });
    Ok(())
}

/// While the Meet window is open cooee is a regular app (Dock icon, ⌘-Tab, its window comes
/// forward like any other); otherwise it lives in the menu bar alone.
fn set_regular_app(app: &AppHandle, regular: bool) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(if regular {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    });
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
            capturing: true,
            sharing: true,
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
            capturing: true,
            ..CallState::default()
        };
        let out = CallState::default();
        assert_eq!(CallState::leave_step(true, out), (false, true));
        assert_eq!(
            CallState::leave_step(false, out),
            (false, false),
            "green room's first report"
        );
        assert_eq!(CallState::leave_step(false, in_call), (true, false));
        assert_eq!(CallState::leave_step(true, in_call), (true, false));
    }

    #[test]
    fn a_live_capture_holds_the_call_when_the_in_call_marker_drops() {
        let state = MeetState::default();
        let in_call = CallState {
            in_call: true,
            capturing: true,
            ..CallState::default()
        };
        let marker_gone = CallState {
            capturing: true,
            ..CallState::default()
        };
        assert!(!state.report(in_call));
        assert!(!state.report(marker_gone), "re-render mid-call");
        assert!(!state.report(in_call), "marker back");
        assert!(!state.report(marker_gone), "real leave, tracks still live");
        assert!(state.report(CallState::default()), "tracks stopped");
        assert!(!state.report(CallState::default()), "left once");
    }

    #[test]
    fn navigating_away_mid_call_is_a_leave_even_with_the_marker_gone() {
        let state = MeetState::default();
        state.report(CallState {
            in_call: true,
            ..CallState::default()
        });
        state.report(CallState {
            capturing: true,
            ..CallState::default()
        });
        assert!(state.page_started());
        assert!(!state.page_started());
    }

    #[test]
    fn a_live_capture_counts_as_a_call_for_the_join_guard() {
        let out = CallState::default();
        assert!(!must_ask_before_replacing(out));
        assert!(
            must_ask_before_replacing(CallState {
                capturing: true,
                ..out
            }),
            "adapter lost the call"
        );
        assert!(must_ask_before_replacing(CallState {
            in_call: true,
            ..out
        }));
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
mod html_sink_tests {
    use super::INJECTED_JS;

    const CHOOSER_HTML: &str = include_str!("../../src/chooser.html");

    /// Lines of `src` (outside `//` comment lines) that use a sink WebKit rejects silently here:
    /// an HTML-string write, which throws under Meet's inherited `require-trusted-types-for
    /// 'script'` CSP, or a `window.open` with no URL, which WebKit reports as an empty string
    /// Tauri denies. Reading `outerHTML` (the adapter's dump) is fine.
    fn html_sinks(src: &str) -> Vec<(usize, &str)> {
        let assigns = |line: &str, prop: &str| {
            line.match_indices(prop).any(|(i, _)| {
                let rest = line[i + prop.len()..].trim_start();
                (rest.starts_with('=') && !rest.starts_with("==")) || rest.starts_with("+=")
            })
        };
        src.lines()
            .enumerate()
            .filter(|(_, line)| {
                let t = line.trim_start();
                !(t.starts_with("//") || t.starts_with('*') || t.starts_with("/*"))
            })
            .filter(|(_, line)| {
                assigns(line, "innerHTML")
                    || assigns(line, "outerHTML")
                    || line.contains("insertAdjacentHTML")
                    || line.contains("document.write")
                    || [
                        "window.open()",
                        "window.open(\"\"",
                        "window.open(''",
                        "window.open(``",
                    ]
                    .iter()
                    .any(|p| line.contains(p))
            })
            .map(|(n, line)| (n + 1, line))
            .collect()
    }

    #[test]
    fn no_inject_script_assigns_html_strings() {
        for (i, src) in INJECTED_JS.iter().enumerate() {
            assert_eq!(html_sinks(src), vec![], "INJECTED_JS[{i}]");
        }
        assert_eq!(html_sinks(CHOOSER_HTML), vec![], "src/chooser.html");
    }

    #[test]
    fn the_sink_scan_fires_on_each_sink() {
        for bad in [
            "d.body.innerHTML = `<div>`;",
            "el.innerHTML += x;",
            "el.outerHTML=x;",
            "el.insertAdjacentHTML('beforeend', x);",
            "document.write(x);",
            "const p = window.open(\"\", \"cooee-faces\");",
            "const p = window.open();",
        ] {
            assert_eq!(html_sinks(bad).len(), 1, "{bad}");
        }
        for ok in [
            "html: t.outerHTML.replace(/x/g, \"\"),",
            "if (el.innerHTML === \"\") return;",
            "// window.open(\"\") reports an empty URL",
            "popup = window.open(`about:blank#${kind}`, `cooee-${kind}`);",
        ] {
            assert_eq!(html_sinks(ok), vec![], "{ok}");
        }
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
