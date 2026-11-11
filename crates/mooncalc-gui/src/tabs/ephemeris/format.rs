//! Text formatting shared by the Ephemeris and Moon tabs: instants in the
//! app time zone, angles in sexagesimal or decimal notation, durations.

use mooncalc_core::Instant;
use mooncalc_core::angle::DEG;

use crate::datetime::format_utc_offset;

/// Angle notation of table cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AngleStyle {
    /// `12h 34m 56.7s`, `+12° 34′ 56″`.
    Sexagesimal,
    /// `12.58242h`, `+12.5822°`.
    Decimal,
}

impl AngleStyle {
    pub fn from_sexagesimal(sexagesimal: bool) -> Self {
        if sexagesimal {
            Self::Sexagesimal
        } else {
            Self::Decimal
        }
    }
}

/// Calendar date and time of day of a (local) Julian Day, with the
/// seconds rounded to `decimals`.
fn split_seconds(jd_local: f64, decimals: u32) -> (i32, u32, u32, u32, u32, f64) {
    let scale = 10f64.powi(decimals as i32);
    let ticks_per_day = (86_400.0 * scale) as i64;
    let total = ((jd_local + 0.5) * 86_400.0 * scale).round() as i64;
    let z = total.div_euclid(ticks_per_day);
    let ticks = total.rem_euclid(ticks_per_day);
    let c = Instant::from_jd_ut(z as f64 - 0.5 + 0.25).to_calendar();
    let secs = ticks as f64 / scale;
    let h = (secs / 3600.0).floor();
    let m = ((secs - h * 3600.0) / 60.0).floor();
    let s = secs - h * 3600.0 - m * 60.0;
    (c.year, c.month, c.day, h as u32, m as u32, s)
}

/// `YYYY-MM-DD HH:MM` in a zone `tz_hours` ahead of UT.
pub fn date_time(t: Instant, tz_hours: f64) -> String {
    let (y, mo, d, h, mi, _) = split_seconds_minute(t, tz_hours);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}")
}

/// `YYYY-MM-DD HH:MM:SS` in a zone `tz_hours` ahead of UT.
pub fn date_time_seconds(t: Instant, tz_hours: f64) -> String {
    let (y, mo, d, h, mi, s) = split_seconds(t.jd_ut + tz_hours / 24.0, 0);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{:02}", s as u32)
}

/// `HH:MM` in a zone `tz_hours` ahead of UT.
pub fn time_hm(t: Instant, tz_hours: f64) -> String {
    let (_, _, _, h, mi, _) = split_seconds_minute(t, tz_hours);
    format!("{h:02}:{mi:02}")
}

/// `YYYY-MM-DD` of `t` in a zone `tz_hours` ahead of UT (minute-rounded,
/// consistent with [`time_hm`]).
pub fn date(t: Instant, tz_hours: f64) -> String {
    let (y, mo, d, _, _, _) = split_seconds_minute(t, tz_hours);
    format!("{y:04}-{mo:02}-{d:02}")
}

fn split_seconds_minute(t: Instant, tz_hours: f64) -> (i32, u32, u32, u32, u32, f64) {
    // Round to the minute first so that 23:59:40 shows as the next day 00:00.
    let jd = t.jd_ut + tz_hours / 24.0;
    let minutes = ((jd + 0.5) * 1440.0).round();
    split_seconds(minutes / 1440.0 - 0.5, 0)
}

/// The zone label for headers, e.g. `UTC+03:00`, or `UT` for offset 0.
pub fn zone_label(tz_hours: f64) -> String {
    if tz_hours == 0.0 {
        "UT".into()
    } else {
        format_utc_offset(tz_hours)
    }
}

