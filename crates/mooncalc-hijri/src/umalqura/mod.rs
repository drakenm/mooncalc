//! The Umm al-Qura calendar, the official calendar of Saudi Arabia.
//!
//! Umm al-Qura month starts are not given by a simple rule over the whole
//! period: the criterion was redefined several times (1346–1419 AH several
//! empirical rules, since 1420 AH conjunction before sunset and moonset after
//! sunset at Mecca; see R. H. van Gent, "The Umm al-Qura Calendar of Saudi
//! Arabia", <https://webspace.science.uu.nl/~gent0113/islam/ummalqura.htm>),
//! and the published calendars contain occasional 28- and 31-day months
//! before 1365 AH. Both calendars here are therefore table-driven, and return
//! `None` outside their tables.
//!
//! * [`UMM_AL_QURA`] (`"umm-al-qura"`), 1343–1500 AH: the month starts **as
//!   officially published**, taken from the hijridate project (MIT licence,
//!   Copyright (c) 2018-2025 Mohammed Alshehri), which verified them against
//!   archived *Umm al-Qura* newspaper issues (1343–1355 AH), the KFUPM
//!   comparison calendar (1356–1411 AH), the official Umm al-Qura
//!   comparative calendar books (1412–1450 AH) and the KACST website
//!   (1451–1500 AH, future projections that KACST may revise). Independently
//!   cross-checked: identical to Microsoft .NET's `UmAlQuraCalendar` for
//!   1395–1399 and 1402–1500 AH and to ICU for 1420–1450 AH.
//! * [`UMM_AL_QURA_ICU`] (`"umm-al-qura-icu"`), 1300–1600 AH: the table of
//!   ICU4C (Unicode License V3), i.e. CLDR's `islamic-umalqura` as used by
//!   web browsers' `Intl` API and Java. It agrees with the official calendar
//!   only for 1420–1450 AH; outside that it is an algorithmic reconstruction.
//!   Use it for interoperability with `Intl.DateTimeFormat` or for years the
//!   official table does not cover.
//!
//! The licence texts are in `crates/mooncalc-hijri/THIRD-PARTY-NOTICES.md`
//! and are also available as [`ATTRIBUTION_OFFICIAL`] and
//! [`ATTRIBUTION_ICU`] for an About box. The tables are regenerated with
//! `tools/hijri/gen_umalqura.py`.

mod data;

use crate::calendar::{SupportedRange, day_offset};
use crate::{HijriCalendar, HijriDate};

/// Attribution for the official Umm al-Qura table.
pub const ATTRIBUTION_OFFICIAL: &str = "Umm al-Qura month starts (1343-1500 AH) from \
hijridate, https://github.com/dralshehri/hijridate. MIT License, \
Copyright (c) 2018-2025 Mohammed Alshehri.";

/// Attribution for the ICU Umm al-Qura table.
pub const ATTRIBUTION_ICU: &str = "Umm al-Qura table (1300-1600 AH) from ICU, \
https://github.com/unicode-org/icu. Unicode License V3, \
Copyright (c) 1991-2026 Unicode, Inc.";

/// One year of a table: the JDN of 1 Muḥarram and the lengths of the months.
#[derive(Debug, Clone, Copy)]
struct YearEntry {
    start: u32,
    lengths: [u8; 12],
}

impl YearEntry {
    const fn new(start: u32, lengths: [u8; 12]) -> Self {
        Self { start, lengths }
    }

    fn len(&self) -> i64 {
        self.lengths.iter().map(|&l| i64::from(l)).sum()
    }
}

/// A table-driven Hijri calendar (one entry per year).
#[derive(Debug)]
pub struct TableCalendar {
    id: &'static str,
    name: &'static str,
    first_year: i32,
    years: &'static [YearEntry],
}

impl TableCalendar {
    fn entry(&self, year: i32) -> Option<&YearEntry> {
        let i = usize::try_from(year.checked_sub(self.first_year)?).ok()?;
        self.years.get(i)
    }
}

impl HijriCalendar for TableCalendar {
    fn id(&self) -> &str {
        self.id
    }

    fn name(&self) -> &str {
        self.name
    }

    fn to_hijri(&self, jdn_civil_day: i64) -> Option<HijriDate> {
        // Index of the last year starting on or before the day.
        let i = self
            .years
            .partition_point(|y| i64::from(y.start) <= jdn_civil_day)
            .checked_sub(1)?;
        let entry = &self.years[i];
        let mut d = jdn_civil_day - i64::from(entry.start);
        if d >= entry.len() {
            return None;
        }
        let year = self.first_year + i32::try_from(i).ok()?;
        for (m, &len) in entry.lengths.iter().enumerate() {
            if d < i64::from(len) {
                return Some(HijriDate::new(year, m as u8 + 1, d as u8 + 1));
            }
            d -= i64::from(len);
        }
        None
    }

