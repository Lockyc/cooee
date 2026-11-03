mod bridge;
mod meet;
mod tray;

use tauri_plugin_dialog::DialogExt;

pub fn run() {
    let config_path = cooee_config::resolve_config_path();
    let (config, load_error) = match cooee_config::load_config(&config_path) {
        Ok(c) => (c, None),
        Err(e) => (
            cooee_config::parse_and_validate("").expect("the empty config is valid"),
            Some(format!(
                "{}: {e}\n\ncooee is running with its defaults.",
                config_path.display()
            )),
        ),
    };
    let builder = shell_core::register_plugins(tauri::Builder::default(), Some(&config_path), &[]);
    builder
        .plugin(tauri_plugin_dialog::init())
        .manage(config)
        .manage(meet::MeetState::default())
        .invoke_handler(tauri::generate_handler![
            bridge::meet_state,
            bridge::adapter_dump
        ])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray::install(app.handle())?;
            if let Some(msg) = &load_error {
                app.dialog()
                    .message(msg)
                    .title("cooee config error")
                    .show(|_| {});
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running cooee");
}
