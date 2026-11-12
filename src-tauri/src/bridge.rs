//! Commands the Meet adapter calls over Tauri IPC (allowed to `https://meet.google.com` only, by
//! capabilities/meet.json).

use tauri::{AppHandle, Manager};

use crate::meet::{CallState, MeetState};

#[tauri::command]
pub fn meet_state(
    app: AppHandle,
    in_call: bool,
    others: usize,
    adapter_ok: bool,
    capturing: bool,
    sharing: bool,
) {
    let next = CallState {
        in_call,
        others,
        adapter_ok,
        capturing,
        sharing,
    };
    let prev = app.state::<MeetState>().swap_call(next);
    crate::popups::close_disallowed(&app, next);
    if CallState::left(prev, next) {
        let a = app.clone();
        let _ = app.run_on_main_thread(move || crate::meet::go_home(&a));
    }
}

/// Meet's screen-share request settled: the window picker, if one opened, has closed.
#[tauri::command]
pub fn share_picker_closed() {
    crate::screenshare::picker_closed();
}

/// Write the adapter's layout snapshot to `<app data>/adapter-dump.json` and reveal it — once per
/// tray request; an unrequested call is ignored.
#[tauri::command]
pub fn adapter_dump(app: AppHandle, json: String) -> Result<(), String> {
    if !app.state::<MeetState>().take_snapshot_request() {
        return Ok(());
    }
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("adapter-dump.json");
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .status();
    Ok(())
}
