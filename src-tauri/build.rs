fn main() {
    // On Windows, Kairo's manifest (Common Controls 6, which the system's dialogs need) is linked
    // into every program the crate builds, its test runner too, rather than into the app alone as
    // Tauri does by default: a test runner without it fails to start once a dialog's function is
    // linked in (STATUS_ENTRYPOINT_NOT_FOUND). The manifest is Tauri's own default, unchanged.
    let msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let mut windows = tauri_build::WindowsAttributes::new();
    if msvc {
        windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("windows").join("app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows)).expect("failed to run the Tauri build script");
}
