//! Civil (Gregorian / Julian) dates and Julian Day Numbers.
//!
//! A *civil day* is identified by its integer **Julian Day Number** (JDN):
//! the Julian Date at noon of that day. For example 1 January 2000
//! (Gregorian) is JDN 2 451 545.
//!
//! The conversions use integer arithmetic with floor division and are exact
//! for every representable date (no floating point, proleptic in both
//! directions). They are equivalent to the algorithms of Meeus,
//! *Astronomical Algorithms* (2nd ed.), ch. 7, restricted to whole days, and
//! follow the day-count formulation of E. G. Richards, "Calendars", in
//! *Explanatory Supplement to the Astronomical Almanac* (3rd ed., 2013),
//! §15.11. Year numbering is astronomical (1 BC = year 0).
//!
//! This module is private to the Hijri work package's needs; the core crate
//! may later provide equivalent functions.

use core::fmt;

/// JDN of the first Gregorian day, 15 October 1582 (the day after Julian
/// 4 October 1582).
pub const GREGORIAN_REFORM_JDN: i64 = 2_299_161;

/// Which civil calendar to use when reading or writing a [`CivilDate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CivilCalendar {
    /// Proleptic Gregorian calendar for all dates.
    #[default]
    Gregorian,
    /// Proleptic Julian calendar for all dates.
    Julian,
    /// Julian calendar before 15 October 1582, Gregorian from then on (the
    /// usual historical convention, as in Meeus ch. 7).
    JulianGregorian,
}

/// A civil calendar date (astronomical year numbering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CivilDate {
    /// Astronomical year (1 BC = 0, 2 BC = -1).
    pub year: i32,
    /// Month, 1 to 12.
    pub month: u8,
    /// Day of the month, 1 to 31.
    pub day: u8,
}

impl CivilDate {
    /// Creates a date without validating it against any calendar.
    pub const fn new(year: i32, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }

    /// The JDN of this date in `calendar`, or `None` if the date does not
    /// exist there (for example 30 February, or 10 October 1582 under
    /// [`CivilCalendar::JulianGregorian`]).
    pub fn to_jdn(self, calendar: CivilCalendar) -> Option<i64> {
        match calendar {
            CivilCalendar::Gregorian => gregorian_to_jdn(self),
            CivilCalendar::Julian => julian_to_jdn(self),
            CivilCalendar::JulianGregorian => {
                let g = gregorian_to_jdn(self)?;
                if g >= GREGORIAN_REFORM_JDN {
                    Some(g)
                } else {
                    julian_to_jdn(self).filter(|&j| j < GREGORIAN_REFORM_JDN)
                }
            }
        }
    }

    /// The date of civil day `jdn` in `calendar`.
    pub fn from_jdn(jdn: i64, calendar: CivilCalendar) -> Self {
        match calendar {
            CivilCalendar::Gregorian => jdn_to_gregorian(jdn),
            CivilCalendar::Julian => jdn_to_julian(jdn),
            CivilCalendar::JulianGregorian => {
                if jdn >= GREGORIAN_REFORM_JDN {
                    jdn_to_gregorian(jdn)
                } else {
                    jdn_to_julian(jdn)
                }
            }
        }
    }
}

impl fmt::Display for CivilDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.year < 0 {
            write!(
                f,
                "-{:04}-{:02}-{:02}",
                -i64::from(self.year),
                self.month,
                self.day
            )
        } else {
            write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
        }
    }
}

/// Whether `year` is a leap year in the Gregorian calendar.
pub const fn is_gregorian_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Whether `year` is a leap year in the Julian calendar.
pub const fn is_julian_leap(year: i32) -> bool {
    year.rem_euclid(4) == 0
}

fn days_in_month(month: u8, leap: bool) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        _ => 28,
    }
}

/// Days from 1 March of the shifted year to the first of the shifted month
/// (March = 0 ... February = 11). Richards (2013) §15.11.
fn march_based(date: CivilDate) -> (i64, i64) {
    let m = i64::from(date.month);
    let y = i64::from(date.year) - i64::from(m <= 2);
    let mp = (m + 9) % 12;
    (y, (153 * mp + 2) / 5)
}

/// Gregorian date to JDN, or `None` if the date is invalid.
pub fn gregorian_to_jdn(date: CivilDate) -> Option<i64> {
    if !(1..=12).contains(&date.month)
        || date.day == 0
        || date.day > days_in_month(date.month, is_gregorian_leap(date.year))
    {
        return None;
    }
    let (y, month_days) = march_based(date);
    Some(
        i64::from(date.day) + month_days + 365 * y + y.div_euclid(4) - y.div_euclid(100)
            + y.div_euclid(400)
            + 1_721_119,
    )
}

/// Julian-calendar date to JDN, or `None` if the date is invalid.
pub fn julian_to_jdn(date: CivilDate) -> Option<i64> {
    if !(1..=12).contains(&date.month)
        || date.day == 0
        || date.day > days_in_month(date.month, is_julian_leap(date.year))
    {
        return None;
    }
    let (y, month_days) = march_based(date);
    Some(i64::from(date.day) + month_days + 365 * y + y.div_euclid(4) + 1_721_117)
}

