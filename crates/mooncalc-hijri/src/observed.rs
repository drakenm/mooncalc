//! Calendars whose months start on observed or predicted dates.
//!
//! An [`ObservedCalendar`] gets the civil day of each 1st of the month from a
//! [`MonthStartProvider`] and caches the answers. Providers include:
//!
//! * crescent visibility at a chosen location under a chosen criterion, or
//!   MoonCalc-style regional calendars: `mooncalc_visibility::calendar`,
//!   built on [`TwentyNinthDayProvider`](crate::chained::TwentyNinthDayProvider).
//! * [`NewMoonRuleProvider`](crate::newmoon::NewMoonRuleProvider):
//!   astronomical conjunction plus a rule such as "conjunction before sunset
//!   at Mecca".
//! * [`TableProvider`]: a precomputed or announced list of month starts, also
//!   useful as a mock in tests.
//! * [`CalendarProvider`]: adapts any [`HijriCalendar`].

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::calendar::{SupportedRange, day_offset};
use crate::tabular::TABULAR_II_CIVIL;
use crate::{HijriCalendar, HijriDate};

/// Supplies the civil day on which each Hijri month begins.
///
/// # Contract
///
/// * `month_start(y, m)` is the JDN of the civil day of 1 `m` `y` (the day
///   after the evening on which the month begins; see the day convention
///   on [`HijriCalendar`]).
/// * Consecutive month starts must be 29 or 30 days apart. A crescent-based
///   provider must enforce the rule that a month that has not ended by
///   sighting after its 29th day completes 30 days. [`ObservedCalendar`]
///   treats any other spacing as an error and returns `None`.
/// * Results must be deterministic, because they are cached.
/// * Return `None` where no answer is available (outside the provider's
///   range, or if a computation fails).
pub trait MonthStartProvider {
    /// JDN of the first day of `month` (1 to 12) in `hijri_year`.
    fn month_start(&self, hijri_year: i32, month: u8) -> Option<i64>;
}

impl<P: MonthStartProvider + ?Sized> MonthStartProvider for &P {
    fn month_start(&self, hijri_year: i32, month: u8) -> Option<i64> {
        (**self).month_start(hijri_year, month)
    }
}

impl<P: MonthStartProvider + ?Sized> MonthStartProvider for Box<P> {
    fn month_start(&self, hijri_year: i32, month: u8) -> Option<i64> {
        (**self).month_start(hijri_year, month)
    }
}

/// Index of the month after `month` in `year`.
fn next_month(year: i32, month: u8) -> (i32, u8) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

fn prev_month(year: i32, month: u8) -> (i32, u8) {
    if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

/// A Hijri calendar built on a [`MonthStartProvider`], with caching.
///
/// The cache is behind a [`Mutex`], so the calendar is `Sync` when the
/// provider is, and can be shared between GUI and worker threads.
pub struct ObservedCalendar<P: MonthStartProvider> {
    provider: P,
    id: String,
    name: String,
    range: SupportedRange,
    cache: Mutex<BTreeMap<(i32, u8), Option<i64>>>,
}

impl<P: MonthStartProvider> core::fmt::Debug for ObservedCalendar<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ObservedCalendar")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("range", &self.range)
            .finish_non_exhaustive()
    }
}

impl<P: MonthStartProvider> ObservedCalendar<P> {
    /// Creates a calendar over `provider` covering the Hijri years
    /// `first_year..=last_year`.
    ///
    /// `id` should be stable for a given configuration (e.g.
    /// `"visibility-yallop-mecca"`), so that a GUI can persist the choice.
    pub fn new(
        provider: P,
        id: impl Into<String>,
        name: impl Into<String>,
        first_year: i32,
        last_year: i32,
    ) -> Self {
        Self {
            provider,
            id: id.into(),
            name: name.into(),
            range: SupportedRange {
                first_year,
                last_year,
            },
            cache: Mutex::new(BTreeMap::new()),
        }
    }

    /// The underlying provider.
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Forgets all cached month starts.
    pub fn clear_cache(&self) {
        self.lock().clear();
    }

