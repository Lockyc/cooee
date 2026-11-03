//! Commands the Meet adapter calls over Tauri IPC (allowed to `https://meet.google.com` only, by
//! capabilities/meet.json).

use tauri::{AppHandle, Manager};

use crate::meet::{CallState, MeetState};

#[tauri::command]
pub fn meet_state(app: AppHandle, in_call: bool, others: usize, adapter_ok: bool, capturing: bool) {
    let next = CallState {
        in_call,
        others,
        adapter_ok,
        capturing,
    };
    let prev = app.state::<MeetState>().swap_call(next);
    if !in_call {
        crate::faces::close(&app);
    }
    if CallState::left(prev, next) {
        let a = app.clone();
        let _ = app.run_on_main_thread(move || crate::meet::go_home(&a));
    }
}

/// Write the adapter's layout snapshot to `<app data>/adapter-dump.json` and reveal it.
#[tauri::command]
pub fn adapter_dump(app: AppHandle, json: String) -> Result<(), String> {
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
