//! Time scales: Julian Day ↔ calendar, ΔT, and sidereal time.
//!
//! References: J. Meeus, *Astronomical Algorithms*, 2nd ed. (1998),
//! ch. 7 (Julian Day), ch. 10 (ΔT), ch. 12 (sidereal time).

use crate::angle::{DEG, norm_2pi};
use crate::deltat;
use crate::nutation::NutationObliquity;

/// Julian Day of the epoch J2000.0 (2000 January 1.5 TT).
pub const J2000: f64 = 2_451_545.0;
/// Days in a Julian century.
pub const DAYS_PER_CENTURY: f64 = 36_525.0;
/// Seconds per day.
pub const SECONDS_PER_DAY: f64 = 86_400.0;

/// First Julian Day of the Gregorian calendar (1582 October 15, 0h).
const GREGORIAN_START_JD: f64 = 2_299_160.5;

/// A moment in time, stored as a Julian Day in Universal Time (UT1; UTC is
/// treated as equal to UT1, the difference is < 0.9 s).
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Instant {
    pub jd_ut: f64,
}

/// A broken-down calendar date and time (UT unless stated otherwise).
/// Gregorian calendar from 1582-10-15, Julian calendar before. Astronomical
/// year numbering (1 BC = year 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalendarDateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: f64,
}

/// Day of the week.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Weekday {
    Sunday = 0,
    Monday = 1,
    Tuesday = 2,
    Wednesday = 3,
    Thursday = 4,
    Friday = 5,
    Saturday = 6,
}

impl Weekday {
    /// From a number with Sunday = 0 (taken modulo 7).
    pub fn from_index(i: u32) -> Self {
        match i % 7 {
            0 => Self::Sunday,
            1 => Self::Monday,
            2 => Self::Tuesday,
            3 => Self::Wednesday,
            4 => Self::Thursday,
            5 => Self::Friday,
            _ => Self::Saturday,
        }
    }
}

/// Julian Day for a calendar date whose day may carry a fraction
/// (Meeus eq. 7.1). Dates from 1582 October 15 are Gregorian, earlier dates
/// Julian. Astronomical year numbering; uses `floor`, so it is valid for
/// negative years and negative Julian Days as well.
pub fn julian_day(year: i32, month: u32, day: f64) -> f64 {
    let (mut y, mut m) = (year as f64, month as f64);
    if month <= 2 {
        y -= 1.0;
        m += 12.0;
    }
    let gregorian = year > 1582 || (year == 1582 && (month > 10 || (month == 10 && day >= 15.0)));
    let b = if gregorian {
        let a = (y / 100.0).floor();
        2.0 - a + (a / 4.0).floor()
    } else {
        0.0
    };
    (365.25 * (y + 4716.0)).floor() + (30.6001 * (m + 1.0)).floor() + day + b - 1524.5
}

/// Calendar date `(year, month, day-with-fraction)` for a Julian Day
/// (Meeus ch. 7, "Calculation of the calendar date from the JD"). Uses
/// `floor`, which extends the method to negative Julian Days.
pub fn calendar_from_jd(jd: f64) -> (i32, u32, f64) {
    let jd5 = jd + 0.5;
    let z = jd5.floor();
    let f = jd5 - z;
    let (day, month, year) = calendar_from_day_number(z);
    (year, month, day as f64 + f)
}

/// Integer part of Meeus' algorithm: `z = floor(JD + 0.5)` to (day, month, year).
fn calendar_from_day_number(z: f64) -> (u32, u32, i32) {
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
    let day = b - d - (30.6001 * e).floor();
    let month = if e < 14.0 { e - 1.0 } else { e - 13.0 };
    let year = if month > 2.0 { c - 4716.0 } else { c - 4715.0 };
    (day as u32, month as u32, year as i32)
}