/// Parse `YYYY-MM-DD HH:MM` (or `YYYY-MM-DD`, optionally with `:SS`) as a
/// local time `tz_hours` ahead of UT.
pub fn parse_date_time(text: &str, tz_hours: f64) -> Option<Instant> {
    let text = text.trim();
    let (date, time) = match text.split_once([' ', 'T']) {
        Some((d, t)) => (d, t.trim()),
        None => (text, ""),
    };
    let neg = date.starts_with('-');
    let mut parts = date.trim_start_matches('-').split('-');
    let mut year: i32 = parts.next()?.parse().ok()?;
    if neg {
        year = -year;
    }
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) {
        return None;
    }
    if day < 1 || day > crate::datetime::days_in_month(year, month) {
        return None;
    }
    let (mut hour, mut minute, mut second) = (0u32, 0u32, 0.0f64);
    if !time.is_empty() {
        let mut t = time.split(':');
        hour = t.next()?.parse().ok()?;
        minute = t.next().map_or(Some(0), |m| m.parse().ok())?;
        second = t.next().map_or(Some(0.0), |s| s.parse().ok())?;
        if t.next().is_some() || hour > 23 || minute > 59 || !(0.0..60.0).contains(&second) {
            return None;
        }
    }
    let local = Instant::from_calendar(year, month, day, hour, minute, second);
    Some(local.add_days(-tz_hours / 24.0))
}

/// Split a non-negative value into whole units, minutes and seconds
/// rounded to `decimals`, carrying 60s into the next unit.
fn sexagesimal(value: f64, decimals: usize) -> (u64, u64, f64) {
    let scale = 10f64.powi(decimals as i32);
    let total = (value * 3600.0 * scale).round();
    let whole_scale = 3600.0 * scale;
    let units = (total / whole_scale).floor();
    let rest = total - units * whole_scale;
    let minutes = (rest / (60.0 * scale)).floor();
    let seconds = (rest - minutes * 60.0 * scale) / scale;
    (units as u64, minutes as u64, seconds)
}

fn seconds_text(s: f64, decimals: usize) -> String {
    if decimals == 0 {
        format!("{:02}", s.round() as u64)
    } else {
        format!("{:0w$.d$}", s, w = decimals + 3, d = decimals)
    }
}

/// Right ascension: `hh mm ss.s` or decimal hours.
pub fn ra(rad: f64, style: AngleStyle) -> String {
    let hours = (rad / DEG / 15.0).rem_euclid(24.0);
    match style {
        AngleStyle::Sexagesimal => {
            let (h, m, s) = sexagesimal(hours, 1);
            format!("{:02}h {m:02}m {}s", h % 24, seconds_text(s, 1))
        }
        AngleStyle::Decimal => format!("{hours:.5}h"),
    }
}

/// A signed angle (declination, latitude, altitude): `+dd° mm′ ss″` or
/// `+dd.dddd°`.
pub fn signed_angle(rad: f64, style: AngleStyle) -> String {
    let deg = rad / DEG;
    let sign = if deg < 0.0 { '−' } else { '+' };
    match style {
        AngleStyle::Sexagesimal => {
            let (d, m, s) = sexagesimal(deg.abs(), 0);
            format!("{sign}{d:02}° {m:02}′ {}″", seconds_text(s, 0))
        }
        AngleStyle::Decimal => format!("{sign}{:.4}°", deg.abs()),
    }
}

/// An angle in `[0°, 360°)` (longitude, azimuth): `ddd° mm′ ss″` or
/// `ddd.dddd°`.
pub fn angle_360(rad: f64, style: AngleStyle) -> String {
    let deg = (rad / DEG).rem_euclid(360.0);
    match style {
        AngleStyle::Sexagesimal => {
            let (d, m, s) = sexagesimal(deg, 0);
            format!("{:03}° {m:02}′ {}″", d % 360, seconds_text(s, 0))
        }
        AngleStyle::Decimal => format!("{deg:.4}°"),
    }
}

/// A small angle (semi-diameter, parallax): `mm′ ss.s″` (with degrees when
/// ≥ 1°) or decimal arc minutes.
pub fn small_angle(rad: f64, style: AngleStyle) -> String {
    let deg = rad / DEG;
    match style {
        AngleStyle::Sexagesimal => {
            let (d, m, s) = sexagesimal(deg.abs(), 1);
            let sign = if deg < 0.0 { "−" } else { "" };
            if d > 0 {
                format!("{sign}{d}° {m:02}′ {}″", seconds_text(s, 1))
            } else {
                format!("{sign}{m:02}′ {}″", seconds_text(s, 1))
            }
        }
        AngleStyle::Decimal => format!("{:.3}′", deg * 60.0),
    }
}

