//! Build script: Tauri code generation plus a guard for the frontend bundle.
//!
//! `tauri::generate_context!` embeds `build.frontendDist` from `tauri.conf.json`
//! (`frontend/tauri/dist`) and fails to compile if that directory is missing
//! whenever the `custom-protocol` feature is on. The directory is build output
//! of `npm run build`, so it is not committed. To keep every `cargo` command
//! working on a fresh checkout without running npm first, this script writes a
//! one-page placeholder into it when no `index.html` is present. A real
//! `npm run build` (Vite, `emptyOutDir`) replaces it. The directory is
//! git-ignored, so this never dirties the worktree.

use std::{fs, path::Path};

/// Commands registered in `src/lib.rs`. Tauri generates an `allow-<command>`
/// permission for each, and `capabilities/default.json` grants exactly these.
const COMMANDS: &[&str] = &[
    "app_info",
    "criteria_list",
    "compute_visibility_map",
    "export_map_png",
    "save_map_png",
    "location_report",
    "sky_scene",
    "sky_stars",
    "trajectory_day",
    "moon_view",
    "moon_features",
    "search_cities",
    "nearest_city",
    "earliest_sighting",
    "basemap",
    "load_de440",
];

const PLACEHOLDER: &str = r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>MoonCalc</title></head>
<body style="font-family: system-ui, sans-serif; padding: 2rem">
<h1>MoonCalc</h1>
<p>The frontend has not been built. Run <code>npm ci &amp;&amp; npm run build</code>
in <code>frontend/tauri/</code>, then rebuild, or use <code>npx tauri dev</code>.</p>
</body>
</html>
"#;

fn main() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../frontend/tauri/dist");
    if !dist.join("index.html").exists() {
        fs::create_dir_all(&dist).expect("create frontend/tauri/dist");
        fs::write(dist.join("index.html"), PLACEHOLDER).expect("write placeholder index.html");
    }
    // Re-embed the bundle when the frontend is rebuilt.
    println!("cargo:rerun-if-changed={}", dist.display());

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri-build failed");
}
