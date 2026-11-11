//! The world map's date line versus an observer's civil date.
//!
//! Maps follow the HMNAO convention: each place is evaluated on its own
//! local evening (or morning) of the map's date, with the day anchored at
//! 12h local mean time, so the date line is the **180° meridian**
//! (`mooncalc_maps::engine`). The location report follows the observer's
//! **civil** date instead. The two agree wherever the time-zone offset is
//! within 12 hours of mean solar time; east of 180° in UTC+13/+14 (Samoa,
//! Tonga, the Line Islands, the Chatham Islands) the civil date runs a day
//! ahead, so the map for a date shows the *next* civil evening there.
//!
//! These helpers produce the shared wording for both frontends:
//!
//! * [`map_convention`] gives the convention line in a map's info panel.
//! * [`report_note`] fills `LocationReport::map_date_note`
//!   when the report's civil evening is on another map date.
//! * [`observer_note`] goes next to a map's observer readout, when the evening
//!   shown under the observer is not their civil evening of the map date.
//!
//! The date arithmetic is `mooncalc_core::events::mean_time_day_shift`.

use mooncalc_core::events::{CalendarDate, mean_time_day_shift};

use crate::dto::{Crescent, MapDateNote};

/// `YYYY-MM-DD` (astronomical year numbering, as `CivilDate`).
pub fn date_text(d: CalendarDate) -> String {
    if d.year < 0 {
        format!("-{:04}-{:02}-{:02}", -d.year, d.month, d.day)
    } else {
        format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
    }
}

/// `UTC+13`, `UTC+12:45`, `UTC−5`, `UTC`.
pub fn utc_offset_text(hours: f64) -> String {
    let minutes = (hours * 60.0).round() as i64;
    if minutes == 0 {
        return "UTC".into();
    }
    let sign = if minutes > 0 { '+' } else { '−' };
    let (h, m) = (minutes.abs() / 60, minutes.abs() % 60);
    if m == 0 {
        format!("UTC{sign}{h}")
    } else {
        format!("UTC{sign}{h}:{m:02}")
    }
}

fn period(crescent: Crescent) -> &'static str {
    match crescent {
        Crescent::Evening => "evening",
        Crescent::Morning => "morning",
    }
}

/// The map's date convention, for its info panel: "Each location is shown
/// on its own local evening of 2024-03-10; the date line is the 180°
/// meridian (HMNAO convention)."
pub fn map_convention(map_date: CalendarDate, crescent: Crescent) -> String {
    format!(
        "Each location is shown on its own local {} of {}; the date line is the 180° \
         meridian (HMNAO convention).",
        period(crescent),
        date_text(map_date)
    )
}

/// For a report of civil `date` at east longitude `lon_deg` in a zone
/// `tz_offset_hours` ahead of UT: the map date whose evening and morning at
/// that longitude are the report's, when it differs from `date`.
pub fn report_note(date: CalendarDate, lon_deg: f64, tz_offset_hours: f64) -> Option<MapDateNote> {
    let shift = mean_time_day_shift(lon_deg, tz_offset_hours);
    if shift == 0 {
        return None;
    }
    let map_date = date.add_days(shift);
    let shown = date.add_days(-shift);
    let relation = if shift < 0 { "ahead of" } else { "behind" };
    let text = format!(
        "The world map for {} shows this evening and morning at your longitude; the map's \
         date line is the 180° meridian (HMNAO convention), and your civil date ({}) runs a \
         day {relation} it. The map for {} shows the evening of {} here.",
        date_text(map_date),
        utc_offset_text(tz_offset_hours),
        date_text(date),
        date_text(shown),
    );
    Some(MapDateNote {
        map_date: map_date.into(),
        day_shift: shift,
        text,
    })
}

