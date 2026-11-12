//! Text formatting shared by the Ephemeris and Moon tabs: instants in the
//! app time zone, angles in sexagesimal or decimal notation, durations.

use mooncalc_core::Instant;
use mooncalc_core::angle::DEG;

use mooncalc_atlas::Zone;

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

/// `YYYY-MM-DD HH:MM` in zone `tz` (its offset at `t`).
pub fn date_time(t: Instant, tz: Zone) -> String {
    let (y, mo, d, h, mi, _) = split_seconds_minute(t, tz);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}")
}

/// `YYYY-MM-DD HH:MM:SS` in zone `tz`.
pub fn date_time_seconds(t: Instant, tz: Zone) -> String {
    let (y, mo, d, h, mi, s) = split_seconds(tz.to_local(t.jd_ut), 0);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{:02}", s as u32)
}

/// `HH:MM` in zone `tz`.
pub fn time_hm(t: Instant, tz: Zone) -> String {
    let (_, _, _, h, mi, _) = split_seconds_minute(t, tz);
    format!("{h:02}:{mi:02}")
}

/// `YYYY-MM-DD` of `t` in zone `tz` (minute-rounded, consistent with
/// [`time_hm`]).
pub fn date(t: Instant, tz: Zone) -> String {
    let (y, mo, d, _, _, _) = split_seconds_minute(t, tz);
    format!("{y:04}-{mo:02}-{d:02}")
}

fn split_seconds_minute(t: Instant, tz: Zone) -> (i32, u32, u32, u32, u32, f64) {
    // Round to the minute first so that 23:59:40 shows as the next day 00:00.
    let jd = tz.to_local(t.jd_ut);
    let minutes = ((jd + 0.5) * 1440.0).round();
    split_seconds(minutes / 1440.0 - 0.5, 0)
}

/// The zone label for headers: `UT` for offset 0, `UTC+03:00` for a fixed
/// offset, the IANA name (`Europe/London`) for a zone, whose offset varies.
pub fn zone_label(tz: Zone) -> String {
    match tz.iana_name() {
        Some(name) => name.to_owned(),
        None if tz == Zone::UTC => "UT".into(),
        None => format_utc_offset(tz.fixed_offset_hours().unwrap_or_default()),
    }
}

/// Parse `YYYY-MM-DD HH:MM` (or `YYYY-MM-DD`, optionally with `:SS`) as a
/// local time in `tz`. A time skipped by a DST change moves forward by the
/// gap; a repeated one is taken at its first occurrence.
pub fn parse_date_time(text: &str, tz: Zone) -> Option<Instant> {
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
    Some(Instant::from_jd_ut(tz.to_ut(local.jd_ut)))
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
        assert_eq!(date_time(t, Zone::fixed_hours(0.0)), "2024-04-08 18:18");
        assert_eq!(
            date_time_seconds(t, Zone::fixed_hours(0.0)),
            "2024-04-08 18:17:40"
        );
        assert_eq!(date_time(t, Zone::fixed_hours(5.75)), "2024-04-09 00:03");
        assert_eq!(time_hm(t, Zone::fixed_hours(-6.0)), "12:18");
        // Minute rounding carries over midnight into the next date.
        let late = Instant::from_calendar(2024, 12, 31, 23, 59, 45.0);
        assert_eq!(date_time(late, Zone::fixed_hours(0.0)), "2025-01-01 00:00");
        assert_eq!(date(late, Zone::fixed_hours(0.0)), "2025-01-01");
        assert_eq!(zone_label(Zone::fixed_hours(0.0)), "UT");
        assert_eq!(zone_label(Zone::fixed_hours(3.0)), "UTC+03:00");
    }

    #[test]
    fn parse_round_trips() {
        let t = parse_date_time("2024-03-11 19:05", Zone::fixed_hours(3.0)).unwrap();
        assert_eq!(date_time(t, Zone::fixed_hours(3.0)), "2024-03-11 19:05");
        assert_eq!(date_time(t, Zone::fixed_hours(0.0)), "2024-03-11 16:05");
        assert!(parse_date_time("2024-02-30 10:00", Zone::fixed_hours(0.0)).is_none());
        assert!(parse_date_time("2024-13-01", Zone::fixed_hours(0.0)).is_none());
        assert!(parse_date_time("2024-01-01 24:00", Zone::fixed_hours(0.0)).is_none());
        let d = parse_date_time("2024-01-01", Zone::fixed_hours(0.0)).unwrap();
        assert_eq!(
            date_time_seconds(d, Zone::fixed_hours(0.0)),
            "2024-01-01 00:00:00"
        );
        let s = parse_date_time("2024-01-01T01:02:03", Zone::fixed_hours(0.0)).unwrap();
        assert_eq!(
            date_time_seconds(s, Zone::fixed_hours(0.0)),
            "2024-01-01 01:02:03"
        );
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

    #[test]
    fn zone_times_follow_dst() {
        let ny = Zone::iana("America/New_York").unwrap();
        assert_eq!(zone_label(ny), "America/New_York");
        assert_eq!(zone_label(Zone::UTC), "UT");
        assert_eq!(zone_label(Zone::fixed_hours(5.5)), "UTC+05:30");
        // 2024-03-10 06:30 and 07:30 UT: 01:30 EST, then 03:30 EDT.
        let t = Instant::from_calendar(2024, 3, 10, 6, 30, 0.0);
        assert_eq!(date_time(t, ny), "2024-03-10 01:30");
        assert_eq!(date_time(t.add_days(1.0 / 24.0), ny), "2024-03-10 03:30");
        // 02:30 was skipped: it parses as 03:30 EDT.
        let gap = parse_date_time("2024-03-10 02:30", ny).unwrap();
        assert_eq!(date_time_seconds(gap, Zone::UTC), "2024-03-10 07:30:00");
        // 01:30 on 3 November happened twice: the first is EDT.
        let fold = parse_date_time("2024-11-03 01:30", ny).unwrap();
        assert_eq!(date_time_seconds(fold, Zone::UTC), "2024-11-03 05:30:00");
    }
}
