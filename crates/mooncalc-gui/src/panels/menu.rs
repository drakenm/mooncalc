//! The top menu bar.

use crate::export::{self, ExportFormat};
use crate::settings::{Settings, ThemeChoice};
use crate::status::StatusInfo;
use crate::tabs::Tab;

/// Draw the menu bar.
pub fn menu_bar(ui: &mut egui::Ui, settings: &mut Settings, status: &mut StatusInfo) {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            for format in [ExportFormat::Png, ExportFormat::Svg] {
                if ui.button(format.menu_label()).clicked() {
                    match export::export_map(format) {
                        Ok(path) => status.notify(format!("Map saved to {}", path.display())),
                        Err(e) => status.error(e),
                    }
                }
            }
            ui.separator();
            if ui.button("Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        ui.menu_button("View", |ui| {
            ui.label("Theme");
            for theme in ThemeChoice::ALL {
                ui.radio_value(&mut settings.view.theme, theme, theme.label());
            }
            ui.separator();
            ui.checkbox(&mut settings.view.show_inputs_panel, "Inputs panel");
            ui.checkbox(&mut settings.view.show_status_bar, "Status bar");
            ui.separator();
            ui.menu_button("Go to tab", |ui| {
                for tab in Tab::ALL {
                    if ui
                        .selectable_label(settings.view.active_tab == tab, tab.title())
                        .clicked()
                    {
                        settings.view.active_tab = tab;
                        ui.close();
                    }
                }
            });
        });

        ui.menu_button("Help", |ui| {
            if ui.button("About MoonCalc-rs").clicked() {
                settings.view.active_tab = Tab::About;
                ui.close();
            }
        });
    });
}
