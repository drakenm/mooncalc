//! Tabular (arithmetical) Islamic calendars.
//!
//! The tabular or *ḥisābī* calendar was introduced by Muslim astronomers in
//! the 8th century to predict the approximate start of the lunar months.
//! Months alternate between 30 and 29 days (odd months 30, even months 29),
//! giving a 354-day common year. In 11 years of every 30-year cycle an
//! intercalary day is added to Dhū al-Ḥijjah (355 days), so a cycle has
//! 10 631 days and the mean month is 29.530 556 days.
//!
//! Four intercalation schemes are found in the historical sources. The leap
//! years and attributions below follow R. H. van Gent, "The Arithmetical or
//! Tabular Islamic Calendar", Utrecht University
//! (<https://webspace.science.uu.nl/~gent0113/islam/islam_tabcal_variants.htm>,
//! retrieved 2026-11-11), consistent with E. M. Reingold and N. Dershowitz,
//! *Calendrical Calculations* (4th ed., 2018), ch. 7:
//!
//! | Scheme | Leap years in the 30-year cycle | Origin / usage |
//! | --- | --- | --- |
//! | I   | 2, 5, 7, 10, 13, **15**, 18, 21, 24, 26, 29 | Kūshyār ibn Labbān, Ulugh Beg |
//! | II  | 2, 5, 7, 10, 13, 16, 18, 21, 24, 26, 29 | al-Fazārī, al-Khwārizmī, al-Battānī; the most common; Microsoft `HijriCalendar` / "Kuwaiti algorithm" |
//! | III | 2, 5, **8**, 10, 13, 16, **19**, 21, 24, **27**, 29 | Fāṭimid (Ismāʿīlī, Bohra) calendar |
//! | IV  | 2, 5, 8, **11**, 13, 16, 19, 21, 24, 27, **30** | Ḥabash al-Ḥāsib, al-Bīrūnī |
//!
//! Each scheme comes with two epochs for 1 Muḥarram 1 AH: the *civil* or
//! Friday epoch, 16 July 622 (Julian), JDN 1 948 440; and the *astronomical*
//! or Thursday epoch, 15 July 622, JDN 1 948 439. Microsoft's `HijriCalendar`
//! is type II astronomical; CLDR/ICU's `islamic-civil` is type II civil and
//! `islamic-tbla` type II astronomical.

use crate::calendar::{SupportedRange, day_offset};
use crate::{HijriCalendar, HijriDate};

/// Days in one 30-year cycle (30 × 354 + 11).
pub const CYCLE_DAYS: i64 = 10_631;

/// JDN of 1 Muḥarram 1 AH with the civil (Friday) epoch, 16 July 622 Julian.
pub const CIVIL_EPOCH_JDN: i64 = 1_948_440;

/// JDN of 1 Muḥarram 1 AH with the astronomical (Thursday) epoch,
/// 15 July 622 Julian.
pub const ASTRONOMICAL_EPOCH_JDN: i64 = 1_948_439;

/// Last year AH supported by the tabular calendars (about 10 322 CE).
pub const TABULAR_MAX_YEAR: i32 = 9999;

/// The four classical intercalation schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Intercalation {
    /// Leap years 2, 5, 7, 10, 13, 15, 18, 21, 24, 26, 29.
    TypeI,
    /// Leap years 2, 5, 7, 10, 13, 16, 18, 21, 24, 26, 29 (the most common).
    TypeII,
    /// Leap years 2, 5, 8, 10, 13, 16, 19, 21, 24, 27, 29 (Fāṭimid).
    TypeIII,
    /// Leap years 2, 5, 8, 11, 13, 16, 19, 21, 24, 27, 30 (Ḥabash al-Ḥāsib).
    TypeIV,
}

impl Intercalation {
    /// All four schemes in order.
    pub const ALL: [Intercalation; 4] = [Self::TypeI, Self::TypeII, Self::TypeIII, Self::TypeIV];

