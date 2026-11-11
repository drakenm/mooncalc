//! Calendars decided on the evening of the 29th day of each month.
//!
//! This is the rule of a sighting-based calendar and of the Umm al-Qura
//! rules since 1420 AH: on the evening of the 29th day of a month, a test is
//! made (is the crescent seen? does the Moon set after the Sun at Mecca?).
//! If it passes, the next day is the first of the new month; otherwise the
//! month completes 30 days. Months are therefore always 29 or 30 days long,
//! whatever the test does, which is the [`MonthStartProvider`] contract.
//!
//! [`TwentyNinthDayProvider`] implements the rule for any test, given as a
//! closure. The astronomy lives elsewhere: the visibility crate supplies
//! crescent-sighting and Umm al-Qura-rule tests.
//!
//! # Month numbering
//!
//! Each Hijri month is tied to one lunation: month `(year, month)` begins
//! after the conjunction of lunation
//! `k = (year − 1)·12 + (month − 1) − 17 037` ([`lunation_of_month`]), where
//! `k` counts new moons from 2000 January 6 as in Meeus, *Astronomical
//! Algorithms* (2nd ed.), ch. 49. The offset makes 1 Shawwāl 1420
//! (8 January 2000) follow the new moon `k = 0` of 6 January 2000. Because
//! the mapping is fixed arithmetic, the numbering cannot drift: every month
//! is looked up from its own conjunction, never by counting months from a
//! distant epoch. Tests check it against every month of the Umm al-Qura
//! table (1343–1500 AH) and the tabular calendar (1–3000 AH).
//!
//! # Anchors, chains and merging
//!
//! The 29th-day rule is a recurrence: the start of a month depends on the
//! start of the previous one. To give random access with bounded work, the
//! provider runs one chain per Hijri year:
//!
//! 1. **Anchor.** Chain `Y` starts at 1 Muḥarram `Y`, placed the day after
//!    the first evening, from the day before the conjunction's UT date on,
//!    on which the test passes (the "unconstrained" start).
//! 2. **Chain.** Each later month follows by the 29th-day rule. The chain is
//!    extended into year `Y + 1` as far as needed.
//! 3. **Merge.** Chains that agree on one month agree on every later month.
//!    Chain `Y − 1`, continued into year `Y`, normally meets chain `Y` at
//!    Muḥarram or within a month or two (each 29-day month moves a chain
//!    about half a day earlier relative to the Moon, each 30-day month about
//!    half a day later, so both settle at the visibility edge). Months of
//!    year `Y` before that meeting point come from chain `Y − 1`, later ones
//!    from chain `Y`. The joined sequence obeys the rule everywhere.
//!
//! A month of year `Y` therefore needs at most chains `Y − 1` and `Y`: about
//! 30 tests, which are cached. If the chains have not met by the end of year
//! `Y` (not seen in practice), year `Y` keeps chain `Y − 1`, and the switch to
//! chain `Y` at the next Muḥarram can produce an invalid month length, which
//! [`ObservedCalendar`](crate::ObservedCalendar) reports as `None`.
//!
//! # Safeguards
//!
//! A test that never passes (no crescent for months, e.g. near the poles)
//! would let 30-day months drift later and later, and one that always passes
//! would drift earlier. So a month never starts before the day before the
//! conjunction's UT date, nor more than [`LATEST_START_DAYS`] days after it:
//! at those bounds the month is closed at 30 or 29 days regardless of the
//! test. Real crescent and Umm al-Qura tests at inhabited latitudes stay
//! inside the bounds.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::newmoon::mean_new_moon;
use crate::observed::MonthStartProvider;

/// `month_index(1420, 10) − 0`: the month index whose lunation is `k = 0`
/// (Shawwāl 1420, after the new moon of 2000 January 6).
pub const LUNATION_OFFSET: i64 = 17_037;

/// A month never starts more than this many days after the UT civil date
/// of its conjunction (see the module docs).
pub const LATEST_START_DAYS: i64 = 4;

/// A month never starts more than this many days before the UT civil date
/// of its conjunction.
pub const EARLIEST_START_DAYS: i64 = 1;

/// Months counted from 1 Muḥarram 1 AH: `(year − 1)·12 + month − 1`.
pub fn month_index(year: i32, month: u8) -> i64 {
    (i64::from(year) - 1) * 12 + i64::from(month) - 1
}

/// Inverse of [`month_index`].
pub fn month_from_index(index: i64) -> (i32, u8) {
    (
        (index.div_euclid(12) + 1) as i32,
        (index.rem_euclid(12) + 1) as u8,
    )
}