/// A duration in minutes as `1h 23m` or `45m 12s`.
pub fn duration_min(minutes: f64) -> String {
    let secs = (minutes * 60.0).round() as i64;
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if h > 0 {
        format!("{h}h {m:02}m {s:02}s")
    } else {
        format!("{m}m {s:02}s")
    }
}

/// Signed minutes of time as `+12m 34s`.
pub fn signed_minutes(minutes: f64) -> String {
    let sign = if minutes < 0.0 { '−' } else { '+' };
    let secs = (minutes.abs() * 60.0).round() as i64;
    format!("{sign}{}m {:02}s", secs / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_in_zones() {
        let t = Instant::from_calendar(2024, 4, 8, 18, 17, 40.0);
        assert_eq!(date_time(t, 0.0), "2024-04-08 18:18");
        assert_eq!(date_time_seconds(t, 0.0), "2024-04-08 18:17:40");
        assert_eq!(date_time(t, 5.75), "2024-04-09 00:03");
        assert_eq!(time_hm(t, -6.0), "12:18");
        // Minute rounding carries over midnight into the next date.
        let late = Instant::from_calendar(2024, 12, 31, 23, 59, 45.0);
        assert_eq!(date_time(late, 0.0), "2025-01-01 00:00");
        assert_eq!(date(late, 0.0), "2025-01-01");
        assert_eq!(zone_label(0.0), "UT");
        assert_eq!(zone_label(3.0), "UTC+03:00");
    }

    #[test]
    fn parse_round_trips() {
        let t = parse_date_time("2024-03-11 19:05", 3.0).unwrap();
        assert_eq!(date_time(t, 3.0), "2024-03-11 19:05");
        assert_eq!(date_time(t, 0.0), "2024-03-11 16:05");
        assert!(parse_date_time("2024-02-30 10:00", 0.0).is_none());
        assert!(parse_date_time("2024-13-01", 0.0).is_none());
        assert!(parse_date_time("2024-01-01 24:00", 0.0).is_none());
        let d = parse_date_time("2024-01-01", 0.0).unwrap();
        assert_eq!(date_time_seconds(d, 0.0), "2024-01-01 00:00:00");
        let s = parse_date_time("2024-01-01T01:02:03", 0.0).unwrap();
        assert_eq!(date_time_seconds(s, 0.0), "2024-01-01 01:02:03");
    }

    #[test]
    fn angles() {
        let s = AngleStyle::Sexagesimal;
        let d = AngleStyle::Decimal;
        // Meeus 47.a: α = 134.688470° = 8h 58m 45.2s, δ = +13.768368°.
        assert_eq!(ra(134.688_470 * DEG, s), "08h 58m 45.2s");
        assert_eq!(ra(134.688_470 * DEG, d), "8.97923h");
        assert_eq!(signed_angle(13.768_368 * DEG, s), "+13° 46′ 06″");
        assert_eq!(signed_angle(-0.5 * DEG, s), "−00° 30′ 00″");
        assert_eq!(signed_angle(-0.5 * DEG, d), "−0.5000°");
        // Carry: 59.9999″ rounds up into the next minute and degree.
        assert_eq!(signed_angle((10.0 - 1e-7) * DEG, s), "+10° 00′ 00″");
        assert_eq!(angle_360(-DEG, s), "359° 00′ 00″");
        assert_eq!(angle_360((360.0 - 1e-7) * DEG, s), "000° 00′ 00″");
        assert_eq!(ra((360.0 - 1e-7) * DEG, s), "00h 00m 00.0s");
        assert_eq!(small_angle(0.991_990 * DEG, s), "59′ 31.2″");
        assert_eq!(small_angle(1.25 * DEG, s), "1° 15′ 00.0″");
        assert_eq!(small_angle(15.5 / 60.0 * DEG, d), "15.500′");
    }

    #[test]
    fn durations() {
        assert_eq!(duration_min(65.5), "1h 05m 30s");
        assert_eq!(duration_min(4.25), "4m 15s");
        assert_eq!(signed_minutes(-14.2), "−14m 12s");
        assert_eq!(signed_minutes(16.4), "+16m 24s");
    }
}