    fn from_hijri(&self, date: HijriDate) -> Option<i64> {
        let offset = day_offset(date, self.month_length(date.year, date.month))?;
        let entry = self.entry(date.year)?;
        let before: i64 = entry.lengths[..usize::from(date.month) - 1]
            .iter()
            .map(|&l| i64::from(l))
            .sum();
        Some(i64::from(entry.start) + before + offset)
    }

    fn month_length(&self, year: i32, month: u8) -> Option<u8> {
        if !(1..=12).contains(&month) {
            return None;
        }
        Some(self.entry(year)?.lengths[usize::from(month) - 1])
    }

    fn supported_range(&self) -> SupportedRange {
        SupportedRange {
            first_year: self.first_year,
            last_year: self.first_year + self.years.len() as i32 - 1,
        }
    }
}

/// The official Umm al-Qura calendar, 1343–1500 AH (1 August 1924 to
/// 16 November 2077).
pub static UMM_AL_QURA: TableCalendar = TableCalendar {
    id: "umm-al-qura",
    name: "Umm al-Qura (Saudi Arabia)",
    first_year: data::OFFICIAL_FIRST_YEAR,
    years: &data::OFFICIAL,
};

/// Umm al-Qura as tabulated by ICU/CLDR (`islamic-umalqura`), 1300–1600 AH.
pub static UMM_AL_QURA_ICU: TableCalendar = TableCalendar {
    id: "umm-al-qura-icu",
    name: "Umm al-Qura (ICU/CLDR table)",
    first_year: data::ICU_FIRST_YEAR,
    years: &data::ICU,
};

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
    fn known_dates() {
        for cal in [&UMM_AL_QURA, &UMM_AL_QURA_ICU] {
            let cases = [
                ((1446, 1, 1), (2024, 7, 7)),
                ((1445, 9, 1), (2024, 3, 11)),
                ((1444, 10, 1), (2023, 4, 21)),
                ((1444, 1, 1), (2022, 7, 30)),
                ((1420, 1, 1), (1999, 4, 17)),
            ];
            for ((hy, hm, hd), (gy, gm, gd)) in cases {
                let h = HijriDate::new(hy, hm, hd);
                assert_eq!(cal.from_hijri(h), Some(greg(gy, gm, gd)), "{}", cal.id());
                assert_eq!(cal.to_hijri(greg(gy, gm, gd)), Some(h), "{}", cal.id());
            }
        }
    }

    #[test]
    fn ranges() {
        let (first, last) = UMM_AL_QURA.supported_jdn_range().unwrap();
        assert_eq!(first, greg(1924, 8, 1));
        assert_eq!(last, greg(2077, 11, 16));
        assert_eq!(UMM_AL_QURA.to_hijri(first - 1), None);
        assert_eq!(UMM_AL_QURA.to_hijri(last + 1), None);
        assert_eq!(
            UMM_AL_QURA.to_hijri(last),
            Some(HijriDate::new(1500, 12, 30))
        );
        assert_eq!(UMM_AL_QURA.from_hijri(HijriDate::new(1342, 12, 1)), None);
        assert_eq!(UMM_AL_QURA.from_hijri(HijriDate::new(1501, 1, 1)), None);
        let r = UMM_AL_QURA_ICU.supported_range();
        assert_eq!((r.first_year, r.last_year), (1300, 1600));
        assert!(UMM_AL_QURA_ICU.supported_jdn_range().is_some());
    }

    #[test]
    fn tables_are_contiguous() {
        for cal in [&UMM_AL_QURA, &UMM_AL_QURA_ICU] {
            for pair in cal.years.windows(2) {
                assert_eq!(
                    i64::from(pair[0].start) + pair[0].len(),
                    i64::from(pair[1].start)
                );
            }
            // Since 1420 AH every month has 29 or 30 days and years 354/355.
            for year in 1420..=1500 {
                let len = cal.year_length(year).unwrap();
                assert!(len == 354 || len == 355, "{} {year}: {len}", cal.id());
                for m in 1..=12 {
                    let l = cal.month_length(year, m).unwrap();
                    assert!(l == 29 || l == 30);
                }
            }
        }
    }

    #[test]
    fn round_trip_every_day() {
        for cal in [&UMM_AL_QURA, &UMM_AL_QURA_ICU] {
            let (first, last) = cal.supported_jdn_range().unwrap();
            let mut prev: Option<HijriDate> = None;
            for jdn in first..=last {
                let h = cal.to_hijri(jdn).unwrap();
                assert_eq!(cal.from_hijri(h), Some(jdn));
                if let Some(p) = prev {
                    assert!(h > p);
                }
                prev = Some(h);
            }
        }
    }

    #[test]
    fn historical_irregular_months() {
        // The officially published calendar had a 28-day Ramadan in 1343 AH.
        assert_eq!(UMM_AL_QURA.month_length(1343, 9), Some(28));
        assert_eq!(UMM_AL_QURA.month_length(1345, 5), Some(31));
        assert!(UMM_AL_QURA.is_valid(HijriDate::new(1345, 5, 31)));
    }
}
