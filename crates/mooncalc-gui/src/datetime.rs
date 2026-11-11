//! Calendar date/time arithmetic for the date-time input widget.
//!
//! The canonical time in [`crate::settings::TimeSettings`] is a Julian Day in
//! UT (`jd_ut`). These helpers convert it to and from broken-down calendar
//! fields (rounded to the minute, which is the input resolution), and step it
//! by calendar units, days and lunations.
//!
//! Calendar conversions follow Jean Meeus, *Astronomical Algorithms*, 2nd ed.
//! (Willmann-Bell, 1998), chapter 7: the Gregorian calendar from 1582-10-15,
//! the Julian calendar before, with astronomical year numbering (1 BC = 0).
//! Valid from JD 0 (−4712-01-01 12h) onwards.
//!
//! When `mooncalc_core::time` gains its own calendar conversions, the
//! [`calendar_to_jd`] / [`jd_to_calendar`] pair can delegate to it; the field
//! stepping logic stays here because it is input-widget behaviour.

use mooncalc_core::CalendarDateTime;

/// Mean synodic month (new Moon to new Moon), in days.
///
/// Meeus, *Astronomical Algorithms*, ch. 49 (the coefficient of `k` in
/// eq. 49.1). Stepping by a lunation moves by this mean interval; true
/// lunations vary by about ±7 hours.
pub const SYNODIC_MONTH_DAYS: f64 = 29.530_588_861;

/// Julian Day of the Unix epoch, 1970-01-01 00:00 UT.
pub const UNIX_EPOCH_JD: f64 = 2_440_587.5;

/// Julian Day of J2000.0 (2000-01-01 12:00 TT, used here as a UT fallback).
pub const J2000_JD: f64 = 2_451_545.0;

/// Earliest year the input accepts (JD 0 lies in −4712).
pub const YEAR_MIN: i32 = -4712;
/// Latest year the input accepts.
pub const YEAR_MAX: i32 = 9999;

/// Whether `(year, month, day)` falls in the Gregorian calendar
/// (on or after 1582-10-15).
pub fn is_gregorian(year: i32, month: u32, day: u32) -> bool {
    (year, month, day) >= (1582, 10, 15)
}

/// Leap-year rule of the calendar in force in `year` (Julian before 1583).
pub fn is_leap_year(year: i32) -> bool {
    if year > 1582 {
        (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
    } else {
        year.rem_euclid(4) == 0
    }
}

/// Number of days in `month` (1–12) of `year`.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 30,
    }
}

/// Julian Day of a calendar date; `day` may carry a fraction of a day.
///
/// Meeus, *Astronomical Algorithms*, eq. 7.1.
pub fn calendar_to_jd(year: i32, month: u32, day: f64) -> f64 {
    let gregorian = is_gregorian(year, month, day.floor() as u32);
    let (mut y, mut m) = (f64::from(year), f64::from(month));
    if month <= 2 {
        y -= 1.0;
        m += 12.0;
    }
    let b = if gregorian {
        let a = (y / 100.0).floor();
        2.0 - a + (a / 4.0).floor()
    } else {
        0.0
    };
    (365.25 * (y + 4716.0)).floor() + (30.6001 * (m + 1.0)).floor() + day + b - 1524.5
}

/// Calendar date of a Julian Day number `z` (the JD of noon of that day).
/// Meeus, *Astronomical Algorithms*, ch. 7 ("Calendar Date from the JD").
fn day_number_to_date(z: i64) -> (i32, u32, u32) {
    let z = z as f64;
    let a = if z < 2_299_161.0 {
        z
    } else {
        let alpha = ((z - 1_867_216.25) / 36_524.25).floor();
        z + 1.0 + alpha - (alpha / 4.0).floor()
    };
    let b = a + 1524.0;
    let c = ((b - 122.1) / 365.25).floor();
    let d = (365.25 * c).floor();
    let e = ((b - d) / 30.6001).floor();
    let day = (b - d - (30.6001 * e).floor()) as u32;
    let month = if e < 14.0 { e - 1.0 } else { e - 13.0 } as u32;
    let year = if month > 2 { c - 4716.0 } else { c - 4715.0 } as i32;
    (year, month, day)
}

