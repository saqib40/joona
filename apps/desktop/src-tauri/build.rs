use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    // Ensure the third-party licences file exists so Tauri's bundle resource
    // check does not panic during clippy, test, or local development runs
    // before `node scripts/notices.mjs` has run.
    let notices = manifest_dir.join("../../../target/notices/THIRD_PARTY_LICENSES.html");
    if !notices.exists() {
        if let Some(parent) = notices.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            &notices,
            "<!doctype html><title>Licences</title><p>Third-party licences</p>",
        );
    }

    // Tauri's manifest only reaches the app's executable, but tests and
    // examples link the same tray and dialog code, and without Common
    // Controls 6 they fail to start with STATUS_ENTRYPOINT_NOT_FOUND.
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri-build failed");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        let manifest = manifest_dir.join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
