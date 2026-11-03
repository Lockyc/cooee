fn main() {
    // Build stamp (BUILD_GIT_SHA/BUILD_DATE) + the shared release scripts materialized into the
    // repo-root scripts/ (git-ignored; the tracked scripts/tooling.env supplies cooee's params).
    shell_core::build_stamp();
    shell_core::materialize_scripts(std::path::Path::new("../scripts"))
        .expect("materialize shell-core scripts");
    tauri_build::build()
}
