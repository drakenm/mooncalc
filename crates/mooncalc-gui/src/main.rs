//! MoonCalc desktop application entry point; see the `mooncalc_gui` library.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    mooncalc_gui::run()
}