/// The lunation (Meeus `k`, new moon of 2000 January 6 = 0) whose
/// conjunction begins `month` of `year`.
pub fn lunation_of_month(year: i32, month: u8) -> i64 {
    month_index(year, month) - LUNATION_OFFSET
}

/// The Hijri month (year, month) that lunation `k` begins.
pub fn month_of_lunation(k: i64) -> (i32, u8) {
    month_from_index(k + LUNATION_OFFSET)
}

/// The civil day (JDN) containing a UT Julian Date.
fn ut_civil_day(jd: f64) -> i64 {
    (jd + 0.5).floor() as i64
}

#[derive(Default)]
struct Cache {
    /// Conjunction (JD UT) per lunation.
    conjunctions: HashMap<i64, Option<f64>>,
    /// Test result per civil day.
    tests: HashMap<i64, bool>,
    /// Chain per anchor year: month starts from 1 Muḥarram of that year on.
    chains: HashMap<i32, Vec<i64>>,
    /// Per year, the first month (0-based) where the previous year's chain
    /// meets this year's.
    merges: HashMap<i32, Option<usize>>,
}

/// A [`MonthStartProvider`] for the 29th-day rule (see the module docs).
///
/// * `conjunction_near(jd_ut)` returns the JD (UT) of the geocentric
///   conjunction nearest to `jd_ut`. It is called with the mean new moon
///   of each lunation, which is within 14 hours of the true one.
/// * `ends_month(jdn)` is the test made on the evening of civil day `jdn`:
///   `true` means the month ends that evening, so the next one starts on
///   `jdn + 1`. It is asked about the 29th day of a month, and for anchors
///   about the days from the day before a conjunction's UT date on.
///
/// Results are cached (conjunctions, tests and chains), so both closures
/// must be deterministic. The caches are behind a [`Mutex`], so the
/// provider is `Sync` when the closures are; computations run without the
/// lock held.
pub struct TwentyNinthDayProvider<C, T> {
    conjunction_near: C,
    ends_month: T,
    cache: Mutex<Cache>,
}

impl<C, T> core::fmt::Debug for TwentyNinthDayProvider<C, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TwentyNinthDayProvider")
            .finish_non_exhaustive()
    }
}

