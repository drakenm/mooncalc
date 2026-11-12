//! The bottom status bar: ephemeris backend, job progress, cursor position
//! and transient messages.

use crate::backend;
use crate::location::format_lat_lon;
use crate::settings::Settings;
use crate::status::StatusInfo;

pub fn status_bar(ui: &mut egui::Ui, settings: &Settings, status: &StatusInfo) {
    ui.horizontal(|ui| {
        ui.label(backend::status_text(settings.backend, &settings.de440_path));

        for job in status.jobs() {
            ui.separator();
            ui.spinner();
            ui.add(
                egui::ProgressBar::new(job.fraction)
                    .desired_width(160.0)
                    .text(format!("{} {:.0}%", job.label, job.fraction * 100.0)),
            );
        }

        if let Some(msg) = status.message() {
            ui.separator();
            let color = if msg.is_error {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().text_color()
            };
            ui.colored_label(color, &msg.text);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            match status.hover() {
                Some((lat, lon)) => ui.monospace(format!("Cursor: {}", format_lat_lon(lat, lon))),
                None => ui.weak("Cursor: —"),
            };
        });
    });
}
