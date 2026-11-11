//! The left side panel: global inputs shared by every tab.

use mooncalc_atlas::UserAtlas;

use crate::backend::{self, BackendChoice};
use crate::criteria::{self, CriterionOption};
use crate::location::{self, CitySuggestion};
use crate::settings::{Crescent, Settings};
use crate::widgets::datetime_input::datetime_input;

/// Maximum number of city search results shown.
const CITY_RESULTS: usize = 12;

/// Id of the city search box (for keyboard navigation of the results).
const CITY_SEARCH_ID: &str = "city_search";

/// Transient state of the inputs panel.
#[derive(Debug, Default)]
pub struct InputsState {
    pub city_query: String,
    pub city_results: Vec<CitySuggestion>,
    /// Index of the keyboard-highlighted result.
    pub city_highlight: usize,
    /// A query was typed while the atlas was loading: search once it is ready.
    pub city_search_pending: bool,
    /// The user's saved locations (persisted by the app under
    /// [`location::USER_ATLAS_KEY`]).
    pub user_atlas: UserAtlas,
    /// Name typed for "save current location".
    pub new_user_name: String,
    /// User location being renamed, and the edited name.
    pub renaming: Option<(usize, String)>,
    /// Last user-atlas error, shown under the manager.
    pub user_error: Option<String>,
    /// Stored JSON that failed to parse, kept so saving does not destroy it
    /// while no new location has been added.
    unreadable_user_json: Option<String>,
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

    /// Restore the user locations from their stored JSON (see
    /// [`location::load_user_atlas`]).
    pub fn load_user_atlas(&mut self, json: Option<&str>) {
        let (atlas, err) = location::load_user_atlas(json);
        self.user_atlas = atlas;
        self.unreadable_user_json = err.as_ref().and(json).map(str::to_owned);
        self.user_error = err.map(|e| format!("Saved locations could not be read: {e}"));
    }

    /// The JSON to store for the user locations. Unreadable stored data is
    /// written back unchanged until the user saves a new location.
    pub fn user_atlas_json(&self) -> String {
        match &self.unreadable_user_json {
            Some(raw) if self.user_atlas.is_empty() => raw.clone(),
            _ => self.user_atlas.to_json_string(),
        }
    }

    fn run_city_search(&mut self) {
        self.city_highlight = 0;
        if self.city_query.trim().is_empty() {
            self.city_results.clear();
            self.city_search_pending = false;
            return;
        }
        match location::search_cities(&self.user_atlas, &self.city_query, CITY_RESULTS) {
            Some(results) => {
                self.city_results = results;
                self.city_search_pending = false;
            }
            None => {
                self.city_results.clear();
                self.city_search_pending = true;
            }
        }
    }

    fn clear_city_search(&mut self) {
        self.city_query.clear();
        self.city_results.clear();
        self.city_highlight = 0;
        self.city_search_pending = false;
    }
}

/// Draw the inputs panel contents.
pub fn inputs_panel(ui: &mut egui::Ui, settings: &mut Settings, state: &mut InputsState) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Date and time");
        datetime_input(ui, &mut settings.time);

        ui.separator();
        ui.heading("Location");
        location_input(ui, settings, state);

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

fn location_input(ui: &mut egui::Ui, settings: &mut Settings, state: &mut InputsState) {
    city_search(ui, settings, state);

    let loc = &mut settings.location;
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

    if let Some((city, km)) = location::nearest_city(loc.lat_deg, loc.lon_deg) {
        ui.horizontal_wrapped(|ui| {
            ui.weak("Nearest city:");
            ui.label(format!("{} ({km:.0} km)", city.display_name))
                .on_hover_text(city.display());
        });
    }

    user_locations(ui, settings, state);
}

/// The type-ahead city search box and its result list.
fn city_search(ui: &mut egui::Ui, settings: &mut Settings, state: &mut InputsState) {
    let id = ui.make_persistent_id(CITY_SEARCH_ID);
    // Arrow keys move the highlight while the box has focus.
    let mut moved = false;
    if !state.city_results.is_empty() && ui.memory(|m| m.has_focus(id)) {
        let (down, up) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
            )
        });
        let last = state.city_results.len() - 1;
        if down {
            state.city_highlight = (state.city_highlight + 1).min(last);
        }
        if up {
            state.city_highlight = state.city_highlight.saturating_sub(1);
        }
        moved = down || up;
    }

    let search = ui.add(
        egui::TextEdit::singleline(&mut state.city_query)
            .id(id)
            .hint_text("Search city or saved location…")
            .desired_width(f32::INFINITY),
    );
    if search.changed() || (state.city_search_pending && location::atlas().is_some()) {
        state.run_city_search();
    }
    let enter = search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if escape && (search.has_focus() || search.lost_focus()) {
        state.clear_city_search();
    }

    if !state.city_query.trim().is_empty() {
        let mut chosen = enter.then_some(state.city_highlight);
        if location::atlas().is_none() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak("Loading atlas…");
            });
        } else if state.city_results.is_empty() {
            ui.weak("No matching city.");
        } else {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .id_salt("city_results")
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for (i, city) in state.city_results.iter().enumerate() {
                            let selected = i == state.city_highlight;
                            let response = ui.add(
                                egui::Button::selectable(selected, city.display())
                                    .min_size(egui::vec2(ui.available_width(), 0.0)),
                            );
                            let response = match &city.timezone {
                                Some(tz) => response.on_hover_text(format!("Time zone {tz}")),
                                None => response.on_hover_text("Saved location"),
                            };
                            if response.clicked() {
                                chosen = Some(i);
                            }
                            if selected && moved {
                                response.scroll_to_me(None);
                            }
                        }
                    });
                ui.weak(location::ATLAS_CREDIT);
            });
        }

        if let Some(i) = chosen.filter(|&i| i < state.city_results.len()) {
            let city = state.city_results.swap_remove(i);
            city.apply_to(&mut settings.location);
            city.apply_zone_to(&mut settings.time);
            state.clear_city_search();
        }
    }
}

