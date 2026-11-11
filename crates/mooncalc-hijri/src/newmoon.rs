//! "Astronomical new moon + rule" calendars.
//!
//! These calendars start each month from the time of the astronomical
//! (geocentric) conjunction plus a rule, without requiring a crescent
//! sighting. Examples:
//!
//! * [`ConjunctionBeforeSunset`]: the month begins on the day after the
//!   conjunction if the conjunction falls before sunset at a reference place
//!   (e.g. Mecca) on its local civil day; otherwise one day later.
//! * [`ConjunctionAndMoonset`]: additionally requires moonset after sunset
//!   on that day. This is the Umm al-Qura rule since 1420 AH (van Gent,
//!   "The Umm al-Qura Calendar of Saudi Arabia"). The real rule tests the
//!   29th day of the running month; here it is applied on the local day of
//!   conjunction, which is the same day except in rare edge cases. Use the
//!   [`UMM_AL_QURA`](crate::UMM_AL_QURA) table for the official calendar,
//!   or a [`TwentyNinthDayProvider`](crate::chained::TwentyNinthDayProvider)
//!   for the strict rule (`mooncalc_visibility::calendar::UmmAlQuraRule`).
//! * [`ConjunctionBeforeLocalTime`]: conjunction before a fixed local clock
//!   time, e.g. before 12:00 UT, or before midnight at a meridian.
//!
//! The ephemeris lives elsewhere, so the astronomy is passed in as closures:
//! a conjunction finder and, where needed, sunset/moonset times. All times
//! are Julian Dates in UT.

use crate::HijriCalendar;
use crate::observed::MonthStartProvider;
use crate::tabular::TABULAR_II_CIVIL;

/// Decides the first day of a month from the time of conjunction.
pub trait NewMoonRule {
    /// JDN of the first day of the month that follows a conjunction at
    /// `conjunction_jd_ut`.
    fn first_day(&self, conjunction_jd_ut: f64) -> Option<i64>;
}

/// The civil day (JDN) containing a UT instant at a place whose civil time
/// is UT + `utc_offset_hours`.
pub fn local_civil_day(jd_ut: f64, utc_offset_hours: f64) -> i64 {
    (jd_ut + 0.5 + utc_offset_hours / 24.0).floor() as i64
}

/// Month begins the day after the conjunction if the conjunction is before
/// sunset on its local civil day at the reference place; otherwise the day
/// after that.
///
/// `sunset(jdn)` returns the JD (UT) of sunset on local civil day `jdn`, or
/// `None` if there is none (polar day/night; the month start is then
/// undefined).
#[derive(Debug, Clone, Copy)]
pub struct ConjunctionBeforeSunset<S> {
    /// Civil time offset of the reference place, hours east of UT (Mecca +3).
    pub utc_offset_hours: f64,
    /// Sunset finder.
    pub sunset: S,
}

impl<S: Fn(i64) -> Option<f64>> NewMoonRule for ConjunctionBeforeSunset<S> {
    fn first_day(&self, conjunction_jd_ut: f64) -> Option<i64> {
        let day = local_civil_day(conjunction_jd_ut, self.utc_offset_hours);
        let sunset = (self.sunset)(day)?;
        Some(if conjunction_jd_ut < sunset {
            day + 1
        } else {
            day + 2
        })
    }
}

/// Sunset and moonset (JD UT) on one local civil day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SunMoonSet {
    /// Sunset, JD (UT).
    pub sunset: f64,
    /// Moonset following (or nearest) that sunset, JD (UT).
    pub moonset: f64,
}

/// Month begins the day after the conjunction if, on the conjunction's local
/// civil day at the reference place, the conjunction is before sunset and the
/// Moon sets after the Sun. Otherwise it begins a day later.
#[derive(Debug, Clone, Copy)]
pub struct ConjunctionAndMoonset<S> {
    /// Civil time offset of the reference place, hours east of UT (Mecca +3).
    pub utc_offset_hours: f64,
    /// Sunset/moonset finder for a local civil day.
    pub sun_moon_set: S,
}

