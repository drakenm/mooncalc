//! **About** tab: version, licence and credits
//! (credits come from [`crate::attributions::ATTRIBUTIONS`]).

use crate::attributions::ATTRIBUTIONS;

use super::TabContext;

/// Application display name.
pub const APP_NAME: &str = "MoonCalc-rs";
/// Application version (from Cargo).
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The project licence text.
pub const LICENSE_TEXT: &str = include_str!("../../../../LICENSE");

#[derive(Debug, Default)]
pub struct AboutTab;

impl AboutTab {
    pub fn ui(&mut self, ui: &mut egui::Ui, _tc: &mut TabContext<'_>) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading(format!("{APP_NAME} {APP_VERSION}"));
            ui.label("Lunar crescent visibility: world maps, location reports, ephemerides and Hijri calendars.");
            ui.add_space(4.0);
            ui.label(
                "Inspired by Dr. Monzur Ahmed's MoonCalc 6.0. This is an independent \
                 reimplementation from published algorithms; it is not affiliated with \
                 or derived from the original program's code.",
            );
            ui.add_space(4.0);
            ui.label(format!(
                "Build: ephemeris backends — {}.",
                crate::backend::available()
                    .iter()
                    .map(|b| b.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));

            ui.add_space(12.0);
            ui.heading("Credits");
            // One stacked entry per credit: a two-column grid squeezes the
            // names when the usage text is long.
            for a in ATTRIBUTIONS {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(a.name);
                    ui.weak(format!("· {}", a.license));
                });
                ui.add(egui::Label::new(a.usage).wrap());
                if let Some(url) = a.url {
                    ui.hyperlink(url);
                }
                ui.add_space(8.0);
            }

            ui.add_space(12.0);
            ui.heading("Licence");
            ui.label("MoonCalc-rs is released under the MIT License:");
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.add(egui::Label::new(egui::RichText::new(LICENSE_TEXT).monospace().small()).wrap());
            });
        });
    }
}
