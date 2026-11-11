//! Text formatting shared by the map, report and export: instants in the
//! user's time zone and UT, durations, angles and positions.

use crate::datetime::format_utc_offset;

/// Broken-down date and time of a Julian Day, rounded to `unit_s` seconds.
fn split(jd: f64, unit_s: f64) -> (i32, u32, u32, u32, u32, u32) {
    let units = ((jd + 0.5) * 86_400.0 / unit_s).round();
    let secs = (units * unit_s) as i64;
    let day = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400) as u32;
    // Noon of that civil day, safely inside it.
    let c = mooncalc_core::Instant::from_jd_ut(day as f64).to_calendar();
    (c.year, c.month, c.day, sod / 3600, sod / 60 % 60, sod % 60)
}

/// `2024-03-10 18:42` (minutes) of a Julian Day in whatever scale it is.
pub fn fmt_jd_minutes(jd: f64) -> String {
    let (y, mo, d, h, mi, _) = split(jd, 60.0);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}")
}

/// `HH:MM:SS` of a Julian Day.
pub fn fmt_jd_time_secs(jd: f64) -> String {
    let (_, _, _, h, mi, s) = split(jd, 1.0);
    format!("{h:02}:{mi:02}:{s:02}")
}

/// `2024-03-10 09:00 UT` plus, for a non-zero offset,
/// ` (12:00 UTC+03:00)` (with the local date when it differs).
pub fn fmt_instant(jd_ut: f64, tz_hours: f64) -> String {
    let ut = fmt_jd_minutes(jd_ut);
    if tz_hours == 0.0 {
        return format!("{ut} UT");
    }
    let local = fmt_jd_minutes(jd_ut + tz_hours / 24.0);
    let shown = if local[..10] == ut[..10] {
        &local[11..]
    } else {
        &local[..]
    };
    format!("{ut} UT ({shown} {})", format_utc_offset(tz_hours))
}

/// Local time first, then UT, to the second: `18:42:10 UTC+03:00 (15:42:10
/// UT)`; dates are added when the local date differs from `local_date`.
pub fn fmt_local_ut(jd_ut: f64, tz_hours: f64, local_date: Option<(i32, u32, u32)>) -> String {
    let local_jd = jd_ut + tz_hours / 24.0;
    let (y, mo, d, ..) = split(local_jd, 1.0);
    let date = if local_date == Some((y, mo, d)) {
        String::new()
    } else {
        format!("{y:04}-{mo:02}-{d:02} ")
    };
    let local = fmt_jd_time_secs(local_jd);
    if tz_hours == 0.0 {
        format!("{date}{local} UT")
    } else {
        format!(
            "{date}{local} {} ({} UT)",
            format_utc_offset(tz_hours),
            fmt_jd_time_secs(jd_ut)
        )
    }
}

/// `1h 23m` / `-25m` / `∞` for a duration in minutes.
pub fn fmt_minutes(minutes: f64) -> String {
    if !minutes.is_finite() {
        return "∞".into();
    }
    let sign = if minutes < 0.0 { "-" } else { "" };
    let m = minutes.abs().round() as i64;
    if m >= 60 {
        format!("{sign}{}h {:02}m", m / 60, m % 60)
    } else {
        format!("{sign}{m}m")
    }
}

/// `17.4 h` / `1d 03.2h` for an age in hours.
pub fn fmt_hours(hours: f64) -> String {
    if !hours.is_finite() {
        return "—".into();
    }
    if hours.abs() >= 24.0 {
        let sign = if hours < 0.0 { "-" } else { "" };
        let h = hours.abs();
        format!("{sign}{}d {:04.1}h", (h / 24.0).floor(), h % 24.0)
    } else {
        format!("{hours:.1} h")
    }
}

/// `21.42°N 39.83°E`.
pub fn fmt_lat_lon_short(lat: f64, lon: f64) -> String {
    let lon = crate::location::normalize_lon(lon);
    format!(
        "{:.2}°{} {:.2}°{}",
        lat.abs(),
        if lat >= 0.0 { 'N' } else { 'S' },
        lon.abs(),
        if lon >= 0.0 { 'E' } else { 'W' }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const JD_2024_03_10_0900: f64 = 2_460_379.875;

    #[test]
    fn instants_in_ut_and_local_time() {
        assert_eq!(fmt_instant(JD_2024_03_10_0900, 0.0), "2024-03-10 09:00 UT");
        assert_eq!(
            fmt_instant(JD_2024_03_10_0900, 3.0),
            "2024-03-10 09:00 UT (12:00 UTC+03:00)"
        );
        assert_eq!(
            fmt_instant(JD_2024_03_10_0900, -10.0),
            "2024-03-10 09:00 UT (2024-03-09 23:00 UTC-10:00)"
        );
        assert_eq!(
            fmt_local_ut(JD_2024_03_10_0900, 3.0, Some((2024, 3, 10))),
            "12:00:00 UTC+03:00 (09:00:00 UT)"
        );
        assert_eq!(
            fmt_local_ut(JD_2024_03_10_0900, 0.0, None),
            "2024-03-10 09:00:00 UT"
        );
        // Rounding carries into the next day.
        assert_eq!(fmt_jd_minutes(2_460_380.499_999), "2024-03-11 00:00");
    }

    #[test]
    fn durations() {
        assert_eq!(fmt_minutes(83.4), "1h 23m");
        assert_eq!(fmt_minutes(-25.2), "-25m");
        assert_eq!(fmt_minutes(f64::INFINITY), "∞");
        assert_eq!(fmt_hours(17.44), "17.4 h");
        assert_eq!(fmt_hours(27.0), "1d 03.0h");
        assert_eq!(fmt_lat_lon_short(-5.0, 240.0), "5.00°S 120.00°W");
    }
}