impl<C, T> TwentyNinthDayProvider<C, T>
where
    C: Fn(f64) -> Option<f64>,
    T: Fn(i64) -> bool,
{
    /// A provider from a conjunction finder and an evening test.
    pub fn new(conjunction_near: C, ends_month: T) -> Self {
        Self {
            conjunction_near,
            ends_month,
            cache: Mutex::new(Cache::default()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Cache> {
        // A poisoned cache only means another thread panicked mid-insert.
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The conjunction (JD UT) of lunation `k`. A non-finite result (an
    /// ephemeris evaluated outside its range returns NaN) counts as a failed
    /// computation, so the months that depend on it are `None`.
    pub fn conjunction(&self, k: i64) -> Option<f64> {
        if let Some(&c) = self.lock().conjunctions.get(&k) {
            return c;
        }
        let c = (self.conjunction_near)(mean_new_moon(k)).filter(|c| c.is_finite());
        self.lock().conjunctions.insert(k, c);
        c
    }

    /// The (cached) evening test for civil day `jdn`.
    pub fn ends_month(&self, jdn: i64) -> bool {
        if let Some(&v) = self.lock().tests.get(&jdn) {
            return v;
        }
        let v = (self.ends_month)(jdn);
        self.lock().tests.insert(jdn, v);
        v
    }

    /// UT civil date of the conjunction that begins month `index`.
    fn conjunction_day(&self, index: i64) -> Option<i64> {
        self.conjunction(index - LUNATION_OFFSET).map(ut_civil_day)
    }

    /// The unconstrained start of month `index`: the day after the first
    /// evening on which the test passes, from the day before the
    /// conjunction's UT date on, and no later than the latest start.
    fn anchor(&self, index: i64) -> Option<i64> {
        let d = self.conjunction_day(index)?;
        let first = d - EARLIEST_START_DAYS - 1;
        let last = d + LATEST_START_DAYS - 1;
        Some(
            (first..=last)
                .find(|&x| self.ends_month(x))
                .map_or(d + LATEST_START_DAYS, |x| x + 1),
        )
    }

    /// Month start `i` (0-based from 1 Muḥarram `year`) of chain `year`.
    fn chain_at(&self, year: i32, i: usize) -> Option<i64> {
        let first = month_index(year, 1);
        let mut chain = match self.lock().chains.get(&year) {
            Some(c) if c.len() > i => return Some(c[i]),
            Some(c) => c.clone(),
            None => Vec::new(),
        };
        if chain.is_empty() {
            chain.push(self.anchor(first)?);
        }
        while chain.len() <= i {
            let s = *chain.last().expect("chain has an anchor");
            let next = first + chain.len() as i64;
            let d = self.conjunction_day(next)?;
            let earliest = d - EARLIEST_START_DAYS;
            let latest = d + LATEST_START_DAYS;
            let short = (self.ends_month(s + 28) && s + 29 >= earliest) || s + 30 > latest;
            chain.push(if short { s + 29 } else { s + 30 });
        }
        let value = chain[i];
        let mut cache = self.lock();
        let stored = cache.chains.entry(year).or_default();
        if stored.len() < chain.len() {
            *stored = chain;
        }
        Some(value)
    }

    /// The first month (0-based) of `year` where chain `year − 1` meets
    /// chain `year`, if they meet within the year.
    fn merge(&self, year: i32) -> Option<usize> {
        if let Some(&m) = self.lock().merges.get(&year) {
            return m;
        }
        let mut m = None;
        for i in 0..12 {
            match (self.chain_at(year - 1, 12 + i), self.chain_at(year, i)) {
                (Some(a), Some(b)) if a == b => {
                    m = Some(i);
                    break;
                }
                (Some(_), Some(_)) => {}
                // A failed computation: do not cache, so a later call may retry.
                _ => return None,
            }
        }
        self.lock().merges.insert(year, m);
        m
    }
}

impl<C, T> MonthStartProvider for TwentyNinthDayProvider<C, T>
where
    C: Fn(f64) -> Option<f64>,
    T: Fn(i64) -> bool,
{
    fn month_start(&self, hijri_year: i32, month: u8) -> Option<i64> {
        if !(1..=12).contains(&month) {
            return None;
        }
        let i = usize::from(month - 1);
        match self.merge(hijri_year) {
            Some(m) if i >= m => self.chain_at(hijri_year, i),
            _ => self.chain_at(hijri_year - 1, 12 + i),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::newmoon::mean_new_moon_near;
    use crate::tabular::TABULAR_II_CIVIL;
    use crate::{HijriCalendar, ObservedCalendar, UMM_AL_QURA};

    #[test]
    fn numbering_round_trips() {
        assert_eq!(lunation_of_month(1420, 10), 0);
        // 1 Ramaḍān 1445 followed the new moon of 2024 March 10 (k = 299).
        assert_eq!(lunation_of_month(1445, 9), 299);
        assert_eq!(month_of_lunation(303), (1446, 1));
        for n in -50..50_000 {
            let (y, m) = month_from_index(n);
            assert!((1..=12).contains(&m));
            assert_eq!(month_index(y, m), n);
        }
    }

    /// Days from the mean new moon of a month's lunation to its first civil
    /// day (JDN = noon of that day).
    fn lag(cal: &dyn HijriCalendar, y: i32, m: u8) -> f64 {
        cal.month_start(y, m).unwrap() as f64 - mean_new_moon(lunation_of_month(y, m))
    }

    #[test]
    fn numbering_matches_umm_al_qura_and_tabular() {
        // Every Umm al-Qura month starts 0.5 to 3 days after the mean new
        // moon its number points to (the true new moon is within 14 h of
        // it); a misnumbered month would be ~29.5 days off.
        let range = UMM_AL_QURA.supported_range();
        let mut worst = (f64::MAX, f64::MIN);
        for y in range.first_year..=range.last_year {
            for m in 1..=12 {
                let d = lag(&UMM_AL_QURA, y, m);
                worst = (worst.0.min(d), worst.1.max(d));
            }
        }
        assert!(worst.0 > -1.0 && worst.1 < 3.5, "{worst:?}");
        // The tabular calendar stays within ±2.5 days of the mean moon over
        // three millennia.
        for y in (1..=3000).step_by(7) {
            for m in 1..=12 {
                let d = lag(&TABULAR_II_CIVIL, y, m);
                assert!((-1.5..3.5).contains(&d), "{y}/{m}: {d}");
            }
        }
    }

    /// "Moon older than `hours` at 15:00 UT (≈ sunset at Mecca)", with the
    /// nearest mean new moon (negative age before it): a smooth stand-in
    /// for a crescent test.
    fn age_test(hours: f64) -> impl Fn(i64) -> bool {
        move |jdn| {
            let t = jdn as f64 + 2.5 / 24.0; // 15h UT
            (t - mean_new_moon_near(t)) * 24.0 >= hours
        }
    }

    fn mean_conj(jd: f64) -> Option<f64> {
        Some(mean_new_moon_near(jd))
    }

    fn check_contract<P: MonthStartProvider>(p: P, years: core::ops::RangeInclusive<i32>) {
        let (first, last) = (*years.start(), *years.end());
        let cal = ObservedCalendar::new(p, "t", "t", first, last);
        for y in first..=last {
            let len = cal.year_length(y).unwrap_or_else(|| panic!("{y}"));
            assert!((354..=355).contains(&len), "{y}: {len}");
            for m in 1..=12 {
                let s = cal.month_start(y, m).unwrap();
                let d = cal.provider().month_start(y, m).unwrap();
                assert_eq!(s, d);
                let c = mean_new_moon(lunation_of_month(y, m));
                let day = (c + 0.5).floor() as i64;
                assert!(
                    (day - EARLIEST_START_DAYS..=day + LATEST_START_DAYS).contains(&s),
                    "{y}/{m}"
                );
            }
        }
    }

    #[test]
    fn keeps_the_29_30_contract() {
        for hours in [0.0, 12.0, 20.0, 30.0, 45.0] {
            check_contract(
                TwentyNinthDayProvider::new(mean_conj, age_test(hours)),
                1400..=1460,
            );
        }
        // Degenerate tests stay bounded by the safeguards.
        check_contract(
            TwentyNinthDayProvider::new(mean_conj, |_| true),
            1440..=1450,
        );
        check_contract(
            TwentyNinthDayProvider::new(mean_conj, |_| false),
            1440..=1450,
        );
        // An erratic test.
        check_contract(
            TwentyNinthDayProvider::new(mean_conj, |jdn: i64| {
                jdn.wrapping_mul(2_654_435_761).rem_euclid(7) < 4
            }),
            1440..=1450,
        );
    }

    #[test]
    fn follows_the_rule_and_one_long_chain() {
        // The joined per-year chains equal a single chain run from far back.
        let test = age_test(20.0);
        let p = TwentyNinthDayProvider::new(mean_conj, age_test(20.0));
        let mut s = p.month_start(1400, 1).unwrap();
        for n in month_index(1400, 1) + 1..month_index(1460, 1) {
            s += if test(s + 28) { 29 } else { 30 };
            let (y, m) = month_from_index(n);
            assert_eq!(p.month_start(y, m), Some(s), "{y}/{m}");
        }
    }

    #[test]
    fn order_independent() {
        let a = TwentyNinthDayProvider::new(mean_conj, age_test(15.0));
        let b = TwentyNinthDayProvider::new(mean_conj, age_test(15.0));
        let forward: Vec<_> = (1430..=1440)
            .flat_map(|y| (1..=12).map(move |m| (y, m)))
            .map(|(y, m)| a.month_start(y, m))
            .collect();
        let mut backward: Vec<_> = (1430..=1440)
            .rev()
            .flat_map(|y| (1..=12).rev().map(move |m| (y, m)))
            .map(|(y, m)| b.month_start(y, m))
            .collect();
        backward.reverse();
        assert_eq!(forward, backward);
        assert_eq!(a.month_start(1440, 13), None);
    }

    #[test]
    fn non_finite_conjunctions_give_no_months() {
        // An ephemeris outside its range returns NaN. Before this was
        // filtered, the NaN conjunction became civil day 0 and the months
        // came out as JDN ~352 (4708 BC) instead of `None`.
        let p = TwentyNinthDayProvider::new(|_| Some(f64::NAN), age_test(15.0));
        assert_eq!(p.month_start(1600, 1), None);
        assert_eq!(p.month_start(1600, 7), None);
        let cal = ObservedCalendar::new(p, "t", "t", 1600, 1600);
        assert_eq!(cal.month_start(1600, 3), None);
        assert_eq!(cal.year_length(1600), None);
        // Partly covered: months whose conjunctions are finite still work.
        let edge = mean_new_moon(lunation_of_month(1446, 6));
        let p = TwentyNinthDayProvider::new(
            move |jd: f64| {
                Some(if jd < edge {
                    mean_new_moon_near(jd)
                } else {
                    f64::NAN
                })
            },
            age_test(15.0),
        );
        assert!(p.month_start(1446, 3).is_some());
        assert_eq!(p.month_start(1446, 8), None);
    }

    #[test]
    fn is_sync() {
        fn assert_sync<T: Sync + Send>(_: &T) {}
        let p = TwentyNinthDayProvider::new(mean_conj, age_test(15.0));
        assert_sync(&p);
    }
}