impl<S: Fn(i64) -> Option<SunMoonSet>> NewMoonRule for ConjunctionAndMoonset<S> {
    fn first_day(&self, conjunction_jd_ut: f64) -> Option<i64> {
        let day = local_civil_day(conjunction_jd_ut, self.utc_offset_hours);
        let t = (self.sun_moon_set)(day)?;
        Some(if conjunction_jd_ut < t.sunset && t.moonset > t.sunset {
            day + 1
        } else {
            day + 2
        })
    }
}

/// Month begins the day after the conjunction if the conjunction is before
/// `cutoff_hours` local time on its local civil day, otherwise a day later.
///
/// For example `{ utc_offset_hours: 0.0, cutoff_hours: 12.0 }` is
/// "conjunction before 12:00 UT"; a cutoff of 24 means the month always
/// starts the day after the conjunction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConjunctionBeforeLocalTime {
    /// Civil time offset, hours east of UT.
    pub utc_offset_hours: f64,
    /// Local clock time of the cutoff, hours (0 to 24).
    pub cutoff_hours: f64,
}

impl NewMoonRule for ConjunctionBeforeLocalTime {
    fn first_day(&self, conjunction_jd_ut: f64) -> Option<i64> {
        let local = conjunction_jd_ut + 0.5 + self.utc_offset_hours / 24.0;
        let day = local.floor();
        let hours = (local - day) * 24.0;
        let day = day as i64;
        Some(if hours < self.cutoff_hours {
            day + 1
        } else {
            day + 2
        })
    }
}

/// A [`MonthStartProvider`] from a conjunction finder and a [`NewMoonRule`].
///
/// `conjunction_near(jd_ut)` must return the JD (UT) of the geocentric
/// conjunction (new moon) **nearest** to `jd_ut`. The provider calls it with
/// an estimate about 1.5 days before the tabular start of the month, which
/// lies within a few days of the true conjunction.
///
/// Wrap it in an [`ObservedCalendar`](crate::ObservedCalendar) for caching
/// and conversions.
#[derive(Debug, Clone, Copy)]
pub struct NewMoonRuleProvider<C, R> {
    /// Conjunction finder.
    pub conjunction_near: C,
    /// Month-start rule.
    pub rule: R,
}

impl<C, R> NewMoonRuleProvider<C, R> {
    /// Combines a conjunction finder with a rule.
    pub fn new(conjunction_near: C, rule: R) -> Self {
        Self {
            conjunction_near,
            rule,
        }
    }
}

impl<C, R> MonthStartProvider for NewMoonRuleProvider<C, R>
where
    C: Fn(f64) -> Option<f64>,
    R: NewMoonRule,
{
    fn month_start(&self, hijri_year: i32, month: u8) -> Option<i64> {
        let tabular = TABULAR_II_CIVIL.month_start(hijri_year, month)?;
        // Tabular day 1 is the civil day; its noon is JD `tabular`. The
        // conjunction usually precedes it by about one to two days.
        let conjunction = (self.conjunction_near)(tabular as f64 - 1.5)?;
        self.rule.first_day(conjunction)
    }
}

/// Mean synodic month, days (Meeus, *Astronomical Algorithms*, 2nd ed., ch. 49).
pub const SYNODIC_MONTH: f64 = 29.530_588_861;

/// Mean new moon (JDE) of lunation `k`, counted from the new moon of
/// 2000 January 6 (`k = 0`): Meeus, *Astronomical Algorithms* (2nd ed.),
/// eq. 49.1, without the periodic terms. It is in error by up to about 14
/// hours: good for numbering lunations and as a first guess, not for
/// calendars, which need an ephemeris.
pub fn mean_new_moon(k: i64) -> f64 {
    let k = k as f64;
    let t = k / 1236.85;
    2_451_550.097_66 + SYNODIC_MONTH * k + 0.000_154_37 * t * t - 0.000_000_150 * t * t * t
        + 0.000_000_000_73 * t * t * t * t
}

