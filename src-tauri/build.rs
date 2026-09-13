// The command list turns Tauri's ACL on for this app's own commands, so
// `capabilities/default.json` has to name each one. A name that does not match
// a command fails the build rather than failing at runtime.
fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "connect",
            "retry",
            "current_instance",
            "open_external",
            "app_version",
        ]),
    ))
    .expect("the Tauri build script failed");
}