/// Calendar date and time of a Julian Day (full precision, seconds included).
pub fn jd_to_calendar(jd: f64) -> CalendarDateTime {
    let z = (jd + 0.5).floor();
    let mut seconds_of_day = (jd + 0.5 - z) * 86_400.0;
    // Guard against 86399.99999 rounding up to a 24th hour.
    if seconds_of_day >= 86_400.0 {
        seconds_of_day = 86_399.999_999;
    }
    let (year, month, day) = day_number_to_date(z as i64);
    let hour = (seconds_of_day / 3600.0).floor();
    let minute = ((seconds_of_day - hour * 3600.0) / 60.0).floor();
    let second = seconds_of_day - hour * 3600.0 - minute * 60.0;
    CalendarDateTime {
        year,
        month,
        day,
        hour: hour as u32,
        minute: minute as u32,
        second,
    }
}

/// The current time as a Julian Day (UT), from the system clock.
pub fn now_jd() -> f64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => UNIX_EPOCH_JD + d.as_secs_f64() / 86_400.0,
        Err(_) => J2000_JD,
    }
}

/// Round a Julian Day to the nearest whole minute.
pub fn round_to_minute(jd: f64) -> f64 {
    ((jd + 0.5) * 1440.0).round() / 1440.0 - 0.5
}

/// Move `jd` by a number of mean lunations ([`SYNODIC_MONTH_DAYS`]).
pub fn step_lunations(jd: f64, lunations: i32) -> f64 {
    jd + f64::from(lunations) * SYNODIC_MONTH_DAYS
}

/// One editable field of [`DateTimeFields`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Year,
    Month,
    Day,
    Hour,
    Minute,
}

impl Field {
    pub const ALL: [Field; 5] = [
        Field::Year,
        Field::Month,
        Field::Day,
        Field::Hour,
        Field::Minute,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Field::Year => "Year",
            Field::Month => "Month",
            Field::Day => "Day",
            Field::Hour => "Hour",
            Field::Minute => "Minute",
        }
    }
}

/// Broken-down date and time at minute resolution, as shown in the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTimeFields {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

impl DateTimeFields {
    /// Fields of a Julian Day, rounded to the nearest minute.
    pub fn from_jd(jd: f64) -> Self {
        let total_minutes = ((jd + 0.5) * 1440.0).round() as i64;
        let z = total_minutes.div_euclid(1440);
        let minute_of_day = total_minutes.rem_euclid(1440) as u32;
        let (year, month, day) = day_number_to_date(z);
        Self {
            year,
            month,
            day,
            hour: minute_of_day / 60,
            minute: minute_of_day % 60,
        }
    }

    /// Julian Day of these fields.
    pub fn to_jd(self) -> f64 {
        calendar_to_jd(self.year, self.month, f64::from(self.day))
            + f64::from(self.hour * 60 + self.minute) / 1440.0
    }

    /// Get one field as a signed number (for generic widgets).
    pub fn get(self, field: Field) -> i64 {
        match field {
            Field::Year => i64::from(self.year),
            Field::Month => i64::from(self.month),
            Field::Day => i64::from(self.day),
            Field::Hour => i64::from(self.hour),
            Field::Minute => i64::from(self.minute),
        }
    }

    /// Set one field directly (as typed or dragged), then clamp everything
    /// into range. No carrying: typing day 31 in a 30-day month gives day 30.
    pub fn with(mut self, field: Field, value: i64) -> Self {
        let v = value.clamp(i64::from(i32::MIN), i64::from(i32::MAX));
        match field {
            Field::Year => self.year = v as i32,
            Field::Month => self.month = v.clamp(1, 12) as u32,
            Field::Day => self.day = v.clamp(1, 31) as u32,
            Field::Hour => self.hour = v.clamp(0, 23) as u32,
            Field::Minute => self.minute = v.clamp(0, 59) as u32,
        }
        self.clamped()
    }

    /// Valid range of a field, given the other fields.
    pub fn range(self, field: Field) -> std::ops::RangeInclusive<i64> {
        match field {
            Field::Year => i64::from(YEAR_MIN)..=i64::from(YEAR_MAX),
            Field::Month => 1..=12,
            Field::Day => 1..=i64::from(days_in_month(self.year, self.month)),
            Field::Hour => 0..=23,
            Field::Minute => 0..=59,
        }
    }