/// Mean new moon nearest to `jd` ([`mean_new_moon`]). Meant only for tests
/// and rough estimates; real calendars need an ephemeris.
pub fn mean_new_moon_near(jd: f64) -> f64 {
    mean_new_moon(((jd - 2_451_550.097_66) / SYNODIC_MONTH).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::civil::{CivilCalendar, CivilDate};
    use crate::{ObservedCalendar, UMM_AL_QURA};

    fn greg(y: i32, m: u8, d: u8) -> i64 {
        CivilDate::new(y, m, d)
            .to_jdn(CivilCalendar::Gregorian)
            .unwrap()
    }

    /// Sunset at 18:00 local time (UT+3) on every civil day.
    fn fixed_sunset(jdn: i64) -> Option<f64> {
        Some(jdn as f64 - 0.5 + (18.0 - 3.0) / 24.0)
    }

    #[test]
    fn mean_new_moon() {
        // Meeus example 49.a: the new moon of 1977 February (k = -283) has
        // mean phase JDE 2443192.94102.
        let jd = mean_new_moon_near(2_443_190.0);
        assert!((jd - 2_443_192.941_02).abs() < 1e-4, "{jd}");
    }

    #[test]
    fn local_days() {
        // 2024-07-05 22:00 UT is 2024-07-06 01:00 at UT+3.
        let jd = greg(2024, 7, 5) as f64 + 10.0 / 24.0;
        assert_eq!(local_civil_day(jd, 0.0), greg(2024, 7, 5));
        assert_eq!(local_civil_day(jd, 3.0), greg(2024, 7, 6));
    }

    #[test]
    fn rules() {
        let day = greg(2024, 7, 5);
        let at = |local_hours: f64| day as f64 - 0.5 + (local_hours - 3.0) / 24.0;
        let before_sunset = ConjunctionBeforeSunset {
            utc_offset_hours: 3.0,
            sunset: fixed_sunset,
        };
        assert_eq!(before_sunset.first_day(at(17.0)), Some(day + 1));
        assert_eq!(before_sunset.first_day(at(19.0)), Some(day + 2));
        let noon_ut = ConjunctionBeforeLocalTime {
            utc_offset_hours: 0.0,
            cutoff_hours: 12.0,
        };
        assert_eq!(noon_ut.first_day(day as f64 - 0.1), Some(day + 1));
        assert_eq!(noon_ut.first_day(day as f64 + 0.1), Some(day + 2));
        let moonset = |moonset_after: bool| ConjunctionAndMoonset {
            utc_offset_hours: 3.0,
            sun_moon_set: move |jdn: i64| {
                let sunset = fixed_sunset(jdn)?;
                let moonset = sunset + if moonset_after { 0.01 } else { -0.01 };
                Some(SunMoonSet { sunset, moonset })
            },
        };
        assert_eq!(moonset(true).first_day(at(10.0)), Some(day + 1));
        assert_eq!(moonset(false).first_day(at(10.0)), Some(day + 2));
        assert_eq!(moonset(true).first_day(at(20.0)), Some(day + 2));
        let no_sunset = ConjunctionBeforeSunset {
            utc_offset_hours: 3.0,
            sunset: |_| None,
        };
        assert_eq!(no_sunset.first_day(at(10.0)), None);
    }

    /// With mean conjunctions and a fixed sunset, "conjunction before sunset
    /// at Mecca" must stay within two days of Umm al-Qura, and produce a
    /// valid calendar.
    #[test]
    fn provider_tracks_umm_al_qura() {
        let provider = NewMoonRuleProvider::new(
            |jd| Some(mean_new_moon_near(jd)),
            ConjunctionBeforeSunset {
                utc_offset_hours: 3.0,
                sunset: fixed_sunset,
            },
        );
        let cal = ObservedCalendar::new(provider, "nm-mecca", "New moon (Mecca)", 1420, 1500);
        for year in 1420..=1500 {
            let len = cal.year_length(year).unwrap();
            assert!((354..=355).contains(&len), "{year}: {len}");
            for month in 1..=12 {
                let ours = cal.month_start(year, month).unwrap();
                let uaq = UMM_AL_QURA.month_start(year, month).unwrap();
                assert!((ours - uaq).abs() <= 2, "{year}/{month}: {ours} vs {uaq}");
            }
        }
        let jdn = greg(2024, 7, 20);
        let h = cal.to_hijri(jdn).unwrap();
        assert_eq!(cal.from_hijri(h), Some(jdn));
        assert_eq!(h.year, 1446);
        assert!(cal.to_hijri(greg(1990, 1, 1)).is_none());
    }
}
