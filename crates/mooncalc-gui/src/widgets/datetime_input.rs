//! Date and time input: year/month/day/hour/minute fields with steppers,
//! "Now", ±1 day and ±1 lunation buttons, and a time-zone offset.

use crate::datetime::{self, DateTimeFields, Field};
use crate::settings::TimeSettings;

/// Show the date/time editor. Returns `true` if the time changed.
pub fn datetime_input(ui: &mut egui::Ui, time: &mut TimeSettings) -> bool {
    let before = time.jd_ut;
    let offset = time.display_offset_days();
    let mut fields = DateTimeFields::from_jd(time.jd_ut + offset);

    ui.horizontal(|ui| {
        ui.label("Enter as:");
        ui.selectable_value(&mut time.edit_local, false, "UT");
        ui.selectable_value(&mut time.edit_local, true, "Local time");
    });
    // Re-derive the fields if the UT/local choice just changed.
    let offset_now = time.display_offset_days();
    if offset_now != offset {
        fields = DateTimeFields::from_jd(time.jd_ut + offset_now);
    }

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
        time.jd_ut = new_fields.to_jd() - offset_now;
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

    ui.horizontal(|ui| {
        ui.label("Time zone:");
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
    });
    ui.checkbox(&mut time.start_at_now, "Start at the current time");

    let ut = DateTimeFields::from_jd(time.jd_ut);
    let local = DateTimeFields::from_jd(time.jd_local());
    ui.weak(format!("UT     {ut}"));
    ui.weak(format!(
        "Local  {local} ({})",
        datetime::format_utc_offset(time.tz_offset_hours)
    ));
    ui.weak(format!("JD     {:.5}", time.jd_ut));

    time.jd_ut != before
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
