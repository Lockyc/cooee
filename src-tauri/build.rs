fn main() {
    // Build stamp (BUILD_GIT_SHA/BUILD_DATE) + the shared release scripts materialized into the
    // repo-root scripts/ (git-ignored; the tracked scripts/tooling.env supplies cooee's params).
    shell_core::build_stamp();
    shell_core::materialize_scripts(std::path::Path::new("../scripts"))
        .expect("materialize shell-core scripts");
    // Declaring the app's commands makes each one permission-gated (`allow-<command>`), so
    // capabilities/meet.json can grant exactly these to Meet's remote origin.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "meet_state",
            "adapter_dump",
            "snap_talk",
            "snap_aside",
            "cycle_faces_size",
            "save_talk",
            "save_aside",
        ]),
    ))
    .expect("tauri build");
}
