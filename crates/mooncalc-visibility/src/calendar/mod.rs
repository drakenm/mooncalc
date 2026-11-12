//! Hijri calendars driven by crescent visibility and by astronomical rules:
//! [`MonthStartProvider`]s for [`mooncalc_hijri::ObservedCalendar`].
//!
//! * [`VisibilityProvider`]: a month begins the day after the evening of its
//!   29th day if the crescent is seen that evening, by a criterion at a
//!   threshold zone (e.g. Yallop zone B or better, "naked eye"), at a place
//!   or anywhere in a set of places or a [`Region`]. Otherwise the month
//!   completes 30 days. Region presets ([`RegionPreset`]) give world-wide
//!   and tri-zonal calendars like MoonCalc 6's.
//! * [`RuleCalendar`]: the Umm al-Qura rules ([`UmmAlQuraRule`]) and
//!   conjunction rules (conjunction before sunset at Mecca, before 12:00 UT)
//!   computed with a real ephemeris.
//!
//! # Day convention and month numbering
//!
//! As everywhere in [`mooncalc_hijri`], a Hijri date is identified with the
//! civil day that begins after its first sunset: if the crescent is seen on
//! the evening of civil day `D`, the month's first day is `D + 1`. The
//! 29th-day recurrence, its anchoring to the conjunction sequence (which
//! keeps every month tied to its own lunation, so the numbering cannot drift)
//! and its safeguards are described in [`mooncalc_hijri::chained`].
//!
//! # Regions
//!
//! A region is sampled on a coarse grid (a few degrees). Each grid point is
//! a sea-level observer on its own local evening of the date (12h local mean
//! time anchors the day, the convention of the world maps; see
//! [`Request::local_date`](crate::circumstances::Request::local_date)), and
//! the evening counts as a sighting as soon as one point meets the threshold.
//! The evaluation is the map engine's (`mooncalc_maps::engine`): one
//! interpolated position table per evening shared by all points, then
//! [`compute_with`](crate::circumstances::compute_with) per point. It is
//! reimplemented here because `mooncalc-maps` depends on this crate. A
//! "not seen" evening over the 60° S – 60° N world at 5° costs about 1 700
//! point evaluations; a Hijri year needs some 30 evenings.
//!
//! # Portability
//!
//! No threads or I/O: the caller parallelises (e.g. one year per job). The
//! providers cache their results behind mutexes and are `Send + Sync`.

mod region;
mod rules;
mod sighting;

use std::sync::Arc;

use mooncalc_core::Ephemeris;
use mooncalc_hijri::{MonthStartProvider, TwentyNinthDayProvider};

pub use region::{LatLonBox, Observers, Region, RegionPreset};
pub use rules::{
    MECCA, MECCA_UTC_OFFSET_HOURS, RuleCalendar, UmmAlQuraRule, umm_al_qura_rule_provider,
};
pub use sighting::{SightingHit, SightingTest};

use crate::Criterion;
use crate::circumstances::Settings;

/// A boxed conjunction finder for [`TwentyNinthDayProvider`].
pub type BoxedConjunction = Box<dyn Fn(f64) -> Option<f64> + Send + Sync>;
/// A boxed evening test for [`TwentyNinthDayProvider`].
pub type BoxedTest = Box<dyn Fn(i64) -> bool + Send + Sync>;
/// Any month-start provider that can be shared between threads.
pub type DynProvider = Box<dyn MonthStartProvider + Send + Sync>;

/// Month starts from crescent sightings (see the module docs).
pub struct VisibilityProvider {
    test: Arc<SightingTest>,
    chain: TwentyNinthDayProvider<BoxedConjunction, BoxedTest>,
}

impl core::fmt::Debug for VisibilityProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VisibilityProvider")
            .field("test", &self.test)
            .finish_non_exhaustive()
    }
}

impl VisibilityProvider {
    /// Month starts by `criterion` at zone `max_zone` or better (0 = the
    /// most visible zone, e.g. Yallop's A) over `observers`, with the
    /// default [`Settings`] (standard horizon and atmosphere).
    pub fn new(
        eph: Arc<dyn Ephemeris>,
        criterion: Arc<dyn Criterion>,
        max_zone: usize,
        observers: &Observers,
    ) -> Self {
        Self::with_settings(eph, criterion, max_zone, observers, Settings::default())
    }

