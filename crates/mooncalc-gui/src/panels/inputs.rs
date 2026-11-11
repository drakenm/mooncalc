//! The left side panel: global inputs shared by every tab.

use crate::backend::{self, BackendChoice};
use crate::criteria::{self, CriterionOption};
use crate::location::{self, CitySuggestion};
use crate::settings::{Crescent, LocationSettings, Settings};
use crate::widgets::datetime_input::datetime_input;

/// Maximum number of city search results shown.
const CITY_RESULTS: usize = 12;

/// Transient state of the inputs panel.
#[derive(Debug, Default)]
pub struct InputsState {
    pub city_query: String,
    pub city_results: Vec<CitySuggestion>,
    /// Available criteria (loaded once at startup).
    pub criteria: Vec<CriterionOption>,
}

impl InputsState {
    pub fn new() -> Self {
        Self {
            criteria: criteria::criterion_options(),
            ..Self::default()
        }
    }
}

/// Draw the inputs panel contents.
pub fn inputs_panel(ui: &mut egui::Ui, settings: &mut Settings, state: &mut InputsState) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Date and time");
        datetime_input(ui, &mut settings.time);

        ui.separator();
        ui.heading("Location");
        location_input(ui, &mut settings.location, state);

        ui.separator();
        ui.heading("Visibility");
        criterion_input(ui, &mut settings.criterion_id, &state.criteria);
        ui.horizontal(|ui| {
            ui.label("Crescent:");
            ui.selectable_value(&mut settings.crescent, Crescent::Evening, "Evening")
                .on_hover_text(Crescent::Evening.label());
            ui.selectable_value(&mut settings.crescent, Crescent::Morning, "Morning")
                .on_hover_text(Crescent::Morning.label());
        });

        ui.separator();
        ui.heading("Ephemeris");
        backend_input(ui, settings);
    });
}

fn location_input(ui: &mut egui::Ui, loc: &mut LocationSettings, state: &mut InputsState) {
    // City search. Results come from the atlas hook.
    let search = ui.add(
        egui::TextEdit::singleline(&mut state.city_query)
            .hint_text("Search city…")
            .desired_width(f32::INFINITY),
    );
    if search.changed() {
        state.city_results = if state.city_query.trim().is_empty() {
            Vec::new()
        } else {
            location::search_cities(state.city_query.trim(), CITY_RESULTS)
        };
    }
    if !state.city_query.trim().is_empty() {
        if !location::atlas_available() {
            ui.weak("City atlas not available yet: enter coordinates below.");
        } else if state.city_results.is_empty() {
            ui.weak("No matching city.");
        }
        let mut chosen = None;
        for (i, city) in state.city_results.iter().enumerate() {
            if ui.selectable_label(false, city.display()).clicked() {
                chosen = Some(i);
            }
        }
        if let Some(i) = chosen {
            let city = state.city_results.swap_remove(i);
            loc.name = city.name;
            loc.lat_deg = city.lat_deg;
            loc.lon_deg = city.lon_deg;
            loc.elevation_m = city.elevation_m;
            state.city_query.clear();
            state.city_results.clear();
        }
    }

    egui::Grid::new("location_fields")
        .num_columns(2)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut loc.name).desired_width(150.0));
            ui.end_row();

            ui.label("Latitude");
            hemisphere_input(ui, &mut loc.lat_deg, 90.0, ['N', 'S']);
            ui.end_row();

            ui.label("Longitude");
            hemisphere_input(ui, &mut loc.lon_deg, 180.0, ['E', 'W']);
            ui.end_row();

            ui.label("Elevation");
            ui.add(
                egui::DragValue::new(&mut loc.elevation_m)
                    .range(-500.0..=9000.0)
                    .speed(1.0)
                    .suffix(" m"),
            );
            ui.end_row();
        });
    ui.weak(location::format_lat_lon(loc.lat_deg, loc.lon_deg));
}

/// Edit a signed angle as an unsigned magnitude plus a hemisphere toggle
/// (`labels[0]` for positive, `labels[1]` for negative).
fn hemisphere_input(ui: &mut egui::Ui, value_deg: &mut f64, max: f64, labels: [char; 2]) {
    ui.horizontal(|ui| {
        let mut positive = *value_deg >= 0.0;
        let mut magnitude = value_deg.abs();
        ui.add(
            egui::DragValue::new(&mut magnitude)
                .range(0.0..=max)
                .speed(0.01)
                .fixed_decimals(4)
                .suffix("°"),
        );
        for (label, is_positive) in [(labels[0], true), (labels[1], false)] {
            ui.selectable_value(&mut positive, is_positive, label.to_string());
        }
        *value_deg = if positive { magnitude } else { -magnitude };
    });
}

fn criterion_input(ui: &mut egui::Ui, criterion_id: &mut String, options: &[CriterionOption]) {
    ui.horizontal(|ui| {
        ui.label("Criterion:");
        if options.is_empty() {
            ui.add_enabled_ui(false, |ui| {
                egui::ComboBox::from_id_salt("criterion")
                    .selected_text("none available yet")
                    .show_ui(ui, |_| {});
            });
            return;
        }
        let resolved = criteria::resolve_id(options, criterion_id);
        if resolved != *criterion_id {
            *criterion_id = resolved;
        }
        let selected = criteria::find(options, criterion_id)
            .map_or(criterion_id.as_str(), |o| o.name.as_str());
        egui::ComboBox::from_id_salt("criterion")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for o in options {
                    ui.selectable_value(criterion_id, o.id.clone(), &o.name)
                        .on_hover_text(&o.reference);
                }
            });
    });
    if options.is_empty() {
        ui.weak(format!(
            "Criteria arrive with the criteria package (selected id: {criterion_id})."
        ));
    }
}

fn backend_input(ui: &mut egui::Ui, settings: &mut Settings) {
    let available = backend::available();
    for choice in BackendChoice::ALL {
        let enabled = available.contains(&choice);
        let response = ui.add_enabled(
            enabled,
            egui::RadioButton::new(settings.backend.effective() == choice, choice.label()),
        );
        if response.clicked() {
            settings.backend = choice;
        }
        if !enabled {
            response.on_disabled_hover_text(
                "Build with `--features de440` to enable the JPL DE440 backend",
            );
        }
    }
    if settings.backend.effective() == BackendChoice::De440 {
        ui.label("DE440 / DE440s file (.bsp):");
        ui.add(
            egui::TextEdit::singleline(&mut settings.de440_path)
                .hint_text("C:\\path\\to\\de440s.bsp")
                .desired_width(f32::INFINITY),
        );
        match backend::de440_path_problem(&settings.de440_path) {
            Some(problem) => ui.colored_label(ui.visuals().warn_fg_color, problem),
            None => ui.weak("File found."),
        };
    }
}