    /// The leap years (355 days) within the 30-year cycle.
    pub const fn leap_years(self) -> [u8; 11] {
        match self {
            Self::TypeI => [2, 5, 7, 10, 13, 15, 18, 21, 24, 26, 29],
            Self::TypeII => [2, 5, 7, 10, 13, 16, 18, 21, 24, 26, 29],
            Self::TypeIII => [2, 5, 8, 10, 13, 16, 19, 21, 24, 27, 29],
            Self::TypeIV => [2, 5, 8, 11, 13, 16, 19, 21, 24, 27, 30],
        }
    }

    /// Bit (k - 1) set when cycle year k is a leap year.
    const fn mask(self) -> u32 {
        let years = self.leap_years();
        let mut mask = 0;
        let mut i = 0;
        while i < years.len() {
            mask |= 1 << (years[i] - 1);
            i += 1;
        }
        mask
    }
}

/// The epoch (1 Muḥarram 1 AH) of a tabular calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TabularEpoch {
    /// Friday 16 July 622 (Julian), JDN 1 948 440.
    Civil,
    /// Thursday 15 July 622 (Julian), JDN 1 948 439.
    Astronomical,
}

impl TabularEpoch {
    /// JDN of 1 Muḥarram 1 AH.
    pub const fn jdn(self) -> i64 {
        match self {
            Self::Civil => CIVIL_EPOCH_JDN,
            Self::Astronomical => ASTRONOMICAL_EPOCH_JDN,
        }
    }
}

/// A tabular Islamic calendar: one intercalation scheme with one epoch.
///
/// Supports years 1 to [`TABULAR_MAX_YEAR`] AH.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabularCalendar {
    intercalation: Intercalation,
    epoch: TabularEpoch,
    mask: u32,
}

const fn names(i: Intercalation, e: TabularEpoch) -> (&'static str, &'static str) {
    use Intercalation::*;
    use TabularEpoch::*;
    match (i, e) {
        (TypeI, Civil) => ("tabular-1-civil", "Tabular I (civil epoch)"),
        (TypeI, Astronomical) => ("tabular-1-astronomical", "Tabular I (astronomical epoch)"),
        (TypeII, Civil) => ("tabular-2-civil", "Tabular II (civil epoch)"),
        (TypeII, Astronomical) => ("tabular-2-astronomical", "Tabular II (astronomical epoch)"),
        (TypeIII, Civil) => ("tabular-3-civil", "Tabular III, Fatimid (civil epoch)"),
        (TypeIII, Astronomical) => (
            "tabular-3-astronomical",
            "Tabular III, Fatimid (astronomical epoch)",
        ),
        (TypeIV, Civil) => ("tabular-4-civil", "Tabular IV (civil epoch)"),
        (TypeIV, Astronomical) => ("tabular-4-astronomical", "Tabular IV (astronomical epoch)"),
    }
}

impl TabularCalendar {
    /// The calendar with the given scheme and epoch.
    pub const fn new(intercalation: Intercalation, epoch: TabularEpoch) -> Self {
        Self {
            intercalation,
            epoch,
            mask: intercalation.mask(),
        }
    }

    /// The intercalation scheme.
    pub const fn intercalation(&self) -> Intercalation {
        self.intercalation
    }

    /// The epoch.
    pub const fn epoch(&self) -> TabularEpoch {
        self.epoch
    }

    /// Whether `year` has 355 days. Defined for all years (proleptically for
    /// years before 1 AH).
    pub fn is_leap_year(&self, year: i32) -> bool {
        let k = (i64::from(year) - 1).rem_euclid(30);
        self.mask & (1 << k) != 0
    }

    /// Days in cycle years 1..=r (r = 0..=30).
    fn days_before_cycle_year(&self, r: u32) -> i64 {
        let leaps = (self.mask & ((1u32 << r) - 1)).count_ones();
        354 * i64::from(r) + i64::from(leaps)
    }

    /// Days from the epoch to 1 Muḥarram of `year` (year >= 1).
    fn days_before_year(&self, year: i32) -> i64 {
        let y = i64::from(year) - 1;
        let cycles = y.div_euclid(30);
        let r = y.rem_euclid(30) as u32;
        cycles * CYCLE_DAYS + self.days_before_cycle_year(r)
    }

