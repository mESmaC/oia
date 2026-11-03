use std::path::Path;

/// Gitignored output of `scripts/fetch-ui-vendor.*`; its `VERSION` file is written last.
const CESIUM_VERSION_FILE: &str = "ui/vendor/cesium/VERSION";
/// Longest `VERSION` content accepted; anything longer is not a version string.
const MAX_VERSION_BYTES: usize = 32;

fn main() {
    // `tauri::generate_context!` embeds `ui/` only when this crate recompiles. Watching the
    // directory reruns this script, and so rebuilds and re-embeds the crate, when a page
    // file changes or the CesiumJS build is fetched after an earlier build. A missing path
    // would rerun every build, so the existing directory is watched, not the VERSION file.
    println!("cargo:rerun-if-changed=ui");
    let bundled = bundled_cesium_version(Path::new(CESIUM_VERSION_FILE));
    match &bundled {
        Some(version) => println!("cargo:rustc-env=OIA_CESIUM_BUNDLED_VERSION={version}"),
        None if cfg!(feature = "desktop") => println!(
            "cargo:warning=The CesiumJS build is not in crates/oia-app/ui/vendor/cesium; the Globe renderer will report it as not present. Run scripts/run.sh fetch (or .\\scripts\\run.ps1 fetch) to install it; the next build embeds it."
        ),
        None => {}
    }
    #[cfg(feature = "desktop")]
    tauri_build::build();
}

/// The version recorded by the fetch script, or `None` when it is absent or unreadable as one.
fn bundled_cesium_version(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let version = text.trim();
    let plausible = !version.is_empty()
        && version.len() <= MAX_VERSION_BYTES
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    plausible.then(|| version.to_owned())
}