/// For a map of `map_date` and an observer at east longitude `lon_deg` in a
/// zone `tz_offset_hours` ahead of UT: a note when the evening (morning) the
/// map shows under the observer is not their civil one of `map_date`.
pub fn observer_note(
    map_date: CalendarDate,
    lon_deg: f64,
    tz_offset_hours: f64,
    crescent: Crescent,
) -> Option<String> {
    let shift = mean_time_day_shift(lon_deg, tz_offset_hours);
    if shift == 0 {
        return None;
    }
    let p = period(crescent);
    Some(format!(
        "At the observer this map shows the {p} of local date {} ({}), not of {}: the map's \
         date line is the 180° meridian. Their {p} of {} is on the map for {}.",
        date_text(map_date.add_days(-shift)),
        utc_offset_text(tz_offset_hours),
        date_text(map_date),
        date_text(map_date),
        date_text(map_date.add_days(shift)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{Bounds, CivilDate, Location, LocationReportParams, MapParams};
    use crate::ephemeris::meeus;
    use crate::map::compute_map;
    use crate::report::location_report;
    use mooncalc_core::events::CalendarDate;

    const DATE: CalendarDate = CalendarDate::new(2024, 3, 10);

    /// The status and Yallop zone the report gives for the evening.
    fn report_evening(lat: f64, lon: f64, tz: f64) -> (String, Option<u8>, Option<MapDateNote>) {
        let eph = meeus(false);
        let r = location_report(
            &*eph,
            &LocationReportParams {
                date: DATE.into(),
                tz_offset_hours: tz,
                location: Location {
                    lat_deg: lat,
                    lon_deg: lon,
                    elevation_m: 0.0,
                },
                ephemeris_id: None,
            },
        )
        .unwrap();
        let y = &r.evening.criteria[0];
        assert_eq!(y.criterion_id, "yallop");
        (
            y.status.clone(),
            y.verdict.as_ref().map(|v| v.zone),
            r.map_date_note,
        )
    }

    /// The Yallop zone and whether the cell is a crescent, for a one-cell
    /// evening map of `date` centred on the place.
    fn map_cell(date: CalendarDate, lat: f64, lon: f64) -> (bool, Option<u8>) {
        let eph = meeus(false);
        let m = compute_map(
            &*eph,
            &MapParams {
                date: CivilDate::from(date),
                criterion_id: "yallop".into(),
                crescent: Crescent::Evening,
                resolution_deg: 0.2,
                bounds: Some(Bounds {
                    lat_min_deg: lat - 0.1,
                    lat_max_deg: lat + 0.1,
                    lon_min_deg: lon - 0.1,
                    lon_max_deg: lon + 0.1,
                }),
                ephemeris_id: None,
                include_values: false,
            },
        )
        .unwrap();
        assert_eq!((m.grid.rows, m.grid.cols), (1, 1));
        let zone = m.zone_counts.iter().position(|&n| n == 1).map(|z| z as u8);
        (m.status_counts.crescent == 1, zone)
    }

    /// Apia and Nuku'alofa (UTC+13) on 2024-03-10: the report's civil
    /// evening is "Moon sets first", the map for 2024-03-10 shows a crescent
    /// under the observer (the next civil evening). The map for the note's
    /// date shows the report's evening.
    #[test]
    fn pacific_places_east_of_180_get_the_previous_map_date() {
        for (name, lat, lon, tz) in [
            ("Apia", -13.8333, -171.7667, 13.0),
            ("Nuku'alofa", -21.1368, -175.2011, 13.0),
            ("Kiritimati", 1.87, -157.4, 14.0),
        ] {
            let (status, zone, note) = report_evening(lat, lon, tz);
            assert_eq!(status, "moon-sets-first", "{name}");
            assert_eq!(zone, None, "{name}");
            // The mismatch: the map of the civil date shows a crescent here.
            let (crescent, map_zone) = map_cell(DATE, lat, lon);
            assert!(crescent && map_zone.is_some(), "{name}: {map_zone:?}");
            // The note names the map that matches the report.
            let note = note.unwrap_or_else(|| panic!("{name}: no note"));
            assert_eq!(note.day_shift, -1);
            assert_eq!(
                note.map_date,
                CivilDate::from(CalendarDate::new(2024, 3, 9))
            );
            assert_eq!(
                map_cell(CalendarDate::from(note.map_date), lat, lon),
                (false, None),
                "{name}"
            );
            assert!(note.text.contains("2024-03-09"), "{}", note.text);
            assert!(note.text.contains("180° meridian"), "{}", note.text);
            // And the map of the civil date says where its evening is.
            let obs = observer_note(DATE, lon, tz, Crescent::Evening).unwrap();
            assert!(obs.contains("local date 2024-03-11"), "{obs}");
            assert!(obs.contains("on the map for 2024-03-09"), "{obs}");
        }
    }

    /// Where the zone offset is within 12 h of mean time the report and the
    /// map cell agree and there is no note: London, Ürümqi on Beijing time
    /// (UTC+8 at 87.6° E), Honolulu, and Tarawa (UTC+12 at 173° E).
    #[test]
    fn places_on_their_mean_time_date_get_no_note() {
        for (name, lat, lon, tz) in [
            ("London", 51.5074, -0.1278, 0.0),
            ("Ürümqi", 43.8, 87.6, 8.0),
            ("Honolulu", 21.31, -157.86, -10.0),
            ("Tarawa", 1.3278, 172.977, 12.0),
        ] {
            let (status, zone, note) = report_evening(lat, lon, tz);
            assert!(note.is_none(), "{name}: {note:?}");
            assert!(observer_note(DATE, lon, tz, Crescent::Evening).is_none());
            let (crescent, map_zone) = map_cell(DATE, lat, lon);
            assert_eq!(crescent, status == "crescent", "{name}");
            assert_eq!(map_zone, zone, "{name}");
        }
    }

    #[test]
    fn texts() {
        assert_eq!(utc_offset_text(13.0), "UTC+13");
        assert_eq!(utc_offset_text(12.75), "UTC+12:45");
        assert_eq!(utc_offset_text(-5.5), "UTC−5:30");
        assert_eq!(utc_offset_text(0.0), "UTC");
        assert_eq!(date_text(CalendarDate::new(-44, 3, 15)), "-0044-03-15");
        assert_eq!(
            map_convention(DATE, Crescent::Morning),
            "Each location is shown on its own local morning of 2024-03-10; the date line is \
             the 180° meridian (HMNAO convention)."
        );
        // The exact wording, also pinned by frontend/tauri/tests/dateline.test.ts.
        assert_eq!(
            report_note(DATE, -171.7667, 13.0).unwrap().text,
            "The world map for 2024-03-09 shows this evening and morning at your longitude; the \
             map's date line is the 180° meridian (HMNAO convention), and your civil date \
             (UTC+13) runs a day ahead of it. The map for 2024-03-10 shows the evening of \
             2024-03-11 here."
        );
        assert_eq!(
            observer_note(DATE, -171.7667, 13.0, Crescent::Evening).unwrap(),
            "At the observer this map shows the evening of local date 2024-03-11 (UTC+13), not \
             of 2024-03-10: the map's date line is the 180° meridian. Their evening of \
             2024-03-10 is on the map for 2024-03-09."
        );
        // A civil date behind mean time points to the next map date.
        let n = report_note(DATE, 173.0, -10.0).unwrap();
        assert_eq!(n.day_shift, 1);
        assert!(n.text.contains("behind"), "{}", n.text);
        assert!(
            n.text.starts_with("The world map for 2024-03-11"),
            "{}",
            n.text
        );
    }
}
