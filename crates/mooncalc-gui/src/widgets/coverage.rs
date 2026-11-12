//! The notice a view shows instead of its content when the date or range
//! lies outside the ephemeris coverage (see
//! [`crate::backend::ActiveEphemeris::span_problem`]).

/// A framed warning: what is outside the coverage and how to fix it.
pub fn coverage_notice(ui: &mut egui::Ui, message: &str) {
    egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.0, ui.visuals().warn_fg_color))
        .show(ui, |ui| {
            ui.colored_label(ui.visuals().warn_fg_color, format!("⚠ {message}"));
            ui.weak(
                "Not computed. Choose a date inside the coverage, or a Meeus backend in the \
                 Ephemeris section of the inputs panel.",
            );
        });
}