    fn in_range(year: i32) -> bool {
        (1..=TABULAR_MAX_YEAR).contains(&year)
    }
}

/// Days from 1 Muḥarram to the first of `month` with alternating 30/29-day
/// months.
fn days_before_month(month: u8) -> i64 {
    let m = i64::from(month);
    29 * (m - 1) + m / 2
}

impl HijriCalendar for TabularCalendar {
    fn id(&self) -> &str {
        names(self.intercalation, self.epoch).0
    }

    fn name(&self) -> &str {
        names(self.intercalation, self.epoch).1
    }

    fn to_hijri(&self, jdn_civil_day: i64) -> Option<HijriDate> {
        let d = jdn_civil_day - self.epoch.jdn();
        if d < 0 {
            return None;
        }
        let cycles = d / CYCLE_DAYS;
        let rem = d % CYCLE_DAYS;
        // Largest r with days_before_cycle_year(r) <= rem; at most 29.
        let mut r = (rem / 355) as u32;
        while self.days_before_cycle_year(r + 1) <= rem {
            r += 1;
        }
        let year = i32::try_from(cycles * 30 + i64::from(r) + 1).ok()?;
        if !Self::in_range(year) {
            return None;
        }
        let doy = rem - self.days_before_cycle_year(r);
        let mut month = ((doy / 30) as u8 + 1).min(12);
        while month < 12 && days_before_month(month + 1) <= doy {
            month += 1;
        }
        while days_before_month(month) > doy {
            month -= 1;
        }
        let day = (doy - days_before_month(month) + 1) as u8;
        Some(HijriDate::new(year, month, day))
    }

    fn from_hijri(&self, date: HijriDate) -> Option<i64> {
        let offset = day_offset(date, self.month_length(date.year, date.month))?;
        Some(
            self.epoch.jdn()
                + self.days_before_year(date.year)
                + days_before_month(date.month)
                + offset,
        )
    }

    fn month_length(&self, year: i32, month: u8) -> Option<u8> {
        if !Self::in_range(year) || !(1..=12).contains(&month) {
            return None;
        }
        Some(match month {
            12 if self.is_leap_year(year) => 30,
            m if m % 2 == 1 => 30,
            _ => 29,
        })
    }

    fn supported_range(&self) -> SupportedRange {
        SupportedRange {
            first_year: 1,
            last_year: TABULAR_MAX_YEAR,
        }
    }
}

/// Type I, civil epoch.
pub const TABULAR_I_CIVIL: TabularCalendar =
    TabularCalendar::new(Intercalation::TypeI, TabularEpoch::Civil);
/// Type I, astronomical epoch.
pub const TABULAR_I_ASTRONOMICAL: TabularCalendar =
    TabularCalendar::new(Intercalation::TypeI, TabularEpoch::Astronomical);
/// Type II, civil epoch (CLDR `islamic-civil`).
pub const TABULAR_II_CIVIL: TabularCalendar =
    TabularCalendar::new(Intercalation::TypeII, TabularEpoch::Civil);
/// Type II, astronomical epoch (CLDR `islamic-tbla`, Microsoft
/// `HijriCalendar`).
pub const TABULAR_II_ASTRONOMICAL: TabularCalendar =
    TabularCalendar::new(Intercalation::TypeII, TabularEpoch::Astronomical);
/// Type III (Fāṭimid), civil epoch.
pub const TABULAR_III_CIVIL: TabularCalendar =
    TabularCalendar::new(Intercalation::TypeIII, TabularEpoch::Civil);
/// Type III (Fāṭimid), astronomical epoch.
pub const TABULAR_III_ASTRONOMICAL: TabularCalendar =
    TabularCalendar::new(Intercalation::TypeIII, TabularEpoch::Astronomical);
/// Type IV, civil epoch.
pub const TABULAR_IV_CIVIL: TabularCalendar =
    TabularCalendar::new(Intercalation::TypeIV, TabularEpoch::Civil);