    /// As [`Self::new`], with explicit circumstance settings.
    pub fn with_settings(
        eph: Arc<dyn Ephemeris>,
        criterion: Arc<dyn Criterion>,
        max_zone: usize,
        observers: &Observers,
        settings: Settings,
    ) -> Self {
        let test = Arc::new(SightingTest::new(
            eph, criterion, max_zone, observers, settings,
        ));
        let (c, t) = (test.clone(), test.clone());
        let chain = TwentyNinthDayProvider::new(
            Box::new(move |jd| Some(c.conjunctions.nearest(jd).jd_ut)) as BoxedConjunction,
            Box::new(move |jdn| t.is_seen(jdn)) as BoxedTest,
        );
        Self { test, chain }
    }

    /// The sighting test, e.g. to show where a month's crescent was seen.
    pub fn sighting_test(&self) -> &SightingTest {
        &self.test
    }

    /// Whether the crescent counts as seen on the evening of civil day
    /// `jdn` (cached; the same answer the month starts use).
    pub fn is_seen(&self, jdn: i64) -> bool {
        self.chain.ends_month(jdn)
    }
}

impl MonthStartProvider for VisibilityProvider {
    fn month_start(&self, hijri_year: i32, month: u8) -> Option<i64> {
        self.chain.month_start(hijri_year, month)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::criteria::Yallop;
    use mooncalc_core::{GeoLocation, MeeusEphemeris};
    use mooncalc_hijri::civil::{CivilCalendar, CivilDate};
    use mooncalc_hijri::{HijriCalendar, ObservedCalendar, UMM_AL_QURA};

    fn eph() -> Arc<dyn Ephemeris> {
        Arc::new(MeeusEphemeris::new())
    }

    fn jdn(y: i32, m: u8, d: u8) -> i64 {
        CivilDate::new(y, m, d)
            .to_jdn(CivilCalendar::Gregorian)
            .unwrap()
    }

    #[test]
    fn ramadan_1445_at_mecca() {
        // New moon 2024 March 10 09:00 UT. At Mecca the crescent is not
        // seen that evening (age ~6 h) but is easily seen on March 11, so
        // Ramaḍān begins on March 12, a day after Umm al-Qura (March 11),
        // which needs only moonset after sunset.
        let p = VisibilityProvider::new(eph(), Arc::new(Yallop), 1, &Observers::Place(MECCA));
        assert!(!p.is_seen(jdn(2024, 3, 10)));
        assert!(p.is_seen(jdn(2024, 3, 11)));
        assert_eq!(p.month_start(1445, 9), Some(jdn(2024, 3, 12)));
        assert_eq!(UMM_AL_QURA.month_start(1445, 9), Some(jdn(2024, 3, 11)));
        let hit = p.sighting_test().first_sighting(jdn(2024, 3, 11)).unwrap();
        assert_eq!(hit.zone, 0);
        assert_eq!(hit.location, MECCA);
    }

    #[test]
    fn world_region_sees_earlier() {
        // On the evening of March 10 the crescent is visible in the
        // Americas (HMNAO), so a world-wide calendar starts on March 11.
        let world = Observers::Region {
            region: RegionPreset::World.region(),
            step_deg: 10.0,
        };
        let p = VisibilityProvider::new(eph(), Arc::new(Yallop), 1, &world);
        let hit = p.sighting_test().first_sighting(jdn(2024, 3, 10)).unwrap();
        assert!(hit.location.lon_deg < -30.0, "{hit:?}");
        assert!(p.sighting_test().first_sighting(jdn(2024, 3, 9)).is_none());
        let asia = Observers::Region {
            region: RegionPreset::AsiaPacific.region(),
            step_deg: 10.0,
        };
        let a = VisibilityProvider::new(eph(), Arc::new(Yallop), 1, &asia);
        assert!(!a.is_seen(jdn(2024, 3, 10)));
        assert!(a.is_seen(jdn(2024, 3, 11)));
    }

    #[test]
    fn rule_calendars_cover_a_year() {
        for rule in RuleCalendar::ALL {
            assert_eq!(RuleCalendar::from_id(rule.id()), Some(rule));
            let cal =
                ObservedCalendar::new(rule.provider(eph()), rule.id(), rule.name(), 1446, 1446);
            let len = cal.year_length(1446).unwrap();
            assert!((354..=355).contains(&len), "{rule:?}: {len}");
            for m in 1..=12 {
                let ours = cal.month_start(1446, m).unwrap();
                let uaq = UMM_AL_QURA.month_start(1446, m).unwrap();
                assert!((ours - uaq).abs() <= 1, "{rule:?} {m}: {ours} vs {uaq}");
            }
        }
    }

    #[test]
    fn providers_are_send_and_sync() {
        fn assert_sync<T: Send + Sync>() {}
        assert_sync::<VisibilityProvider>();
        assert_sync::<DynProvider>();
        assert_sync::<ObservedCalendar<VisibilityProvider>>();
        let _ = GeoLocation::new(0.0, 0.0, 0.0);
    }
}
