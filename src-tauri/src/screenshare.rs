//! Meet's Share button opens the picker the menu bar's **Share Opens** choice names: windows
//! (darkfield's canvas, one app) or entire screens. WebKit routes `getDisplayMedia` to the
//! private `WKUIDelegatePrivate` `_webView:requestDisplayCapturePermissionForOrigin:…` once a UI
//! delegate implements the public camera/mic permission method (wry's does); its answer picks
//! the picker. Without the method WebKit only ever offers a whole screen.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use block2::{Block, RcBlock};
use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
use objc2::{msg_send, sel};
use tauri::{AppHandle, Manager, WebviewWindow};

/// `WKDisplayCapturePermissionDecision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(isize)]
enum Decision {
    WindowPrompt = 2,
    ScreenPrompt = 1,
}

/// What Meet's Share button opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Window,
    Screen,
}

impl Target {
    fn decision(self) -> Decision {
        match self {
            Target::Window => Decision::WindowPrompt,
            Target::Screen => Decision::ScreenPrompt,
        }
    }

    /// The saved choice; anything unreadable is the default, the window picker.
    fn parse(s: &str) -> Target {
        if s.trim() == "screen" {
            Target::Screen
        } else {
            Target::Window
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Target::Window => "window",
            Target::Screen => "screen",
        }
    }
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static SCREEN: AtomicBool = AtomicBool::new(false);

fn target_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("share-opens"))
}

/// Read the saved choice at launch.
pub fn load(app: &AppHandle) {
    let t = target_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| Target::parse(&s))
        .unwrap_or(Target::Window);
    SCREEN.store(t == Target::Screen, Ordering::Relaxed);
}

pub fn target() -> Target {
    if SCREEN.load(Ordering::Relaxed) {
        Target::Screen
    } else {
        Target::Window
    }
}

/// Set and save what the Share button opens.
pub fn set_target(app: &AppHandle, t: Target) {
    SCREEN.store(t == Target::Screen, Ordering::Relaxed);
    if let Some(p) = target_path(app) {
        let _ = p.parent().map(std::fs::create_dir_all);
        if let Err(e) = std::fs::write(&p, t.as_str()) {
            eprintln!("cooee: can't save the share choice to {}: {e}", p.display());
        }
    }
}

extern "C-unwind" fn request_display_capture(
    _this: &AnyObject,
    _cmd: Sel,
    _webview: *mut AnyObject,
    _origin: *mut AnyObject,
    _frame: *mut AnyObject,
    _with_system_audio: Bool,
    decision_handler: *mut Block<dyn Fn(isize)>,
) {
    let d = target().decision();
    if let Some(app) = APP.get() {
        crate::winlog::note(app, &format!("share picker opening: {d:?}"));
    }
    if d == Decision::WindowPrompt {
        darkfield(PICKER_WILL_OPEN);
    }
    // SAFETY: WebKit hands a valid block, called here, on the main thread, before returning.
    if let Some(handler) = unsafe { RcBlock::copy(decision_handler) } {
        handler.call((d as isize,));
    }
}

/// darkfield shows its off-screen "▶ Darkfield" canvas window while these bracket a window
/// picker, since macOS's picker selects a window by clicking it on screen. The names are
/// darkfield's.
const PICKER_WILL_OPEN: &str = "com.lockyc.darkfield.pickerWillOpen";
const PICKER_DID_CLOSE: &str = "com.lockyc.darkfield.pickerDidClose";

fn darkfield(name: &str) {
    let Ok(name) = std::ffi::CString::new(name) else {
        return;
    };
    unsafe {
        let (Some(center), Some(string)) = (
            AnyClass::get(c"NSDistributedNotificationCenter"),
            AnyClass::get(c"NSString"),
        ) else {
            return;
        };
        let center: *mut AnyObject = msg_send![center, defaultCenter];
        let name: *mut AnyObject = msg_send![string, stringWithUTF8String: name.as_ptr()];
        let none: *mut AnyObject = std::ptr::null_mut();
        let _: () = msg_send![&*center, postNotificationName: name, object: none, userInfo: none, deliverImmediately: true];
    }
}

/// Meet's `getDisplayMedia` settled — shared or cancelled — so the picker is gone.
pub fn picker_closed(app: &AppHandle) {
    crate::winlog::note(app, "share picker closed (page)");
    darkfield(PICKER_DID_CLOSE);
}

/// Give the Meet webview's UI delegate the display-capture method. The delegate class is wry's,
/// shared by every webview, so adding the method once covers every later Meet window.
pub fn install(app: &AppHandle, w: &WebviewWindow) -> tauri::Result<()> {
    let _ = APP.set(app.clone());
    w.with_webview(|pw| unsafe {
        let wv = pw.inner() as *mut AnyObject;
        let delegate: *mut AnyObject = msg_send![&*wv, UIDelegate];
        if delegate.is_null() {
            return;
        }
        let cls: &AnyClass = (*delegate).class();
        let sel = sel!(_webView:requestDisplayCapturePermissionForOrigin:initiatedByFrame:withSystemAudio:decisionHandler:);
        type Imp = extern "C-unwind" fn(
            &AnyObject,
            Sel,
            *mut AnyObject,
            *mut AnyObject,
            *mut AnyObject,
            Bool,
            *mut Block<dyn Fn(isize)>,
        );
        let imp: Imp = request_display_capture;
        // Returns false when the method is already there (a second Meet window): nothing to do.
        objc2::ffi::class_addMethod(
            cls as *const AnyClass as *mut AnyClass,
            sel,
            std::mem::transmute::<Imp, objc2::runtime::Imp>(imp),
            c"v@:@@@B@?".as_ptr(),
        );
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choice_picks_webkits_picker() {
        assert_eq!(Target::Window.decision(), Decision::WindowPrompt);
        assert_eq!(Target::Screen.decision(), Decision::ScreenPrompt);
        assert_eq!(Decision::WindowPrompt as isize, 2);
        assert_eq!(Decision::ScreenPrompt as isize, 1);
    }

    #[test]
    fn the_saved_choice_round_trips_and_defaults_to_window() {
        for t in [Target::Window, Target::Screen] {
            assert_eq!(Target::parse(t.as_str()), t);
        }
        assert_eq!(Target::parse("screen\n"), Target::Screen);
        assert_eq!(Target::parse(""), Target::Window);
        assert_eq!(Target::parse("junk"), Target::Window);
    }
}