    /// Clamp every field into its valid range.
    pub fn clamped(mut self) -> Self {
        self.year = self.year.clamp(YEAR_MIN, YEAR_MAX);
        self.month = self.month.clamp(1, 12);
        self.day = self.day.clamp(1, days_in_month(self.year, self.month));
        self.hour = self.hour.min(23);
        self.minute = self.minute.min(59);
        self
    }

    /// Step one field by `delta` units, carrying into the other fields.
    ///
    /// Years and months step by calendar arithmetic (the day is clamped, so
    /// Jan 31 + 1 month = Feb 28/29); days, hours and minutes step by
    /// elapsed time, so they roll over month and year boundaries.
    pub fn step(self, field: Field, delta: i32) -> Self {
        match field {
            Field::Year => Self {
                year: self.year.saturating_add(delta),
                ..self
            }
            .clamped(),
            Field::Month => {
                let m0 = self.month as i32 - 1 + delta;
                Self {
                    year: self.year.saturating_add(m0.div_euclid(12)),
                    month: (m0.rem_euclid(12) + 1) as u32,
                    ..self
                }
                .clamped()
            }
            Field::Day => Self::from_jd(self.to_jd() + f64::from(delta)).clamped(),
            Field::Hour => Self::from_jd(self.to_jd() + f64::from(delta) / 24.0).clamped(),
            Field::Minute => Self::from_jd(self.to_jd() + f64::from(delta) / 1440.0).clamped(),
        }
    }
}

impl std::fmt::Display for DateTimeFields {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }
}

