//! **Moon** tab (placeholder). Moon close-up.
//!
//! **Extension point:** wave 3 (Moon close-up): fill `MoonTab::ui`.
//! Builds on: mooncalc_core (libration, position angle).

use super::{TabContext, inputs_summary, placeholder};

/// State of the Moon tab.
#[derive(Debug, Default)]
pub struct MoonTab;

impl MoonTab {
    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            placeholder(
                ui,
                "Moon close-up.",
                &[
                "The illuminated crescent at the selected time: phase angle, position angle of the bright limb",
                "Libration and apparent size",
                ],
                "crate::tabs::moon::MoonTab::ui",
            );
            inputs_summary(ui, tc.settings);
        });
    }
}