/// Converts a March-based year and day-of-year (0 = 1 March) to a date.
fn from_march_based(year: i64, doy: i64) -> CivilDate {
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year + i64::from(month <= 2);
    CivilDate {
        // Years outside i32 cannot arise from JDNs of practical interest;
        // saturate rather than wrap.
        year: year.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        month: month as u8,
        day: day as u8,
    }
}

/// JDN to proleptic Gregorian date.
pub fn jdn_to_gregorian(jdn: i64) -> CivilDate {
    // Day 0 = 1 March of year 0.
    let z = jdn - 1_721_120;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    from_march_based(yoe + 400 * era, doy)
}

/// JDN to proleptic Julian-calendar date.
pub fn jdn_to_julian(jdn: i64) -> CivilDate {
    // Day 0 = 1 March of year 0 (Julian).
    let z = jdn - 1_721_118;
    let era = z.div_euclid(1461);
    let doe = z.rem_euclid(1461);
    let yoe = (doe - doe / 1460) / 365;
    let doy = doe - 365 * yoe;
    from_march_based(yoe + 4 * era, doy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(y: i32, m: u8, d: u8) -> i64 {
        gregorian_to_jdn(CivilDate::new(y, m, d)).unwrap()
    }
    fn j(y: i32, m: u8, d: u8) -> i64 {
        julian_to_jdn(CivilDate::new(y, m, d)).unwrap()
    }

    /// Reference values from Meeus, *Astronomical Algorithms*, ch. 7
    /// (examples 7.a/7.b and the table on p. 62), as JD at noon.
    #[test]
    fn meeus_reference_values() {
        assert_eq!(g(2000, 1, 1), 2_451_545);
        assert_eq!(g(1999, 1, 1), 2_451_180);
        assert_eq!(g(1987, 1, 27), 2_446_823);
        assert_eq!(g(1987, 6, 19), 2_446_966);
        assert_eq!(g(1988, 1, 27), 2_447_188);
        assert_eq!(g(1988, 6, 19), 2_447_332);
        assert_eq!(g(1900, 1, 1), 2_415_021);
        assert_eq!(g(1600, 1, 1), 2_305_448);
        assert_eq!(g(1600, 12, 31), 2_305_813);
        assert_eq!(g(1957, 10, 4), 2_436_116);
        assert_eq!(j(837, 4, 10), 2_026_872);
        assert_eq!(j(-123, 12, 31), 1_676_497);
        assert_eq!(j(-122, 1, 1), 1_676_498);
        assert_eq!(j(-1000, 7, 12), 1_356_001);
        assert_eq!(j(-1000, 2, 29), 1_355_867);
        assert_eq!(j(-1001, 8, 17), 1_355_671);
        assert_eq!(j(-4712, 1, 1), 0);
        assert_eq!(j(333, 1, 27), 1_842_713);
    }

    #[test]
    fn reform_and_hijra() {
        assert_eq!(j(1582, 10, 4), GREGORIAN_REFORM_JDN - 1);
        assert_eq!(g(1582, 10, 15), GREGORIAN_REFORM_JDN);
        // 16 July 622 (Julian), the civil epoch of the Hijri calendar.
        assert_eq!(j(622, 7, 16), 1_948_440);
        assert_eq!(g(622, 7, 19), 1_948_440);
        let mixed = CivilCalendar::JulianGregorian;
        assert_eq!(CivilDate::new(1582, 10, 10).to_jdn(mixed), None);
        assert_eq!(
            CivilDate::from_jdn(GREGORIAN_REFORM_JDN - 1, mixed),
            CivilDate::new(1582, 10, 4)
        );
        assert_eq!(
            CivilDate::from_jdn(1_948_440, mixed),
            CivilDate::new(622, 7, 16)
        );
    }

    #[test]
    fn invalid_dates() {
        assert_eq!(gregorian_to_jdn(CivilDate::new(1900, 2, 29)), None);
        assert!(gregorian_to_jdn(CivilDate::new(2000, 2, 29)).is_some());
        assert!(julian_to_jdn(CivilDate::new(1900, 2, 29)).is_some());
        assert_eq!(gregorian_to_jdn(CivilDate::new(2024, 13, 1)), None);
        assert_eq!(gregorian_to_jdn(CivilDate::new(2024, 4, 31)), None);
        assert_eq!(gregorian_to_jdn(CivilDate::new(2024, 4, 0)), None);
    }

    #[test]
    fn round_trips() {
        for jdn in (-1_000_000..4_000_000).step_by(997) {
            let gd = jdn_to_gregorian(jdn);
            assert_eq!(gregorian_to_jdn(gd), Some(jdn), "{gd}");
            let jd = jdn_to_julian(jdn);
            assert_eq!(julian_to_jdn(jd), Some(jdn), "{jd}");
        }
        // Consecutive days are consecutive dates.
        let mut prev = jdn_to_gregorian(2_400_000);
        for jdn in 2_400_001..2_500_000 {
            let d = jdn_to_gregorian(jdn);
            assert!(d > prev);
            prev = d;
        }
    }

    #[test]
    fn display() {
        assert_eq!(CivilDate::new(2024, 7, 7).to_string(), "2024-07-07");
        assert_eq!(CivilDate::new(-44, 3, 15).to_string(), "-0044-03-15");
    }
}