/// Type IV, astronomical epoch.
pub const TABULAR_IV_ASTRONOMICAL: TabularCalendar =
    TabularCalendar::new(Intercalation::TypeIV, TabularEpoch::Astronomical);

/// All eight tabular calendars.
pub const ALL_TABULAR: [TabularCalendar; 8] = [
    TABULAR_I_CIVIL,
    TABULAR_I_ASTRONOMICAL,
    TABULAR_II_CIVIL,
    TABULAR_II_ASTRONOMICAL,
    TABULAR_III_CIVIL,
    TABULAR_III_ASTRONOMICAL,
    TABULAR_IV_CIVIL,
    TABULAR_IV_ASTRONOMICAL,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::civil::{CivilCalendar, CivilDate};

    fn greg(y: i32, m: u8, d: u8) -> i64 {
        CivilDate::new(y, m, d)
            .to_jdn(CivilCalendar::Gregorian)
            .unwrap()
    }

    #[test]
    fn epochs() {
        assert_eq!(
            TABULAR_II_CIVIL.from_hijri(HijriDate::new(1, 1, 1)),
            Some(1_948_440)
        );
        assert_eq!(
            TABULAR_II_ASTRONOMICAL.from_hijri(HijriDate::new(1, 1, 1)),
            Some(1_948_439)
        );
        for cal in ALL_TABULAR {
            let e = cal.epoch().jdn();
            assert_eq!(cal.to_hijri(e), Some(HijriDate::new(1, 1, 1)));
            assert_eq!(cal.to_hijri(e - 1), None);
        }
    }

    #[test]
    fn leap_sets() {
        for i in Intercalation::ALL {
            let cal = TabularCalendar::new(i, TabularEpoch::Civil);
            let leaps: Vec<u8> = (1..=30u8)
                .filter(|&y| cal.is_leap_year(i32::from(y)))
                .collect();
            assert_eq!(leaps, i.leap_years());
            assert_eq!(cal.days_before_cycle_year(30), CYCLE_DAYS);
        }
    }

    /// Type II civil against the closed formula `fixed-from-islamic` of
    /// Reingold & Dershowitz, *Calendrical Calculations* (4th ed., 2018),
    /// ch. 7: `RD = epoch - 1 + 354(y - 1) + floor((3 + 11y) / 30) +
    /// 29(m - 1) + floor(m / 2) + d`, with epoch RD 227 015 (16 July 622
    /// Julian) and JDN = RD + 1 721 425.
    #[test]
    fn matches_calendrical_calculations_formula() {
        for y in 1..=3000i64 {
            for m in 1..=12i64 {
                let rd = 227_015 - 1
                    + 354 * (y - 1)
                    + (3 + 11 * y).div_euclid(30)
                    + 29 * (m - 1)
                    + m / 2
                    + 1;
                let h = HijriDate::new(y as i32, m as u8, 1);
                assert_eq!(
                    TABULAR_II_CIVIL.from_hijri(h),
                    Some(rd + 1_721_425),
                    "{h:?}"
                );
            }
        }
    }

    #[test]
    fn known_dates() {
        // 2000-01-01 (JDN 2451545) is 24 Ramadan 1420 in type II civil.
        assert_eq!(
            TABULAR_II_CIVIL.to_hijri(greg(2000, 1, 1)),
            Some(HijriDate::new(1420, 9, 24))
        );
        // Astronomical epoch: one day earlier for the same Hijri date.
        assert_eq!(
            TABULAR_II_ASTRONOMICAL.to_hijri(greg(1999, 12, 31)),
            Some(HijriDate::new(1420, 9, 24))
        );
        // 1 Muharram 1446: 8 July 2024 (civil), 7 July (astronomical).
        assert_eq!(
            TABULAR_II_CIVIL.from_hijri(HijriDate::new(1446, 1, 1)),
            Some(greg(2024, 7, 8))
        );
        assert_eq!(
            TABULAR_II_ASTRONOMICAL.from_hijri(HijriDate::new(1446, 1, 1)),
            Some(greg(2024, 7, 7))
        );
    }

    /// The schemes differ only in which cycle years are leap, so dates in
    /// cycle year 15/16 separate types I and II.
    #[test]
    fn schemes_differ_where_expected() {
        // 1455 AH is cycle year 15: leap in type I only.
        assert_eq!(TABULAR_I_CIVIL.year_length(1455), Some(355));
        assert_eq!(TABULAR_II_CIVIL.year_length(1455), Some(354));
        assert_eq!(TABULAR_II_CIVIL.year_length(1456), Some(355));
        assert_eq!(TABULAR_I_CIVIL.year_length(1456), Some(354));
        // Cycles realign: same start for the year after a full cycle.
        assert_eq!(
            TABULAR_I_CIVIL.month_start(1471, 1),
            TABULAR_IV_CIVIL.month_start(1471, 1)
        );
    }

    #[test]
    fn round_trip_and_continuity() {
        for cal in ALL_TABULAR {
            let start = cal.epoch().jdn();
            assert_eq!(cal.from_hijri(cal.to_hijri(start).unwrap()), Some(start));
            // ~1600 years day by day near the present, sparser elsewhere.
            for jdn in (start + 1..start + 3_500_000)
                .step_by(7)
                .chain(greg(1900, 1, 1)..greg(2100, 1, 1))
            {
                let h = cal.to_hijri(jdn).unwrap();
                assert_eq!(cal.from_hijri(h), Some(jdn), "{} {h:?}", cal.id());
                assert!(cal.is_valid(h));
            }
        }
    }

    #[test]
    fn consecutive_days() {
        let cal = TABULAR_III_ASTRONOMICAL;
        let mut prev = cal.to_hijri(cal.epoch().jdn()).unwrap();
        for jdn in cal.epoch().jdn() + 1..cal.epoch().jdn() + 40_000 {
            let h = cal.to_hijri(jdn).unwrap();
            let len = cal.month_length(prev.year, prev.month).unwrap();
            let expected = if prev.day < len {
                HijriDate::new(prev.year, prev.month, prev.day + 1)
            } else if prev.month < 12 {
                HijriDate::new(prev.year, prev.month + 1, 1)
            } else {
                HijriDate::new(prev.year + 1, 1, 1)
            };
            assert_eq!(h, expected);
            prev = h;
        }
    }

    #[test]
    fn year_lengths() {
        for cal in ALL_TABULAR {
            let mut total = 0i64;
            for y in 1..=30 {
                let len = cal.year_length(y).unwrap();
                assert_eq!(len, if cal.is_leap_year(y) { 355 } else { 354 });
                assert_eq!(
                    cal.month_start(y + 1, 1).unwrap() - cal.month_start(y, 1).unwrap(),
                    i64::from(len)
                );
                total += i64::from(len);
            }
            assert_eq!(total, CYCLE_DAYS);
        }
    }

    #[test]
    fn invalid_dates() {
        let cal = TABULAR_II_CIVIL;
        assert_eq!(cal.from_hijri(HijriDate::new(1445, 2, 30)), None);
        assert_eq!(cal.from_hijri(HijriDate::new(1445, 13, 1)), None);
        assert_eq!(cal.from_hijri(HijriDate::new(1445, 1, 0)), None);
        assert_eq!(cal.from_hijri(HijriDate::new(0, 1, 1)), None);
        assert_eq!(cal.from_hijri(HijriDate::new(10_000, 1, 1)), None);
        // 1445 is cycle year 5: a leap year in all schemes.
        assert!(cal.from_hijri(HijriDate::new(1445, 12, 30)).is_some());
        assert_eq!(cal.from_hijri(HijriDate::new(1446, 12, 30)), None);
        let (first, last) = cal.supported_jdn_range().unwrap();
        assert_eq!(first, CIVIL_EPOCH_JDN);
        assert_eq!(cal.to_hijri(last + 1), None);
        assert!(cal.to_hijri(last).is_some());
    }
}
