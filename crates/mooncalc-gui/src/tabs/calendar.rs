//! **Calendar** tab (placeholder). Hijri calendars.
//!
//! **Extension point:** wave 2 (Hijri visibility calendars): fill `CalendarTab::ui`; month-start scans run as jobs.
//! Builds on: mooncalc_hijri.

use super::{TabContext, inputs_summary, placeholder};

/// State of the Calendar tab.
#[derive(Debug, Default)]
pub struct CalendarTab;

impl CalendarTab {
    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            placeholder(
                ui,
                "Hijri calendars.",
                &[
                    "Tabular Islamic calendars and Umm al-Qura",
                    "Visibility-based month starts from the selected criterion and location",
                    "Month grid with Gregorian equivalents; date converter",
                ],
                "crate::tabs::calendar::CalendarTab::ui",
            );
            inputs_summary(ui, tc.settings);
        });
    }
}
