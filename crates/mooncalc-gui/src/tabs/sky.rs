//! **Sky** tab (placeholder). Sky view at the selected time and place.
//!
//! **Extension point:** wave 3 (sky view / star chart): fill `SkyTab::ui`, drawing with egui's painter.
//! Builds on: mooncalc_core::coords (horizontal coordinates).

use super::{TabContext, inputs_summary, placeholder};

/// State of the Sky tab.
#[derive(Debug, Default)]
pub struct SkyTab;

impl SkyTab {
    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            placeholder(
                ui,
                "Sky view at the selected time and place.",
                &[
                    "Western horizon at sunset with the Sun, Moon and the crescent's orientation",
                    "Star chart background with bright stars and planets",
                ],
                "crate::tabs::sky::SkyTab::ui",
            );
            inputs_summary(ui, tc.settings);
        });
    }
}
