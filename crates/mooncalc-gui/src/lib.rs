//! MoonCalc-rs desktop application (egui/eframe).
//!
//! # Structure
//!
//! * [`app`]: [`app::MoonCalcApp`] owns the persisted [`settings::Settings`],
//!   the transient [`app::UiState`] and the [`tabs::Tabs`], and lays out the
//!   panels.
//! * [`panels`]: menu bar, global-inputs side panel, status bar.
//! * [`tabs`]: one module per central tab (Map, Report, Ephemeris, Calendar,
//!   Sky, Moon, Trajectory, About). Wave-2/3 features fill these in; see the
//!   module docs for the `poll`/`ui` contract and [`tabs::TabContext`].
//! * [`jobs`]: background computation with progress, cancellation and
//!   replace-on-new-request.
//! * [`widgets`]: the date/time input and the zoomable map view.
//! * Hooks for other crates: [`backend::create_ephemeris`] (ephemeris
//!   backends), [`location::search_cities`] (city atlas),
//!   [`criteria::criterion_options`] (criteria), [`tabs::map::compute_map_image`]
//!   (visibility grid + renderer), [`export::export_map`] (PNG/SVG export),
//!   [`attributions::ATTRIBUTIONS`] (credits).

pub mod app;
pub mod attributions;
pub mod backend;
pub mod criteria;
pub mod datetime;
pub mod export;
pub mod icon;
pub mod jobs;
pub mod location;
pub mod panels;
pub mod placeholder_map;
pub mod settings;
pub mod sky;
pub mod status;
pub mod tabs;
pub mod widgets;

/// Window title.
pub const WINDOW_TITLE: &str = "MoonCalc";

/// eframe options: window size, title and icon.
pub fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(WINDOW_TITLE)
            .with_app_id("mooncalc")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 560.0])
            .with_icon(icon::app_icon()),
        ..Default::default()
    }
}

/// Run the application until the window closes.
pub fn run() -> eframe::Result {
    eframe::run_native(
        WINDOW_TITLE,
        native_options(),
        Box::new(|cc| Ok(Box::new(app::MoonCalcApp::new(cc)))),
    )
}
