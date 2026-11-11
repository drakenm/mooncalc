//! The crescent-sighting test behind [`super::VisibilityProvider`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mooncalc_core::events::phases::nearest_new_moon;
use mooncalc_core::events::{ApparentTable, CalendarDate, DirectSource, PositionSource};
use mooncalc_core::time::Instant;
use mooncalc_core::{Ephemeris, GeoLocation};
use mooncalc_hijri::newmoon::{SYNODIC_MONTH, mean_new_moon};

use super::region::Observers;
use crate::Criterion;
use crate::circumstances::{Request, Settings, Sighting, compute_with};

/// Epoch of the Meeus lunation count (mean new moon `k = 0`), JDE.
const LUNATION_EPOCH: f64 = 2_451_550.097_66;

/// With this many observers or fewer, positions come straight from the
/// ephemeris; with more, from one interpolation table per evening.
const DIRECT_SOURCE_LIMIT: usize = 8;

/// Geocentric conjunctions (true new moons), cached by lunation number.
pub(crate) struct Conjunctions {
    eph: Arc<dyn Ephemeris>,
    cache: Mutex<HashMap<i64, Instant>>,
}

impl Conjunctions {
    pub(crate) fn new(eph: Arc<dyn Ephemeris>) -> Self {
        Self {
            eph,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// The new moon of lunation `k` (Meeus numbering).
    pub(crate) fn of_lunation(&self, k: i64) -> Instant {
        let lock = || self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&t) = lock().get(&k) {
            return t;
        }
        let t = nearest_new_moon(&*self.eph, Instant::from_jd_tt(mean_new_moon(k)));
        lock().insert(k, t);
        t
    }

    fn lunation_near(jd: f64) -> i64 {
        ((jd - LUNATION_EPOCH) / SYNODIC_MONTH).round() as i64
    }

    /// The new moon nearest to `jd` (UT), by its mean lunation.
    pub(crate) fn nearest(&self, jd: f64) -> Instant {
        self.of_lunation(Self::lunation_near(jd))
    }

    /// The new moons of the lunation nearest `jd` and its neighbours, as
    /// [`compute_with`] wants them.
    pub(crate) fn around(&self, jd: f64) -> Vec<Instant> {
        let k = Self::lunation_near(jd);
        (k - 1..=k + 1).map(|k| self.of_lunation(k)).collect()
    }
}

/// Where and how well the crescent was seen on one evening.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SightingHit {
    /// The first place (west to east) where the crescent met the threshold.
    pub location: GeoLocation,
    /// Its zone (index into the criterion's zones).
    pub zone: usize,
    /// The criterion's value there, if it has one.
    pub value: Option<f64>,
}

/// "Is the crescent seen on the evening of this civil day?": the criterion
/// reaches zone `max_zone` or better at one of the observers, each on its
/// own local evening of that date (12h local mean time anchors the day, the
/// map convention of [`Request::local_date`]).
///
/// Places where there is no crescent to look for (Moon sets first, before
/// conjunction, no sunset) count as not seen; see
/// [`Outcome::evaluate`](crate::circumstances::Outcome::evaluate).
pub struct SightingTest {
    eph: Arc<dyn Ephemeris>,
    criterion: Arc<dyn Criterion>,
    max_zone: usize,
    points: Vec<GeoLocation>,
    settings: Settings,
    pub(crate) conjunctions: Conjunctions,
}

impl core::fmt::Debug for SightingTest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SightingTest")
            .field("criterion", &self.criterion.id())
            .field("max_zone", &self.max_zone)
            .field("points", &self.points.len())
            .finish_non_exhaustive()
    }
}

impl SightingTest {
    /// A test of `criterion` at zone `max_zone` or better (0 = the most
    /// visible zone) over `observers`.
    pub fn new(
        eph: Arc<dyn Ephemeris>,
        criterion: Arc<dyn Criterion>,
        max_zone: usize,
        observers: &Observers,
        settings: Settings,
    ) -> Self {
        Self {
            conjunctions: Conjunctions::new(eph.clone()),
            eph,
            criterion,
            max_zone,
            points: observers.points(),
            settings,
        }
    }

    pub fn criterion(&self) -> &dyn Criterion {
        &*self.criterion
    }

    pub fn max_zone(&self) -> usize {
        self.max_zone
    }

    /// The places tested, west to east.
    pub fn points(&self) -> &[GeoLocation] {
        &self.points
    }

    /// The first place, west to east, where the crescent meets the threshold
    /// on the evening of civil day `jdn`; `None` if it is not seen anywhere.
    pub fn first_sighting(&self, jdn: i64) -> Option<SightingHit> {
        let date = CalendarDate::from_jd(jdn as f64);
        let jd0 = date.jd0();
        let moons = self.conjunctions.around(jd0 + 1.0);
        if self.points.len() <= DIRECT_SOURCE_LIMIT {
            self.scan(&DirectSource::new(&*self.eph), date, &moons)
        } else {
            // The span the map engine tabulates for one date.
            let table = ApparentTable::with_default_step(
                &*self.eph,
                Instant::from_jd_ut(jd0 - 1.0),
                Instant::from_jd_ut(jd0 + 3.0),
            );
            self.scan(&table, date, &moons)
        }
    }

    /// Whether the crescent is seen on the evening of civil day `jdn`.
    pub fn is_seen(&self, jdn: i64) -> bool {
        self.first_sighting(jdn).is_some()
    }

    fn scan<S: PositionSource + ?Sized>(
        &self,
        src: &S,
        date: CalendarDate,
        moons: &[Instant],
    ) -> Option<SightingHit> {
        let evaluation = self.criterion.evaluation_time();
        self.points.iter().find_map(|&location| {
            let req = Request::local_date(location, date, Sighting::Evening, evaluation);
            let r = compute_with(src, moons, &req, &self.settings).evaluate(&*self.criterion)?;
            (r.zone <= self.max_zone).then_some(SightingHit {
                location,
                zone: r.zone,
                value: r.value,
            })
        })
    }
}
