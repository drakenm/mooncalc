//! Date and time input: year/month/day/hour/minute fields with steppers,
//! "Now", ±1 day and ±1 lunation buttons, and the time zone: an IANA zone
//! (with its DST rules) or a fixed offset.

use std::sync::OnceLock;

use mooncalc_atlas::LocalTime;

use crate::datetime::{self, DateTimeFields, Field};
use crate::settings::{TimeSettings, TzMode};

/// Most zone names listed at once in the zone picker.
const ZONE_LIST_LIMIT: usize = 200;

/// Every IANA zone name, read once from the embedded database.
fn zone_names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(mooncalc_atlas::tz::zone_names)
}

/// Show the date/time editor. Returns `true` if the time changed.
pub fn datetime_input(ui: &mut egui::Ui, time: &mut TimeSettings) -> bool {
    let before = time.jd_ut;
    let edit_local = time.edit_local;
    let mut fields = DateTimeFields::from_jd(time.to_display(time.jd_ut));

    ui.horizontal(|ui| {
        ui.label("Enter as:");
        ui.selectable_value(&mut time.edit_local, false, "UT");
        ui.selectable_value(&mut time.edit_local, true, "Local time");
    });
    // Re-derive the fields if the UT/local choice just changed.
    if time.edit_local != edit_local {
        fields = DateTimeFields::from_jd(time.to_display(time.jd_ut));
    }
    let gap_id = ui.make_persistent_id("datetime_gap_note");

    let mut new_fields = fields;
    egui::Grid::new("datetime_fields")
        .num_columns(4)
        .spacing([4.0, 4.0])
        .show(ui, |ui| {
            for field in Field::ALL {
                ui.label(field.label());
                if ui
                    .small_button("-")
                    .on_hover_text(format!("{} − 1", field.label()))
                    .clicked()
                {
                    new_fields = new_fields.step(field, -1);
                }
                let mut value = new_fields.get(field);
                let range = new_fields.range(field);
                let width = if field == Field::Year { 56.0 } else { 40.0 };
                let response = ui.add_sized(
                    [width, ui.spacing().interact_size.y],
                    egui::DragValue::new(&mut value).range(range).speed(0.2),
                );
                if response.changed() {
                    new_fields = new_fields.with(field, value);
                }
                if ui
                    .small_button("+")
                    .on_hover_text(format!("{} + 1", field.label()))
                    .clicked()
                {
                    new_fields = new_fields.step(field, 1);
                }
                ui.end_row();
            }
        });
    if new_fields != fields {
        let entered = new_fields.to_jd();
        time.jd_ut = time.from_display(entered);
        // A local time skipped by a DST change was moved forward: say so.
        let note = match time.zone().resolve_local(entered) {
            LocalTime::Gap { .. } if time.edit_local => Some(format!(
                "{new_fields} does not exist in this zone (clocks went forward); \
                 using {}.",
                DateTimeFields::from_jd(time.jd_local())
            )),
            _ => None,
        };
        ui.data_mut(|d| d.insert_temp(gap_id, note));
    }
    if let Some(note) = ui.data(|d| d.get_temp::<Option<String>>(gap_id)).flatten() {
        ui.colored_label(ui.visuals().warn_fg_color, note);
    }

    ui.horizontal_wrapped(|ui| {
        if ui
            .button("« Lunation")
            .on_hover_text(format!(
                "Back one mean lunation ({:.4} days)",
                datetime::SYNODIC_MONTH_DAYS
            ))
            .clicked()
        {
            time.jd_ut = datetime::step_lunations(time.jd_ut, -1);
        }
        if ui.button("« Day").on_hover_text("Back one day").clicked() {
            time.jd_ut -= 1.0;
        }
        if ui
            .button("Now")
            .on_hover_text("Set to the current time")
            .clicked()
        {
            time.set_now();
        }
        if ui
            .button("Day »")
            .on_hover_text("Forward one day")
            .clicked()
        {
            time.jd_ut += 1.0;
        }
        if ui
            .button("Lunation »")
            .on_hover_text(format!(
                "Forward one mean lunation ({:.4} days)",
                datetime::SYNODIC_MONTH_DAYS
            ))
            .clicked()
        {
            time.jd_ut = datetime::step_lunations(time.jd_ut, 1);
        }
    });

    zone_input(ui, time);
    ui.checkbox(&mut time.start_at_now, "Start at the current time");

    let ut = DateTimeFields::from_jd(time.jd_ut);
    let local = DateTimeFields::from_jd(time.jd_local());
    let zone = time.zone();
    ui.weak(format!("UT     {ut}"));
    ui.weak(format!(
        "Local  {local} ({})",
        datetime::zone_abbrev(zone, time.jd_ut)
    ));
    if let LocalTime::Fold { earlier, .. } = time.local_resolution() {
        let which = if (earlier - time.jd_ut).abs() < 1e-9 {
            "first"
        } else {
            "second"
        };
        ui.weak(format!(
            "This local time occurs twice (clocks go back); this is the {which}."
        ));
    }
    ui.weak(format!("JD     {:.5}", time.jd_ut));

    time.jd_ut != before
}

