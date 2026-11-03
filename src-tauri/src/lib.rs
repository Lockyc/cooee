mod tray;

pub fn run() {
    let config_path = cooee_config::resolve_config_path();
    let builder = shell_core::register_plugins(tauri::Builder::default(), Some(&config_path), &[]);
    builder
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray::install(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running cooee");
}
