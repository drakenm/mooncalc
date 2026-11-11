//! **Trajectory** tab (placeholder). Local trajectory of the Sun and Moon after sunset (mooncalc.org style).
//!
//! **Extension point:** wave 3 (trajectory view): fill `TrajectoryTab::ui`.
//! Builds on: mooncalc_core::coords.

use super::{TabContext, inputs_summary, placeholder};

/// State of the Trajectory tab.
#[derive(Debug, Default)]
pub struct TrajectoryTab;

impl TrajectoryTab {
    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            placeholder(
                ui,
                "Local trajectory of the Sun and Moon after sunset (mooncalc.org style).",
                &[
                    "Altitude/azimuth tracks of the Sun and Moon from sunset to moonset",
                    "Best-time marker and twilight shading",
                ],
                "crate::tabs::trajectory::TrajectoryTab::ui",
            );
            inputs_summary(ui, tc.settings);
        });
    }
}