/// The time-zone choice: an IANA zone (picked from a filtered list, or set
/// by choosing a city) or a fixed offset.
fn zone_input(ui: &mut egui::Ui, time: &mut TimeSettings) {
    ui.horizontal(|ui| {
        ui.label("Time zone:");
        let zone_name = time.zone().iana_name().map(str::to_owned);
        if ui
            .selectable_label(time.tz_mode == TzMode::Zone, "Zone")
            .on_hover_text("An IANA time zone: local times follow its DST and historical rules")
            .clicked()
        {
            let name = zone_name
                .clone()
                .or_else(|| (!time.tz_name.is_empty()).then(|| time.tz_name.clone()))
                .unwrap_or_else(|| "Etc/UTC".into());
            time.set_zone(&name);
        }
        if ui
            .selectable_label(time.tz_mode == TzMode::Fixed, "Fixed offset")
            .on_hover_text("One offset from UT for every date")
            .clicked()
        {
            time.set_fixed(time.offset_hours());
        }
    });
    match time.tz_mode {
        TzMode::Zone => {
            let filter_id = ui.make_persistent_id("tz_zone_filter");
            let selected = time
                .zone()
                .iana_name()
                .map_or_else(|| format!("{} (unknown)", time.tz_name), str::to_owned);
            let mut picked = None;
            egui::ComboBox::from_id_salt("tz_zone")
                .selected_text(selected)
                .width(ui.available_width().min(260.0))
                .height(320.0)
                .show_ui(ui, |ui| {
                    let mut filter: String =
                        ui.data_mut(|d| d.get_temp(filter_id)).unwrap_or_default();
                    ui.add(
                        egui::TextEdit::singleline(&mut filter)
                            .hint_text("Filter, e.g. london")
                            .desired_width(f32::INFINITY),
                    )
                    .request_focus();
                    let needle = filter.trim().to_lowercase();
                    let mut shown = 0;
                    for name in zone_names()
                        .iter()
                        .filter(|n| needle.is_empty() || n.to_lowercase().contains(&needle))
                    {
                        if shown == ZONE_LIST_LIMIT {
                            ui.weak("… type to narrow the list");
                            break;
                        }
                        shown += 1;
                        if ui
                            .selectable_label(time.tz_name == *name, name.as_str())
                            .clicked()
                        {
                            picked = Some(name.clone());
                        }
                    }
                    if shown == 0 {
                        ui.weak("No matching zone.");
                    }
                    ui.data_mut(|d| d.insert_temp(filter_id, filter));
                });
            if let Some(name) = picked {
                time.set_zone(&name);
            }
            ui.weak(format!(
                "{} at the selected time",
                datetime::zone_text(time.zone(), time.jd_ut)
            ))
            .on_hover_text(format!(
                "Rules from the IANA Time Zone Database {}",
                mooncalc_atlas::tz::TZDB_VERSION
            ));
        }
        TzMode::Fixed => {
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::DragValue::new(&mut time.tz_offset_hours)
                        .range(-14.0..=14.0)
                        .speed(0.05)
                        .fixed_decimals(2)
                        .custom_formatter(|h, _| datetime::format_utc_offset(h))
                        .custom_parser(parse_utc_offset),
                );
                if response.changed() {
                    // Snap to quarter hours (every real zone offset is a multiple).
                    time.tz_offset_hours = (time.tz_offset_hours * 4.0).round() / 4.0;
                }
                ui.weak("for every date (no DST)");
            });
        }
    }
}

/// Parse `UTC+05:30`, `+5:30`, `-3`, `5.5` into hours.
pub fn parse_utc_offset(text: &str) -> Option<f64> {
    let t = text.trim();
    let t = t
        .strip_prefix("UTC")
        .or_else(|| t.strip_prefix("utc"))
        .or_else(|| t.strip_prefix("GMT"))
        .unwrap_or(t)
        .trim();
    if t.is_empty() {
        return Some(0.0);
    }
    let (sign, rest) = match t.as_bytes()[0] {
        b'-' => (-1.0, &t[1..]),
        b'+' => (1.0, &t[1..]),
        _ => (1.0, t),
    };
    let hours = match rest.split_once(':') {
        Some((h, m)) => h.trim().parse::<f64>().ok()? + m.trim().parse::<f64>().ok()? / 60.0,
        None => rest.trim().parse::<f64>().ok()?,
    };
    Some(sign * hours)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_utc_offsets() {
        assert_eq!(parse_utc_offset("UTC+05:30"), Some(5.5));
        assert_eq!(parse_utc_offset("-3"), Some(-3.0));
        assert_eq!(parse_utc_offset("+9:45"), Some(9.75));
        assert_eq!(parse_utc_offset("5.5"), Some(5.5));
        assert_eq!(parse_utc_offset("UTC"), Some(0.0));
        assert_eq!(parse_utc_offset("abc"), None);
    }

    #[test]
    fn formatted_offsets_parse_back() {
        for h in [-12.0, -9.5, 0.0, 3.0, 5.75, 14.0] {
            assert_eq!(parse_utc_offset(&datetime::format_utc_offset(h)), Some(h));
        }
    }
}