/// Format a time-zone offset in hours as `UTC±HH:MM`.
pub fn format_utc_offset(hours: f64) -> String {
    let sign = if hours < 0.0 { '-' } else { '+' };
    let total_minutes = (hours.abs() * 60.0).round() as u32;
    format!(
        "UTC{sign}{:02}:{:02}",
        total_minutes / 60,
        total_minutes % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// Meeus, *Astronomical Algorithms*, examples 7.a, 7.b and the table on p. 62.
    #[test]
    fn calendar_to_jd_matches_meeus_examples() {
        assert!(close(calendar_to_jd(1957, 10, 4.81), 2_436_116.31));
        assert!(close(calendar_to_jd(333, 1, 27.5), 1_842_713.0));
        assert!(close(calendar_to_jd(2000, 1, 1.5), 2_451_545.0));
        assert!(close(calendar_to_jd(1987, 6, 19.5), 2_446_966.0));
        assert!(close(calendar_to_jd(1600, 12, 31.0), 2_305_812.5));
        assert!(close(calendar_to_jd(837, 4, 10.3), 2_026_871.8));
        assert!(close(calendar_to_jd(-1000, 7, 12.5), 1_356_001.0));
        assert!(close(calendar_to_jd(-4712, 1, 1.5), 0.0));
        // Gregorian reform: 1582-10-04 (Julian) is followed by 1582-10-15.
        assert!(close(
            calendar_to_jd(1582, 10, 15.0) - calendar_to_jd(1582, 10, 4.0),
            1.0
        ));
    }

    /// Meeus example 7.c: JD 2436116.31 = 1957 October 4.81.
    #[test]
    fn jd_to_calendar_matches_meeus_example() {
        let c = jd_to_calendar(2_436_116.31);
        assert_eq!((c.year, c.month, c.day), (1957, 10, 4));
        let frac = (f64::from(c.hour) * 3600.0 + f64::from(c.minute) * 60.0 + c.second) / 86_400.0;
        assert!((frac - 0.81).abs() < 1e-6);
        let c = jd_to_calendar(1_842_713.0);
        assert_eq!((c.year, c.month, c.day, c.hour), (333, 1, 27, 12));
        let c = jd_to_calendar(1_507_900.13);
        assert_eq!((c.year, c.month, c.day), (-584, 5, 28));
    }

    #[test]
    fn fields_round_trip_every_minute_of_a_day() {
        let base = calendar_to_jd(2024, 2, 29.0);
        for minute in 0..1440 {
            let jd = base + f64::from(minute) / 1440.0;
            let f = DateTimeFields::from_jd(jd);
            assert_eq!(f.hour * 60 + f.minute, minute as u32);
            assert!(close(f.to_jd(), jd));
        }
    }

    #[test]
    fn from_jd_rounds_to_nearest_minute() {
        let jd = calendar_to_jd(2026, 12, 31.0) + (23.0 * 60.0 + 59.0 + 0.6) / 1440.0;
        let f = DateTimeFields::from_jd(jd);
        assert_eq!(
            f,
            DateTimeFields {
                year: 2027,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0
            }
        );
    }

    #[test]
    fn day_hour_minute_steps_carry_across_boundaries() {
        let f = DateTimeFields {
            year: 2023,
            month: 12,
            day: 31,
            hour: 23,
            minute: 59,
        };
        let g = f.step(Field::Minute, 1);
        assert_eq!(
            (g.year, g.month, g.day, g.hour, g.minute),
            (2024, 1, 1, 0, 0)
        );
        let g = f.step(Field::Hour, 1);
        assert_eq!((g.year, g.month, g.day, g.hour), (2024, 1, 1, 0));
        let g = f.step(Field::Day, 60);
        assert_eq!((g.year, g.month, g.day), (2024, 2, 29));
        let g = f.step(Field::Day, -365);
        assert_eq!((g.year, g.month, g.day), (2022, 12, 31));
    }

    #[test]
    fn month_and_year_steps_clamp_the_day() {
        let f = DateTimeFields {
            year: 2024,
            month: 1,
            day: 31,
            hour: 18,
            minute: 30,
        };
        assert_eq!(f.step(Field::Month, 1).day, 29);
        assert_eq!(f.step(Field::Month, 13).day, 28); // Feb 2025
        let g = f.step(Field::Month, -1);
        assert_eq!((g.year, g.month, g.day), (2023, 12, 31));
        let leap = DateTimeFields {
            month: 2,
            day: 29,
            ..f
        };
        let g = leap.step(Field::Year, 1);
        assert_eq!((g.year, g.month, g.day), (2025, 2, 28));
        assert_eq!((g.hour, g.minute), (18, 30));
    }

    #[test]
    fn direct_edits_are_clamped() {
        let f = DateTimeFields {
            year: 2025,
            month: 4,
            day: 10,
            hour: 0,
            minute: 0,
        };
        assert_eq!(f.with(Field::Day, 31).day, 30);
        assert_eq!(f.with(Field::Month, 13).month, 12);
        assert_eq!(f.with(Field::Minute, -5).minute, 0);
        assert_eq!(f.with(Field::Year, 20_000).year, YEAR_MAX);
        assert_eq!(f.range(Field::Day), 1..=30);
    }

    #[test]
    fn leap_years_follow_julian_then_gregorian_rule() {
        assert!(is_leap_year(1500)); // Julian
        assert!(!is_leap_year(1700)); // Gregorian
        assert!(is_leap_year(2000));
        assert!(is_leap_year(-4)); // 5 BC, Julian
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);
    }

    #[test]
    fn lunation_and_day_stepping() {
        let jd = 2_460_000.25;
        assert!(close(step_lunations(jd, 1) - jd, SYNODIC_MONTH_DAYS));
        assert!(close(
            step_lunations(jd, -2) - jd,
            -2.0 * SYNODIC_MONTH_DAYS
        ));
        // 12 lunations ≈ 354.37 days (one lunar year).
        assert!((step_lunations(jd, 12) - jd - 354.367).abs() < 1e-3);
    }

    #[test]
    fn now_is_after_2025_and_rounding_is_idempotent() {
        let now = now_jd();
        assert!(now > calendar_to_jd(2025, 1, 1.0));
        let r = round_to_minute(now);
        assert!((r - now).abs() <= 0.5 / 1440.0 + 1e-9);
        assert!(close(round_to_minute(r), r));
    }

    #[test]
    fn utc_offset_formatting() {
        assert_eq!(format_utc_offset(3.0), "UTC+03:00");
        assert_eq!(format_utc_offset(5.5), "UTC+05:30");
        assert_eq!(format_utc_offset(-9.75), "UTC-09:45");
        assert_eq!(format_utc_offset(0.0), "UTC+00:00");
    }
}
