//! **Ephemeris** tab (placeholder). Ephemeris tables for the Sun and Moon.
//!
//! **Extension point:** wave 2 (rise/set and events): fill `EphemerisTab::ui`; long tables belong in a `JobSlot` (crate::jobs).
//! Builds on: mooncalc_core (rise/set, phases, events).

use super::{TabContext, inputs_summary, placeholder};

/// State of the Ephemeris tab.
#[derive(Debug, Default)]
pub struct EphemerisTab;

impl EphemerisTab {
    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            placeholder(
                ui,
                "Ephemeris tables for the Sun and Moon.",
                &[
                "Daily/hourly tables: RA/Dec, ecliptic longitude/latitude, distance, altitude/azimuth",
                "Rise, set and transit times; phases, apogee/perigee, eclipses, libration",
                "Copy/export as CSV",
                ],
                "crate::tabs::ephemeris::EphemerisTab::ui",
            );
            inputs_summary(ui, tc.settings);
        });
    }
}