    /// Fetches and caches all month starts of `years` (for example before
    /// drawing a year view), returning how many were available.
    pub fn prefetch(&self, years: core::ops::RangeInclusive<i32>) -> usize {
        years
            .flat_map(|y| (1..=12).map(move |m| (y, m)))
            .filter(|&(y, m)| self.cached_start(y, m).is_some())
            .count()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<(i32, u8), Option<i64>>> {
        // A poisoned cache only means another thread panicked mid-insert;
        // the map itself is still consistent.
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Month start from the cache or the provider. Years one outside the
    /// supported range may be queried, to bound the first and last months.
    fn cached_start(&self, year: i32, month: u8) -> Option<i64> {
        if let Some(&v) = self.lock().get(&(year, month)) {
            return v;
        }
        // Do not hold the lock while the provider computes.
        let v = self.provider.month_start(year, month);
        self.lock().insert((year, month), v);
        v
    }

    /// Start and length of a supported month.
    fn month_span(&self, year: i32, month: u8) -> Option<(i64, u8)> {
        if !self.range.contains(year) || !(1..=12).contains(&month) {
            return None;
        }
        let start = self.cached_start(year, month)?;
        let (ny, nm) = next_month(year, month);
        let len = self.cached_start(ny, nm)? - start;
        matches!(len, 29 | 30).then_some((start, len as u8))
    }
}

impl<P: MonthStartProvider> HijriCalendar for ObservedCalendar<P> {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn to_hijri(&self, jdn_civil_day: i64) -> Option<HijriDate> {
        // Estimate with the tabular calendar (within a couple of days of any
        // real lunar calendar), then walk to the containing month.
        if self.range.first_year > self.range.last_year {
            return None; // an empty range (`clamp` below needs min <= max)
        }
        let est = TABULAR_II_CIVIL.to_hijri(jdn_civil_day)?;
        // Clamp into the supported range so the walk only queries months the
        // provider is expected to know.
        let (mut y, mut m) =
            (est.year, est.month).clamp((self.range.first_year, 1), (self.range.last_year, 12));
        for _ in 0..4 {
            let (start, len) = self.month_span(y, m)?;
            if jdn_civil_day < start {
                (y, m) = prev_month(y, m);
            } else if jdn_civil_day >= start + i64::from(len) {
                (y, m) = next_month(y, m);
            } else {
                return Some(HijriDate::new(y, m, (jdn_civil_day - start) as u8 + 1));
            }
        }
        None
    }

    fn from_hijri(&self, date: HijriDate) -> Option<i64> {
        let (start, len) = self.month_span(date.year, date.month)?;
        Some(start + day_offset(date, Some(len))?)
    }

    fn month_length(&self, year: i32, month: u8) -> Option<u8> {
        self.month_span(year, month).map(|(_, len)| len)
    }

    fn supported_range(&self) -> SupportedRange {
        self.range
    }
}

/// A fixed list of consecutive month starts, beginning with month
/// `first_month` of `first_year`.
///
/// Useful for announced calendars, precomputed results, and as a mock
/// provider in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableProvider {
    first_year: i32,
    first_month: u8,
    starts: Vec<i64>,
}

impl TableProvider {
    /// `starts[0]` is the JDN of 1 `first_month` `first_year`, `starts[1]`
    /// that of the following month, and so on.
    pub fn new(first_year: i32, first_month: u8, starts: Vec<i64>) -> Self {
        Self {
            first_year,
            first_month,
            starts,
        }
    }

