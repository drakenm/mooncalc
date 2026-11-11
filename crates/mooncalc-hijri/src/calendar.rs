//! The [`HijriCalendar`] trait shared by every calendar in this crate.

use crate::HijriDate;

/// The Hijri years a calendar can convert (inclusive, whole years).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SupportedRange {
    /// First supported year AH.
    pub first_year: i32,
    /// Last supported year AH.
    pub last_year: i32,
}

impl SupportedRange {
    /// Whether `year` lies in the range.
    pub fn contains(&self, year: i32) -> bool {
        (self.first_year..=self.last_year).contains(&year)
    }
}

/// A Hijri calendar: a mapping between civil days and Hijri dates.
///
/// # Day convention
///
/// The Islamic day begins at sunset, so the evening before a civil day
/// already belongs to the next Hijri date. Every calendar in this crate
/// instead follows the **civil-day convention** used by almanacs and official
/// calendars: a Hijri date is identified with the civil day (midnight to
/// midnight) that starts after its first sunset, i.e. the daylight part of
/// the Hijri day. Civil days are given as integer Julian Day Numbers (the
/// Julian Date at noon of the Gregorian/Julian civil date; see
/// [`crate::civil`]).
///
/// Methods return `None` for dates outside [`supported_range`] or for invalid
/// dates such as 30 Ṣafar in a 29-day month.
///
/// [`supported_range`]: HijriCalendar::supported_range
pub trait HijriCalendar {
    /// Stable identifier, e.g. `"umm-al-qura"`.
    fn id(&self) -> &str;

    /// Human-readable name for display.
    fn name(&self) -> &str;

    /// The Hijri date of civil day `jdn_civil_day`.
    fn to_hijri(&self, jdn_civil_day: i64) -> Option<HijriDate>;

    /// The civil day (JDN) of `date`.
    // The name pairs with `to_hijri` and is part of the agreed API.
    #[allow(clippy::wrong_self_convention)]
    fn from_hijri(&self, date: HijriDate) -> Option<i64>;

    /// Length of `month` (1 to 12) of `year` in days.
    fn month_length(&self, year: i32, month: u8) -> Option<u8>;

    /// The years this calendar covers.
    fn supported_range(&self) -> SupportedRange;

    /// The civil day (JDN) of the first day of `month` in `year`.
    fn month_start(&self, year: i32, month: u8) -> Option<i64> {
        self.from_hijri(HijriDate::new(year, month, 1))
    }

    /// Length of `year` in days (354 or 355 for tabular calendars).
    fn year_length(&self, year: i32) -> Option<u16> {
        (1..=12).try_fold(0u16, |acc, m| {
            Some(acc + u16::from(self.month_length(year, m)?))
        })
    }

    /// Whether `date` exists in this calendar.
    fn is_valid(&self, date: HijriDate) -> bool {
        self.month_length(date.year, date.month)
            .is_some_and(|len| (1..=len).contains(&date.day))
    }

    /// First and last supported civil days (JDN), inclusive.
    fn supported_jdn_range(&self) -> Option<(i64, i64)> {
        let r = self.supported_range();
        let first = self.month_start(r.first_year, 1)?;
        let last_len = self.month_length(r.last_year, 12)?;
        let last = self.from_hijri(HijriDate::new(r.last_year, 12, last_len))?;
        Some((first, last))
    }
}

/// Validates `date` against a month-length function and returns the
/// zero-based day offset within the month.
pub(crate) fn day_offset(date: HijriDate, month_length: Option<u8>) -> Option<i64> {
    let len = month_length?;
    (1..=len)
        .contains(&date.day)
        .then(|| i64::from(date.day) - 1)
}
