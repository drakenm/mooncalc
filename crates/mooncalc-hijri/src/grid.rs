//! Month grids for calendar views.
//!
//! [`hijri_month_grid`] lists the civil dates of each day of a Hijri month;
//! [`civil_month_grid`] lists the Hijri dates of each day of a civil month.
//! [`MonthGrid::weeks`] lays either out in week rows.

use crate::civil::{CivilCalendar, CivilDate};
use crate::{HijriCalendar, HijriDate, Weekday};

/// One civil day with both of its dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridDay {
    /// Julian Day Number of the civil day.
    pub jdn: i64,
    /// The Hijri date, or `None` if outside the calendar's range.
    pub hijri: Option<HijriDate>,
    /// The civil date.
    pub civil: CivilDate,
    /// Day of the week.
    pub weekday: Weekday,
}

impl GridDay {
    fn new<C: HijriCalendar + ?Sized>(calendar: &C, jdn: i64, civil: CivilCalendar) -> Self {
        Self {
            jdn,
            hijri: calendar.to_hijri(jdn),
            civil: CivilDate::from_jdn(jdn, civil),
            weekday: Weekday::from_jdn(jdn),
        }
    }
}

/// The consecutive days of one month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthGrid {
    /// The days in order (never empty).
    pub days: Vec<GridDay>,
}

impl MonthGrid {
    /// Lays out the days in rows of seven, the first column being
    /// `week_start` (e.g. [`Weekday::Saturday`] in much of the Islamic world,
    /// [`Weekday::Sunday`] or [`Weekday::Monday`] elsewhere). Cells before
    /// the first and after the last day are `None`.
    pub fn weeks(&self, week_start: Weekday) -> Vec<[Option<GridDay>; 7]> {
        let lead = usize::from(self.days[0].weekday.days_since(week_start));
        let cells = lead + self.days.len();
        let mut rows = vec![[None; 7]; cells.div_ceil(7)];
        for (i, day) in self.days.iter().enumerate() {
            let cell = lead + i;
            rows[cell / 7][cell % 7] = Some(*day);
        }
        rows
    }

    /// The distinct Hijri (year, month) pairs touched, in order.
    pub fn hijri_months(&self) -> Vec<(i32, u8)> {
        let mut out: Vec<(i32, u8)> = Vec::new();
        for h in self.days.iter().filter_map(|d| d.hijri) {
            if out.last() != Some(&(h.year, h.month)) {
                out.push((h.year, h.month));
            }
        }
        out
    }

    /// The distinct civil (year, month) pairs touched, in order.
    pub fn civil_months(&self) -> Vec<(i32, u8)> {
        let mut out: Vec<(i32, u8)> = Vec::new();
        for c in self.days.iter().map(|d| d.civil) {
            if out.last() != Some(&(c.year, c.month)) {
                out.push((c.year, c.month));
            }
        }
        out
    }
}

/// The days of Hijri month `month` of `year` in `calendar`, with civil dates
/// in `civil`. `None` if the month is outside the calendar's range.
pub fn hijri_month_grid<C: HijriCalendar + ?Sized>(
    calendar: &C,
    year: i32,
    month: u8,
    civil: CivilCalendar,
) -> Option<MonthGrid> {
    let start = calendar.month_start(year, month)?;
    let len = calendar.month_length(year, month)?;
    let days = (0..i64::from(len))
        .map(|i| GridDay::new(calendar, start + i, civil))
        .collect();
    Some(MonthGrid { days })
}

/// The days of civil month `month` of `year` (in `civil`), with Hijri dates
/// from `calendar`. Days outside the Hijri calendar's range have
/// `hijri: None`. Returns `None` only for an invalid month. Under
/// [`CivilCalendar::JulianGregorian`], October 1582 has 21 days.
pub fn civil_month_grid<C: HijriCalendar + ?Sized>(
    calendar: &C,
    year: i32,
    month: u8,
    civil: CivilCalendar,
) -> Option<MonthGrid> {
    let first = (1..=15).find_map(|d| CivilDate::new(year, month, d).to_jdn(civil))?;
    let days: Vec<GridDay> = (first..)
        .map(|jdn| GridDay::new(calendar, jdn, civil))
        .take_while(|d| d.civil.month == month)
        .collect();
    Some(MonthGrid { days })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UMM_AL_QURA;

    #[test]
    fn ramadan_1445() {
        let g = hijri_month_grid(&UMM_AL_QURA, 1445, 9, CivilCalendar::Gregorian).unwrap();
        assert_eq!(g.days.len(), 30); // Eid al-Fitr was 10 April 2024
        assert_eq!(g.days[0].civil, CivilDate::new(2024, 3, 11));
        assert_eq!(g.days[0].weekday, Weekday::Monday);
        assert_eq!(g.days[29].civil, CivilDate::new(2024, 4, 9));
        assert!(
            g.days
                .iter()
                .enumerate()
                .all(|(i, d)| { d.hijri == Some(HijriDate::new(1445, 9, i as u8 + 1)) })
        );
        assert_eq!(g.civil_months(), vec![(2024, 3), (2024, 4)]);
        let weeks = g.weeks(Weekday::Saturday);
        // Monday is the third column when weeks start on Saturday.
        assert_eq!(weeks[0][0], None);
        assert_eq!(weeks[0][1], None);
        assert_eq!(weeks[0][2], Some(g.days[0]));
        assert_eq!(weeks.len(), 5);
        let filled: usize = weeks.iter().flatten().filter(|c| c.is_some()).count();
        assert_eq!(filled, 30);
    }

    #[test]
    fn civil_month() {
        let g = civil_month_grid(&UMM_AL_QURA, 2024, 7, CivilCalendar::Gregorian).unwrap();
        assert_eq!(g.days.len(), 31);
        assert_eq!(g.days[6].hijri, Some(HijriDate::new(1446, 1, 1)));
        assert_eq!(g.hijri_months(), vec![(1445, 12), (1446, 1)]);
        let w = g.weeks(Weekday::Monday);
        assert_eq!(w[0][0].unwrap().civil.day, 1); // 1 July 2024 was a Monday
        assert!(civil_month_grid(&UMM_AL_QURA, 2024, 13, CivilCalendar::Gregorian).is_none());
        // Outside the table: civil dates still listed, Hijri dates missing.
        let g = civil_month_grid(&UMM_AL_QURA, 1900, 1, CivilCalendar::Gregorian).unwrap();
        assert!(g.days.iter().all(|d| d.hijri.is_none()));
        // The Gregorian reform month has 21 days in the mixed calendar.
        let g = civil_month_grid(&UMM_AL_QURA, 1582, 10, CivilCalendar::JulianGregorian).unwrap();
        assert_eq!(g.days.len(), 21);
        assert!(hijri_month_grid(&UMM_AL_QURA, 1600, 1, CivilCalendar::Gregorian).is_none());
    }
}