    /// Builds a table by sampling `calendar` over `first_year..=last_year`
    /// plus the following Muḥarram (so the last month has a known length).
    pub fn from_calendar<C: HijriCalendar + ?Sized>(
        calendar: &C,
        first_year: i32,
        last_year: i32,
    ) -> Option<Self> {
        let mut starts = Vec::new();
        for y in first_year..=last_year {
            for m in 1..=12 {
                starts.push(calendar.month_start(y, m)?);
            }
        }
        starts.push(calendar.month_start(last_year + 1, 1)?);
        Some(Self::new(first_year, 1, starts))
    }
}

impl MonthStartProvider for TableProvider {
    fn month_start(&self, hijri_year: i32, month: u8) -> Option<i64> {
        if !(1..=12).contains(&month) {
            return None;
        }
        let index = (i64::from(hijri_year) - i64::from(self.first_year)) * 12 + i64::from(month)
            - i64::from(self.first_month);
        self.starts.get(usize::try_from(index).ok()?).copied()
    }
}

/// Uses any [`HijriCalendar`] as a provider (for testing, or to base an
/// observed calendar on a tabular one outside its observed range).
#[derive(Debug, Clone, Copy)]
pub struct CalendarProvider<C>(pub C);

impl<C: HijriCalendar> MonthStartProvider for CalendarProvider<C> {
    fn month_start(&self, hijri_year: i32, month: u8) -> Option<i64> {
        self.0.month_start(hijri_year, month)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::{TABULAR_II_CIVIL, TABULAR_IV_ASTRONOMICAL};
    use crate::umalqura::UMM_AL_QURA;
    use std::cell::Cell;

    #[test]
    fn matches_underlying_calendar() {
        let base = TABULAR_IV_ASTRONOMICAL;
        let cal = ObservedCalendar::new(CalendarProvider(base), "t", "t", 1400, 1460);
        let (first, last) = cal.supported_jdn_range().unwrap();
        assert_eq!(first, base.month_start(1400, 1).unwrap());
        assert_eq!(last + 1, base.month_start(1461, 1).unwrap());
        for jdn in first..=last {
            let h = cal.to_hijri(jdn).unwrap();
            assert_eq!(Some(h), base.to_hijri(jdn));
            assert_eq!(cal.from_hijri(h), Some(jdn));
        }
        assert_eq!(cal.to_hijri(first - 1), None);
        assert_eq!(cal.to_hijri(last + 1), None);
    }

    #[test]
    fn table_provider_over_umm_al_qura() {
        // Umm al-Qura differs from tabular II by up to a few days.
        let table = TableProvider::from_calendar(&UMM_AL_QURA, 1420, 1499).unwrap();
        let cal = ObservedCalendar::new(table, "uaq-copy", "UAQ copy", 1420, 1499);
        let (first, last) = cal.supported_jdn_range().unwrap();
        for jdn in first..=last {
            assert_eq!(cal.to_hijri(jdn), UMM_AL_QURA.to_hijri(jdn));
        }
        assert_eq!(cal.year_length(1445), UMM_AL_QURA.year_length(1445));
    }

    struct Counting<'a> {
        calls: &'a Cell<usize>,
    }
    impl MonthStartProvider for Counting<'_> {
        fn month_start(&self, y: i32, m: u8) -> Option<i64> {
            self.calls.set(self.calls.get() + 1);
            TABULAR_II_CIVIL.month_start(y, m)
        }
    }

    #[test]
    fn caches_provider_calls() {
        let calls = Cell::new(0);
        let cal = ObservedCalendar::new(Counting { calls: &calls }, "c", "c", 1440, 1450);
        assert_eq!(cal.prefetch(1440..=1450), 132);
        let after_prefetch = calls.get();
        for jdn in TABULAR_II_CIVIL.month_start(1441, 1).unwrap()
            ..TABULAR_II_CIVIL.month_start(1449, 1).unwrap()
        {
            cal.to_hijri(jdn).unwrap();
        }
        assert_eq!(calls.get(), after_prefetch);
        cal.clear_cache();
        cal.month_length(1445, 9).unwrap();
        assert_eq!(calls.get(), after_prefetch + 2);
    }

    #[test]
    fn rejects_bad_month_lengths() {
        // Month 2 would be 31 days long.
        let b = TABULAR_II_CIVIL.month_start(1445, 1).unwrap();
        let p = TableProvider::new(1445, 1, vec![b, b + 30, b + 61, b + 90]);
        let cal = ObservedCalendar::new(p, "bad", "bad", 1445, 1445);
        assert_eq!(cal.month_length(1445, 1), Some(30));
        assert_eq!(cal.month_length(1445, 2), None);
        assert_eq!(cal.month_length(1445, 3), Some(29));
        assert_eq!(cal.month_length(1445, 4), None); // no next start
        assert_eq!(cal.to_hijri(b + 5), Some(HijriDate::new(1445, 1, 6)));
        assert_eq!(cal.to_hijri(b + 40), None);
        assert_eq!(cal.from_hijri(HijriDate::new(1445, 1, 31)), None);
    }

    #[test]
    fn table_provider_offsets() {
        let p = TableProvider::new(1445, 9, vec![10, 40, 69, 99, 128, 158]);
        assert_eq!(p.month_start(1445, 9), Some(10));
        assert_eq!(p.month_start(1445, 12), Some(99));
        assert_eq!(p.month_start(1446, 1), Some(128));
        assert_eq!(p.month_start(1446, 3), None);
        assert_eq!(p.month_start(1445, 8), None);
        assert_eq!(p.month_start(1445, 13), None);
    }

    #[test]
    fn empty_range_converts_nothing() {
        let cal = ObservedCalendar::new(CalendarProvider(TABULAR_II_CIVIL), "e", "e", 1, 0);
        assert_eq!(
            cal.to_hijri(TABULAR_II_CIVIL.month_start(1445, 1).unwrap()),
            None
        );
        assert_eq!(cal.month_start(1445, 1), None);
        assert_eq!(cal.supported_jdn_range(), None);
    }

    #[test]
    fn is_sync_for_sync_providers() {
        fn assert_sync<T: Sync + Send>() {}
        assert_sync::<ObservedCalendar<TableProvider>>();
    }
}
