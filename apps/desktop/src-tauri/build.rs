fn main() {
    // Declaring the app's commands makes Tauri generate `allow-<command>` permissions and
    // enforce them: a command is reachable from the UI only if a capability grants it.
    let mut attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "get_app_info",
            // CAP-02A local self-preview (ADR-007).
            "get_capture_displays",
            "get_local_preview_status",
            "start_local_preview",
            "stop_local_preview",
            "get_local_preview_frame",
        ]));

    // tauri-build embeds the Windows app manifest (Common Controls v6) into the binary only.
    // Test executables that link Tauri then fail to start with STATUS_ENTRYPOINT_NOT_FOUND,
    // so the same manifest is embedded through the linker for every target instead.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }

    tauri_build::try_build(attributes).expect("tauri-build failed");
}
