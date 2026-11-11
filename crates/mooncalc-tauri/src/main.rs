//! MoonCalc desktop application (Tauri shell), binary `mooncalc-tauri`.

// Release builds on Windows get no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    mooncalc_tauri_lib::run();
}
