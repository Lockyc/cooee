//! Meet's Share button: you choose a window (darkfield's canvas, one app) or your whole screen,
//! as Safari asks. WebKit routes `getDisplayMedia` to the private `WKUIDelegatePrivate`
//! `_webView:requestDisplayCapturePermissionForOrigin:…` once a UI delegate implements the public
//! camera/mic permission method (wry's does). An answer of "window" opens WebKit's window picker,
//! "screen" its screen picker. Without the method WebKit only ever offers a whole screen.

use std::sync::OnceLock;

use block2::{Block, RcBlock};
use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
use objc2::{msg_send, sel};
use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogResult};

/// `WKDisplayCapturePermissionDecision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(isize)]
enum Decision {
    Deny = 0,
    ScreenPrompt = 1,
    WindowPrompt = 2,
}

const WINDOW: &str = "Share a Window";
const SCREEN: &str = "Share Entire Screen";
const CANCEL: &str = "Cancel";

fn decision(answer: &MessageDialogResult) -> Decision {
    match answer {
        MessageDialogResult::Custom(s) if s == WINDOW => Decision::WindowPrompt,
        MessageDialogResult::Custom(s) if s == SCREEN => Decision::ScreenPrompt,
        // The dialog maps its first button to Yes and its second to No.
        MessageDialogResult::Yes => Decision::WindowPrompt,
        MessageDialogResult::No => Decision::ScreenPrompt,
        _ => Decision::Deny,
    }
}

static APP: OnceLock<AppHandle> = OnceLock::new();

/// WebKit's decision handler, carried to the dialog's callback and called back on the main thread.
struct Handler(RcBlock<dyn Fn(isize)>);
// SAFETY: the block is only ever called, and dropped, on the main thread (`run_on_main_thread`).
unsafe impl Send for Handler {}

extern "C-unwind" fn request_display_capture(
    _this: &AnyObject,
    _cmd: Sel,
    _webview: *mut AnyObject,
    _origin: *mut AnyObject,
    _frame: *mut AnyObject,
    _with_system_audio: Bool,
    decision_handler: *mut Block<dyn Fn(isize)>,
) {
    // SAFETY: WebKit hands a valid block; copying keeps it alive past this call.
    let Some(handler) = (unsafe { RcBlock::copy(decision_handler) }) else {
        return;
    };
    let Some(app) = APP.get() else {
        handler.call((Decision::Deny as isize,));
        return;
    };
    let handler = Handler(handler);
    let main = app.clone();
    app.dialog()
        .message("Share a window, such as darkfield's, or your entire screen?")
        .title("Share in Meet")
        .buttons(MessageDialogButtons::YesNoCancelCustom(
            WINDOW.into(),
            SCREEN.into(),
            CANCEL.into(),
        ))
        .show_with_result(move |answer| {
            let d = decision(&answer) as isize;
            let _ = main.run_on_main_thread(move || {
                let handler = handler;
                handler.0.call((d,));
            });
        });
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
    fn the_answer_picks_webkits_picker() {
        let custom = |s: &str| MessageDialogResult::Custom(s.into());
        assert_eq!(decision(&custom(WINDOW)), Decision::WindowPrompt);
        assert_eq!(decision(&custom(SCREEN)), Decision::ScreenPrompt);
        assert_eq!(decision(&custom(CANCEL)), Decision::Deny);
        assert_eq!(decision(&MessageDialogResult::Yes), Decision::WindowPrompt);
        assert_eq!(decision(&MessageDialogResult::No), Decision::ScreenPrompt);
        assert_eq!(decision(&MessageDialogResult::Cancel), Decision::Deny);
    }
}