/// Whether `year` is a leap year in the calendar in force for that year
/// (Julian before 1583, Gregorian from 1583). Meeus ch. 7.
pub fn is_leap_year(year: i32) -> bool {
    if year < 1583 {
        year.rem_euclid(4) == 0
    } else {
        (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
    }
}

/// Julian centuries of TT from J2000.0.
#[inline]
pub fn centuries_since_j2000(jd: f64) -> f64 {
    (jd - J2000) / DAYS_PER_CENTURY
}

/// Greenwich mean sidereal time for a UT Julian Day, radians in `[0, 2π)`
/// (IAU 1982 expression, Meeus eq. 12.4).
pub fn gmst(jd_ut: f64) -> f64 {
    let d = jd_ut - J2000;
    let t = d / DAYS_PER_CENTURY;
    let deg = 280.460_618_37 + 360.985_647_366_29 * d + t * t * (0.000_387_933 - t / 38_710_000.0);
    norm_2pi(deg.rem_euclid(360.0) * DEG)
}

impl Instant {
    pub const fn from_jd_ut(jd_ut: f64) -> Self {
        Self { jd_ut }
    }

    pub fn add_days(self, days: f64) -> Self {
        Self {
            jd_ut: self.jd_ut + days,
        }
    }

    /// From a UT calendar date and time (Meeus eq. 7.1). Gregorian from
    /// 1582-10-15, Julian before; astronomical year numbering. Out-of-range
    /// hours, minutes and seconds simply carry (e.g. hour 24 is the next day).
    pub fn from_calendar(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: f64,
    ) -> Self {
        let jd0 = julian_day(year, month, day as f64);
        let frac = (hour as f64 * 3600.0 + minute as f64 * 60.0 + second) / SECONDS_PER_DAY;
        Self { jd_ut: jd0 + frac }
    }

    /// From a [`CalendarDateTime`] (UT).
    pub fn from_calendar_date_time(c: &CalendarDateTime) -> Self {
        Self::from_calendar(c.year, c.month, c.day, c.hour, c.minute, c.second)
    }

    /// From a TT Julian Day, inverting ΔT by fixed-point iteration.
    pub fn from_jd_tt(jd_tt: f64) -> Self {
        let mut jd_ut = jd_tt - deltat::delta_t_seconds(jd_tt) / SECONDS_PER_DAY;
        for _ in 0..3 {
            jd_ut = jd_tt - deltat::delta_t_seconds(jd_ut) / SECONDS_PER_DAY;
        }
        Self { jd_ut }
    }

    /// The current system time. Not available on `wasm32`, where the host
    /// (browser) supplies the time instead.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn now() -> Self {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        Self::from_unix_seconds(secs)
    }

    /// From seconds since 1970-01-01T00:00:00 UTC (leap seconds ignored).
    pub fn from_unix_seconds(secs: f64) -> Self {
        Self {
            jd_ut: 2_440_587.5 + secs / SECONDS_PER_DAY,
        }
    }

    /// Calendar date and time (UT), rounded to the nearest microsecond so
    /// that values built with [`Instant::from_calendar`] round-trip cleanly.
    pub fn to_calendar(self) -> CalendarDateTime {
        let jd5 = self.jd_ut + 0.5;
        let mut z = jd5.floor();
        let mut micros = ((jd5 - z) * SECONDS_PER_DAY * 1e6).round() as i64;
        if micros >= 86_400_000_000 {
            z += 1.0;
            micros -= 86_400_000_000;
        }
        let (day, month, year) = calendar_from_day_number(z);
        let hour = (micros / 3_600_000_000) as u32;
        let minute = ((micros / 60_000_000) % 60) as u32;
        let second = (micros % 60_000_000) as f64 / 1e6;
        CalendarDateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        }
    }

    /// Day of the week of the UT date (Meeus ch. 7).
    pub fn weekday(self) -> Weekday {
        Weekday::from_index((self.jd_ut + 1.5).floor().rem_euclid(7.0) as u32)
    }

    /// Whether this instant falls in the Gregorian calendar.
    pub fn is_gregorian(self) -> bool {
        self.jd_ut >= GREGORIAN_START_JD
    }

    /// ΔT = TT − UT in seconds (see [`crate::deltat`]).
    pub fn delta_t_seconds(self) -> f64 {
        deltat::delta_t_seconds(self.jd_ut)
    }

    /// Terrestrial Time as a Julian Day (JDE in Meeus' notation).
    pub fn jd_tt(self) -> f64 {
        self.jd_ut + self.delta_t_seconds() / SECONDS_PER_DAY
    }

    /// Greenwich mean sidereal time, radians (Meeus eq. 12.4).
    pub fn gmst(self) -> f64 {
        gmst(self.jd_ut)
    }

    /// Greenwich apparent sidereal time, radians: GMST plus the equation of
    /// the equinoxes Δψ cos ε (Meeus ch. 12), nutation evaluated at TT.
    pub fn gast(self) -> f64 {
        let n = NutationObliquity::at(self.jd_tt());
        self.gast_with(&n)
    }

    /// GAST using an already-computed nutation (avoids re-evaluating the
    /// 63-term series when the caller has it).
    pub fn gast_with(self, n: &NutationObliquity) -> f64 {
        norm_2pi(self.gmst() + n.equation_of_equinoxes())
    }

    /// Local mean sidereal time for an east-positive longitude in radians.
    pub fn local_mean_sidereal_time(self, lon_east: f64) -> f64 {
        norm_2pi(self.gmst() + lon_east)
    }

    /// Local apparent sidereal time for an east-positive longitude in radians.
    pub fn local_apparent_sidereal_time(self, lon_east: f64) -> f64 {
        norm_2pi(self.gast() + lon_east)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::angle::ARCSEC;

    #[test]
    fn meeus_7a_7b_julian_day() {
        // Example 7.a: launch of Sputnik 1, 1957 October 4.81.
        assert!((julian_day(1957, 10, 4.81) - 2_436_116.31).abs() < 1e-9);
        // Example 7.b: 333 January 27, 12h.
        assert!((julian_day(333, 1, 27.5) - 1_842_713.0).abs() < 1e-9);
        // Table on p. 62.
        let cases = [
            (2000, 1, 1.5, 2_451_545.0),
            (1999, 1, 1.0, 2_451_179.5),
            (1987, 1, 27.0, 2_446_822.5),
            (1987, 6, 19.5, 2_446_966.0),
            (1988, 1, 27.0, 2_447_187.5),
            (1988, 6, 19.5, 2_447_332.0),
            (1900, 1, 1.0, 2_415_020.5),
            (1600, 1, 1.0, 2_305_447.5),
            (1600, 12, 31.0, 2_305_812.5),
            (837, 4, 10.3, 2_026_871.8),
            (-123, 12, 31.0, 1_676_496.5),
            (-122, 1, 1.0, 1_676_497.5),
            (-1000, 7, 12.5, 1_356_001.0),
            (-1000, 2, 29.0, 1_355_866.5),
            (-1001, 8, 17.9, 1_355_671.4),
            (-4712, 1, 1.5, 0.0),
        ];
        for (y, m, d, jd) in cases {
            assert!((julian_day(y, m, d) - jd).abs() < 1e-6, "{y}-{m}-{d}");
            let (y2, m2, d2) = calendar_from_jd(jd);
            assert_eq!((y2, m2), (y, m), "{jd}");
            assert!((d2 - d).abs() < 1e-6, "{jd}");
        }
    }

    #[test]
    fn meeus_7c_calendar_from_jd() {
        let (y, m, d) = calendar_from_jd(2_436_116.31);
        assert_eq!((y, m), (1957, 10));
        assert!((d - 4.81).abs() < 1e-6);
        // Gregorian reform: Oct 4 (Julian) is followed by Oct 15 (Gregorian).
        assert_eq!(calendar_from_jd(2_299_159.5).2, 4.0);
        assert_eq!(calendar_from_jd(2_299_160.5).2, 15.0);
    }

    #[test]
    fn calendar_round_trip() {
        let i = Instant::from_calendar(2024, 3, 10, 18, 45, 30.25);
        let c = i.to_calendar();
        assert_eq!(
            (c.year, c.month, c.day, c.hour, c.minute),
            (2024, 3, 10, 18, 45)
        );
        assert!((c.second - 30.25).abs() < 1e-5);
        let midnight = Instant::from_calendar(2023, 12, 31, 24, 0, 0.0).to_calendar();
        assert_eq!(
            (midnight.year, midnight.month, midnight.day, midnight.hour),
            (2024, 1, 1, 0)
        );
        let c = Instant::from_calendar(-500, 2, 28, 23, 59, 59.5).to_calendar();
        assert_eq!(
            (c.year, c.month, c.day, c.hour, c.minute),
            (-500, 2, 28, 23, 59)
        );
        assert!((c.second - 59.5).abs() < 1e-4);
    }

    #[test]
    fn meeus_7e_weekday() {
        // 1954 June 30 was a Wednesday.
        assert_eq!(
            Instant::from_calendar(1954, 6, 30, 0, 0, 0.0).weekday(),
            Weekday::Wednesday
        );
        assert_eq!(
            Instant::from_calendar(2000, 1, 1, 23, 0, 0.0).weekday(),
            Weekday::Saturday
        );
    }

    #[test]
    fn leap_years() {
        assert!(is_leap_year(900) && is_leap_year(1236) && is_leap_year(-4));
        assert!(!is_leap_year(1900) && is_leap_year(2000) && !is_leap_year(2100));
    }

    #[test]
    fn meeus_12a_sidereal_time() {
        // 1987 April 10, 0h UT: θ0 = 13h10m46.3668s, apparent 13h10m46.1351s.
        let i = Instant::from_jd_ut(2_446_895.5);
        let mean = (13.0 + 10.0 / 60.0 + 46.3668 / 3600.0) * 15.0 * DEG;
        assert!((i.gmst() - mean).abs() < 0.0001 * 15.0 * ARCSEC);
        // Meeus evaluates nutation at the UT JD here; ΔT changes it negligibly.
        let n = NutationObliquity::at(i.jd_ut);
        let app = (13.0 + 10.0 / 60.0 + 46.1351 / 3600.0) * 15.0 * DEG;
        assert!((i.gast_with(&n) - app).abs() < 0.0005 * 15.0 * ARCSEC);
        assert!((i.gast() - app).abs() < 0.001 * 15.0 * ARCSEC);
    }

    #[test]
    fn meeus_12b_sidereal_time() {
        // 1987 April 10, 19h21m00s UT: θ0 = 128.7378734°.
        let i = Instant::from_calendar(1987, 4, 10, 19, 21, 0.0);
        assert!((i.jd_ut - 2_446_896.306_25).abs() < 1e-9);
        assert!((i.gmst() / DEG - 128.737_873_4).abs() < 1e-6);
    }

    #[test]
    fn tt_round_trip() {
        let i = Instant::from_calendar(2025, 6, 1, 12, 0, 0.0);
        let back = Instant::from_jd_tt(i.jd_tt());
        assert!((back.jd_ut - i.jd_ut).abs() * SECONDS_PER_DAY < 1e-6);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn now_is_after_2025() {
        assert!(Instant::now().jd_ut > 2_460_676.5);
        assert!((Instant::from_unix_seconds(0.0).jd_ut - 2_440_587.5).abs() < 1e-12);
    }
}