/// What the user clicked in the saved-locations list.
enum UserAction {
    Use(usize),
    StartRename(usize, String),
    CancelRename,
    Rename(usize, String),
    Delete(usize),
}

/// The saved-locations manager: save the current location, then use,
/// rename or delete saved ones.
fn user_locations(ui: &mut egui::Ui, settings: &mut Settings, state: &mut InputsState) {
    egui::CollapsingHeader::new(format!("My locations ({})", state.user_atlas.len()))
        .id_salt("user_locations")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.new_user_name)
                        .hint_text(settings.location.name.as_str())
                        .desired_width(140.0),
                );
                if ui
                    .button("Save current")
                    .on_hover_text(
                        "Save the current coordinates, elevation and time zone under \
                         this name (the location name if left empty)",
                    )
                    .clicked()
                {
                    let name = if state.new_user_name.trim().is_empty() {
                        settings.location.name.clone()
                    } else {
                        state.new_user_name.clone()
                    };
                    let loc =
                        location::user_location_from(&name, &settings.location, &settings.time);
                    match state.user_atlas.add(loc) {
                        Ok(_) => {
                            state.new_user_name.clear();
                            state.user_error = None;
                        }
                        Err(e) => state.user_error = Some(e.to_string()),
                    }
                }
            });

            let mut action = None;
            for (i, u) in state.user_atlas.locations().iter().enumerate() {
                ui.horizontal(|ui| match &mut state.renaming {
                    Some((r, name)) if *r == i => {
                        let edit = ui.add(egui::TextEdit::singleline(name).desired_width(140.0));
                        let enter =
                            edit.lost_focus() && ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                        if ui.small_button("OK").clicked() || enter {
                            action = Some(UserAction::Rename(i, name.clone()));
                        }
                        if ui.small_button("Cancel").clicked() {
                            action = Some(UserAction::CancelRename);
                        }
                    }
                    _ => {
                        if ui
                            .small_button("Use")
                            .on_hover_text("Set the observer to this location")
                            .clicked()
                        {
                            action = Some(UserAction::Use(i));
                        }
                        ui.label(&u.name).on_hover_text(format!(
                            "{}, {:.0} m, {}",
                            location::format_lat_lon(u.lat_deg, u.lon_deg),
                            u.elevation_m,
                            u.zone()
                        ));
                        if ui.small_button("Rename").clicked() {
                            action = Some(UserAction::StartRename(i, u.name.clone()));
                        }
                        if ui.small_button("Delete").clicked() {
                            action = Some(UserAction::Delete(i));
                        }
                    }
                });
            }
            match action {
                Some(UserAction::Use(i)) => {
                    if let Some(u) = state.user_atlas.get(i) {
                        let place = CitySuggestion::from_user(u);
                        place.apply_to(&mut settings.location);
                        place.apply_zone_to(&mut settings.time);
                    }
                }
                Some(UserAction::StartRename(i, name)) => state.renaming = Some((i, name)),
                Some(UserAction::CancelRename) => state.renaming = None,
                Some(UserAction::Rename(i, name)) => {
                    match location::rename_user_location(&mut state.user_atlas, i, &name) {
                        Ok(()) => {
                            state.renaming = None;
                            state.user_error = None;
                        }
                        Err(e) => state.user_error = Some(e.to_string()),
                    }
                }
                Some(UserAction::Delete(i)) => {
                    state.user_atlas.remove(i);
                    state.renaming = None;
                }
                None => {}
            }
            if state.user_atlas.is_empty() {
                ui.weak("No saved locations yet.");
            }
            if let Some(err) = &state.user_error {
                ui.colored_label(ui.visuals().warn_fg_color, err);
            }
        });
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
        ui.horizontal(|ui| {
            if ui.button("Browse…").clicked()
                && let Some(p) = crate::export::ask_bsp_path(&settings.de440_path)
            {
                settings.de440_path = p.display().to_string();
            }
            if ui
                .button("Reload")
                .on_hover_text("Reopen the file (after replacing it)")
                .clicked()
            {
                backend::reload();
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut settings.de440_path)
                .hint_text("C:\\path\\to\\de440s.bsp")
                .desired_width(f32::INFINITY),
        );
    }
    // The ephemeris actually in use, its coverage, and any fallback.
    let active = backend::resolve(settings.backend, &settings.de440_path);
    match &active.notice {
        Some(notice) => {
            ui.colored_label(ui.visuals().warn_fg_color, notice);
        }
        None => {
            ui.weak(format!("In use: {}", active.eph.name()));
        }
    }
    ui.weak(format!("Coverage: {}", active.coverage_text()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_user_json_is_kept_until_replaced() {
        let mut state = InputsState::default();
        state.load_user_atlas(Some("{broken"));
        assert!(state.user_error.is_some());
        assert_eq!(state.user_atlas_json(), "{broken");

        let loc = location::user_location_from("Home", &Default::default(), &Default::default());
        state.user_atlas.add(loc).unwrap();
        let json = state.user_atlas_json();
        let mut again = InputsState::default();
        again.load_user_atlas(Some(&json));
        assert!(again.user_error.is_none());
        assert_eq!(again.user_atlas.get(0).unwrap().name, "Home");
    }
}
