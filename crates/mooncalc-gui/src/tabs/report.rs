//! **Report** tab (placeholder). Location report: observing circumstances for the selected observer and date.
//!
//! **Extension point:** wave 2 (circumstances + GUI location report): fill `ReportTab::ui`; compute in a `JobSlot` from `poll` when `Settings::instant()`/`observer()`/`crescent` change.
//! Builds on: mooncalc_visibility::geometry::CrescentGeometry, crate::backend::create_ephemeris.

use super::{TabContext, inputs_summary, placeholder};

/// State of the Report tab.
#[derive(Debug, Default)]
pub struct ReportTab;

impl ReportTab {
    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            placeholder(
                ui,
                "Location report: observing circumstances for the selected observer and date.",
                &[
                "Sunset/sunrise, moonset/moonrise and the best time (Yallop: sunset + 4/9 lag)",
                "Crescent geometry at the best time: ARCL, ARCV, DAZ, crescent width W, lag, Moon age",
                "Every criterion's verdict (Yallop q, Odeh V, ...) with its zone",
                "Times of the conjunction (new Moon) and the Moon's altitude/azimuth",
                ],
                "crate::tabs::report::ReportTab::ui",
            );
            inputs_summary(ui, tc.settings);
        });
    }
}
