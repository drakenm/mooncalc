//! Observing circumstances of the young (evening) or old (morning)
//! crescent: from an ephemeris, a place and a date to a full
//! [`CrescentGeometry`] at the criterion's evaluation time.
//!
//! # Procedure
//!
//! For an evening (new crescent) on local date D:
//!
//! 1. **Day anchor.** The Sun's upper transit nearest to the request's
//!    `noon` (for [`Request::local_date`], 12h local mean time on D).
//! 2. **Sunset** `Ts`: the first sunset in the 24 h after that transit.
//!    None means polar day or night ([`Outcome::NoSunEvent`]).
//! 3. **Moonset** `Tm`: the moonset nearest to `Ts` (within ±12 h, so a
//!    Moon that set an hour before the Sun yields a negative lag).
//! 4. **Lag** `Tm − Ts`. If ≤ 0 the Moon sets first
//!    ([`Outcome::MoonSetsFirst`]).
//! 5. **Evaluation time** by [`EvaluationTime`]: Yallop's best time
//!    `Tb = Ts + 4/9 Lag` (Yallop 1997, eq. 4.1; Odeh 2004, eq. 1), sunset,
//!    or the instant the Sun's centre reaches a given airless altitude.
//! 6. **Geometry** at that time ([`geometry_at`] states every definition),
//!    with the age from the geocentric conjunction in ecliptic longitude
//!    nearest to it. A non-positive age is [`Outcome::BeforeConjunction`].
//!
//! The morning (old crescent) mirrors this: sunrise `Tr` is the last sunrise
//! in the 24 h before the transit, moonrise the one nearest `Tr`, and the
//! best time `Tr − 4/9 Lag`.
//!
//! **Sign conventions for the morning.** The criteria take a crescent to be
//! present when `lag_minutes > 0` and `age_hours > 0`
//! ([`crate::criteria::crescent_present`]). So for the morning,
//! `lag_minutes` is sunrise − moonrise and `age_hours` is the time *until*
//! conjunction, both positive when there is an old crescent to see. (Yallop's
//! Table 4 prints both negative for morning records.)
//!
//! **Ephemeris coverage.** An ephemeris evaluated outside its valid range
//! (a DE440 file beyond its span) returns NaN coordinates. Before searching,
//! [`compute_with`] checks that the Sun's and Moon's positions are finite
//! across the window the searches use (about ±1.75 days around the request's
//! `noon`) and that a finite conjunction lies within half a synodic month of
//! it; otherwise it returns [`Outcome::OutsideEphemeris`] rather than a
//! polar or "not visible" result computed from NaN.
//!
//! Rise and set use [`Settings::horizon`], by default the standard horizon
//! of [`mooncalc_core::events::riseset`] (upper limb, 34′ refraction, level
//! horizon). That is the convention of HMNAO and USNO, and reproduces
//! Yallop's Table 4 lags (median 0.04 min). Odeh's (2004) Table VI best
//! times include the dip of the horizon for the site's elevation: use
//! `Horizon::UpperLimb { dip: true, .. }` to reproduce them.

mod snapshot;

pub use snapshot::{YALLOP_SD_PER_PARALLAX, geometry_at};

use mooncalc_core::GeoLocation;
use mooncalc_core::ephemeris::Ephemeris;
use mooncalc_core::events::phases::{Phase, SYNODIC_MONTH_DAYS, phase_near};
use mooncalc_core::events::{
    ApparentTable, Body, CalendarDate, EventKind, Horizon, Observer, PositionSource, Searcher,
};
use mooncalc_core::refraction::Atmosphere;
use mooncalc_core::time::Instant;

use crate::criteria::{Criterion, CriterionResult, EvaluationTime, crescent_present};
use crate::geometry::CrescentGeometry;

/// Which crescent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sighting {
    /// The new crescent after sunset.
    Evening,
    /// The old crescent before sunrise.
    Morning,
}

/// What to compute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub location: GeoLocation,
    /// An approximate local noon (UT). The Sun's upper transit nearest to it
    /// anchors the day: the evening follows it, the morning precedes it.
    pub noon: Instant,
    pub sighting: Sighting,
    pub evaluation: EvaluationTime,
}

impl Request {
    /// The evening or morning of local calendar date `date`, with the day
    /// anchored at 12h local mean time ([`CalendarDate::local_mean_noon`];
    /// the date line is the 180° meridian). This is the HMNAO/MoonCalc map
    /// convention. For a civil time zone, set `noon` to civil noon instead.
    pub fn local_date(
        location: GeoLocation,
        date: CalendarDate,
        sighting: Sighting,
        evaluation: EvaluationTime,
    ) -> Self {
        Self {
            location,
            noon: date.local_mean_noon(location.lon_deg),
            sighting,
            evaluation,
        }
    }
}

/// Computation settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Horizon for sunset/sunrise and moonset/moonrise.
    pub horizon: Horizon,
    /// Atmosphere for the apparent altitudes in the geometry.
    pub atmosphere: Atmosphere,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            horizon: Horizon::STANDARD,
            atmosphere: Atmosphere::STANDARD,
        }
    }
}

/// The computed circumstances of one evening or morning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circumstances {
    pub sighting: Sighting,
    /// Sunset (evening) or sunrise (morning).
    pub sun_event: Instant,
    /// Moonset (evening) or moonrise (morning) nearest the Sun event; `None`
    /// when the Moon stays up (then `geometry.lag_minutes` is +∞).
    pub moon_event: Option<Instant>,
    /// The instant the geometry is computed for.
    pub evaluation: Instant,
    /// The geocentric new Moon nearest the evaluation instant.
    pub conjunction: Instant,
    pub geometry: CrescentGeometry,
}

/// Polar conditions with no sunset (evening) or sunrise (morning).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Polar {
    /// The Sun stays above the horizon.
    MidnightSun,
    /// The Sun stays below the horizon.
    PolarNight,
}

/// The result for one place and date.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// A crescent to look for: lag > 0 and the evaluation instant is after
    /// conjunction (evening; before it in the morning).
    Crescent(Circumstances),
    /// Lag > 0, but the evaluation instant precedes conjunction (evening) or
    /// follows it (morning). The geometry is computed; age ≤ 0.
    BeforeConjunction(Circumstances),
    /// The Moon sets at or before sunset (rises at or after sunrise in the
    /// morning): lag ≤ 0. The geometry is computed at the Sun event.
    MoonSetsFirst(Circumstances),
    /// The Moon does not set (rise) within 12 h of the Sun event. With a
    /// sunset- or Sun-altitude-based evaluation time the circumstances are
    /// computed (lag +∞); Yallop's best time is undefined, so `None`.
    MoonAlwaysUp {
        sun_event: Instant,
        circumstances: Option<Circumstances>,
    },
    /// The Moon is below the horizon throughout the 24 h around the Sun event.
    MoonAlwaysDown { sun_event: Instant },
    /// No sunset (sunrise) in the day.
    NoSunEvent(Polar),
    /// [`EvaluationTime::SunAltitude`]: the Sun does not reach that altitude
    /// within 12 h after sunset (before sunrise).
    SunAltitudeNotReached { sun_event: Instant },
    /// The position source gave non-finite Sun or Moon positions around the
    /// request (the date lies outside the ephemeris coverage), or no finite
    /// conjunction lies within half a synodic month of it. Nothing was
    /// computed; callers should report the coverage rather than a verdict.
    OutsideEphemeris,
}

/// A compact status code per outcome, e.g. for map cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Status {
    Crescent = 0,
    BeforeConjunction = 1,
    MoonSetsFirst = 2,
    MoonAlwaysUp = 3,
    MoonAlwaysDown = 4,
    MidnightSun = 5,
    PolarNight = 6,
    SunAltitudeNotReached = 7,
    /// [`Outcome::OutsideEphemeris`].
    OutsideEphemeris = 8,
}

impl Status {
    /// Every status, indexed by its discriminant.
    pub const ALL: [Status; 9] = [
        Status::Crescent,
        Status::BeforeConjunction,
        Status::MoonSetsFirst,
        Status::MoonAlwaysUp,
        Status::MoonAlwaysDown,
        Status::MidnightSun,
        Status::PolarNight,
        Status::SunAltitudeNotReached,
        Status::OutsideEphemeris,
    ];
}

impl Outcome {
    pub fn status(&self) -> Status {
        match self {
            Self::Crescent(_) => Status::Crescent,
            Self::BeforeConjunction(_) => Status::BeforeConjunction,
            Self::MoonSetsFirst(_) => Status::MoonSetsFirst,
            Self::MoonAlwaysUp { .. } => Status::MoonAlwaysUp,
            Self::MoonAlwaysDown { .. } => Status::MoonAlwaysDown,
            Self::NoSunEvent(Polar::MidnightSun) => Status::MidnightSun,
            Self::NoSunEvent(Polar::PolarNight) => Status::PolarNight,
            Self::SunAltitudeNotReached { .. } => Status::SunAltitudeNotReached,
            Self::OutsideEphemeris => Status::OutsideEphemeris,
        }
    }

    /// The computed circumstances, if any.
    pub fn circumstances(&self) -> Option<&Circumstances> {
        match self {
            Self::Crescent(c) | Self::BeforeConjunction(c) | Self::MoonSetsFirst(c) => Some(c),
            Self::MoonAlwaysUp { circumstances, .. } => circumstances.as_ref(),
            _ => None,
        }
    }

    pub fn geometry(&self) -> Option<&CrescentGeometry> {
        self.circumstances().map(|c| &c.geometry)
    }

    /// Apply `criterion` where there is a crescent to look for: always for
    /// [`Outcome::Crescent`], and for [`Outcome::MoonAlwaysUp`] when its
    /// geometry exists and is after conjunction. `None` elsewhere, which
    /// maps render as "not visible".
    pub fn evaluate(&self, criterion: &dyn Criterion) -> Option<CriterionResult> {
        match self {
            Self::Crescent(c) => Some(criterion.evaluate(&c.geometry)),
            Self::MoonAlwaysUp {
                circumstances: Some(c),
                ..
            } if crescent_present(&c.geometry) => Some(criterion.evaluate(&c.geometry)),
            _ => None,
        }
    }
}

/// The new Moons nearest to `t` and one lunation either side, in time order:
/// enough to give the nearest conjunction anywhere within ±2 weeks of `t`.
pub fn new_moons_around(eph: &dyn Ephemeris, t: Instant) -> Vec<Instant> {
    let mid = phase_near(eph, Phase::New, t);
    vec![
        phase_near(eph, Phase::New, mid.add_days(-SYNODIC_MONTH_DAYS)),
        mid,
        phase_near(eph, Phase::New, mid.add_days(SYNODIC_MONTH_DAYS)),
    ]
}

/// Compute directly from an ephemeris. Builds a small interpolation table
/// around the request; for many places on one date use [`compute_with`] and
/// share the table and conjunctions.
pub fn compute(eph: &dyn Ephemeris, req: &Request, settings: &Settings) -> Outcome {
    let table =
        ApparentTable::with_default_step(eph, req.noon.add_days(-2.0), req.noon.add_days(2.0));
    let new_moons = new_moons_around(eph, req.noon);
    compute_with(&table, &new_moons, req, settings)
}

/// The nearest finite instant in `list` to `jd_ut`, if any.
fn nearest(list: &[Instant], jd_ut: f64) -> Option<Instant> {
    list.iter()
        .filter(|t| t.jd_ut.is_finite())
        .min_by(|a, b| (a.jd_ut - jd_ut).abs().total_cmp(&(b.jd_ut - jd_ut).abs()))
        .copied()
}

/// Offsets from the request's `noon`, days, at which [`compute_with`] checks
/// that the source is finite. The Sun-event search stays within a day of the
/// transit nearest `noon` and the Moon-event search within 0.55 d of the Sun
/// event, so every position the searches evaluate lies inside ±1.75 d; the
/// coverage of an ephemeris is one interval, so checking the ends (and the
/// middle) suffices.
const COVERAGE_PROBES: [f64; 3] = [-1.75, 0.0, 1.75];

/// Whether `src` gives finite Sun and Moon positions (and sidereal time)
/// throughout the window the searches around `noon` use.
fn source_covers<S: PositionSource + ?Sized>(src: &S, noon: f64) -> bool {
    COVERAGE_PROBES.iter().all(|dt| {
        let t = noon + dt;
        src.gast(t).is_finite()
            && [Body::Sun, Body::Moon]
                .iter()
                .all(|&b| src.position(b, t).iter().all(|x| x.is_finite()))
    })
}

/// Compute from a position source and a list of new Moons (geocentric
/// conjunctions; the one nearest the evaluation instant gives the age).
/// The source must cover about ±2 days around `req.noon`, and the list
/// must hold the conjunction nearest it: otherwise (non-finite positions,
/// or no finite conjunction within half a synodic month) the result is
/// [`Outcome::OutsideEphemeris`]. Non-finite entries in `new_moons` (phases
/// beyond the ephemeris coverage) are ignored.
pub fn compute_with<S: PositionSource + ?Sized>(
    src: &S,
    new_moons: &[Instant],
    req: &Request,
    settings: &Settings,
) -> Outcome {
    let noon = req.noon.jd_ut;
    // The evaluation instant lies within about two days of `noon`.
    let conjunction_known = nearest(new_moons, noon)
        .is_some_and(|c| (c.jd_ut - noon).abs() <= SYNODIC_MONTH_DAYS / 2.0 + 2.0);
    if !conjunction_known || !source_covers(src, noon) {
        return Outcome::OutsideEphemeris;
    }
    let obs = Observer::new(&req.location);
    let sun = Searcher::with_observer(src, Body::Sun, obs, settings.horizon);
    let moon = Searcher::with_observer(src, Body::Moon, obs, settings.horizon);
    let evening = req.sighting == Sighting::Evening;
    let (kind, dir) = if evening {
        (EventKind::Set, 1.0)
    } else {
        (EventKind::Rise, -1.0)
    };

    // 1–2. Sun event in the half-day after (before) the transit.
    let transit = sun.transit_near(req.noon.jd_ut, false);
    let (lo, hi) = if evening {
        (transit, transit + 1.0)
    } else {
        (transit - 1.0, transit)
    };
    let fast = sun
        .crossing_near(transit + dir * 0.25, kind)
        .filter(|t| (lo..=hi).contains(t));
    let ts = match fast.or_else(|| {
        let mut c = sun.crossings(lo, hi);
        c.retain(|e| e.kind == kind);
        let pick = if evening { c.first() } else { c.last() };
        pick.map(|e| e.time.jd_ut)
    }) {
        Some(t) => t,
        None => {
            return Outcome::NoSunEvent(if sun.margin(transit).0 < 0.0 {
                Polar::PolarNight
            } else {
                Polar::MidnightSun
            });
        }
    };
    let sun_event = Instant::from_jd_ut(ts);

    // 3. Moon event nearest the Sun event.
    let tm = moon
        .crossing_near(ts, kind)
        .filter(|t| (t - ts).abs() <= 0.5)
        .or_else(|| {
            moon.crossings(ts - 0.55, ts + 0.55)
                .into_iter()
                .filter(|e| e.kind == kind)
                .map(|e| e.time.jd_ut)
                .min_by(|a, b| (a - ts).abs().total_cmp(&(b - ts).abs()))
        });

    let atm = &settings.atmosphere;
    let build = |te: f64, moon_event: Option<f64>, lag_minutes: f64| {
        let conj = nearest(new_moons, te).expect("checked above: a finite new Moon");
        let age_hours = dir * (te - conj.jd_ut) * 24.0;
        Circumstances {
            sighting: req.sighting,
            sun_event,
            moon_event: moon_event.map(Instant::from_jd_ut),
            evaluation: Instant::from_jd_ut(te),
            conjunction: conj,
            geometry: geometry_at(src, &obs, te, lag_minutes, age_hours, atm),
        }
    };

    // Evaluation instant from a Sun-altitude rule, nearest the Sun event.
    let sun_altitude_time = |alt_deg: f64| -> Option<f64> {
        let s = Searcher::with_observer(
            src,
            Body::Sun,
            obs,
            Horizon::Centre {
                altitude_deg: alt_deg,
            },
        );
        s.crossing_near(ts, kind)
            .filter(|t| (t - ts).abs() <= 0.5)
            .or_else(|| {
                let (a, b) = if evening {
                    (ts - 0.25, ts + 0.5)
                } else {
                    (ts - 0.5, ts + 0.25)
                };
                s.crossings(a, b)
                    .into_iter()
                    .filter(|e| e.kind == kind)
                    .map(|e| e.time.jd_ut)
                    .min_by(|x, y| (x - ts).abs().total_cmp(&(y - ts).abs()))
            })
    };

    let Some(tm) = tm else {
        if moon.margin(ts).0 <= 0.0 {
            return Outcome::MoonAlwaysDown { sun_event };
        }
        let te = match req.evaluation {
            EvaluationTime::YallopBestTime => None,
            EvaluationTime::Sunset => Some(ts),
            EvaluationTime::SunAltitude(a) => sun_altitude_time(a),
        };
        return Outcome::MoonAlwaysUp {
            sun_event,
            circumstances: te.map(|te| build(te, None, f64::INFINITY)),
        };
    };

    // 4. Lag.
    let lag_days = dir * (tm - ts);
    let lag_minutes = lag_days * 1440.0;
    if lag_days <= 0.0 {
        return Outcome::MoonSetsFirst(build(ts, Some(tm), lag_minutes));
    }

    // 5. Evaluation time.
    let te = match req.evaluation {
        EvaluationTime::YallopBestTime => ts + dir * lag_days * 4.0 / 9.0,
        EvaluationTime::Sunset => ts,
        EvaluationTime::SunAltitude(a) => match sun_altitude_time(a) {
            Some(t) => t,
            None => return Outcome::SunAltitudeNotReached { sun_event },
        },
    };

    // 6. Geometry and age.
    let c = build(te, Some(tm), lag_minutes);
    if c.geometry.age_hours > 0.0 {
        Outcome::Crescent(c)
    } else {
        Outcome::BeforeConjunction(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::criteria::Yallop;
    use mooncalc_core::MeeusEphemeris;
    use mooncalc_core::events::{ApparentTable, Body, EventKind, Horizon, Searcher};

    fn evening(lat: f64, lon: f64, date: CalendarDate) -> Outcome {
        let eph = MeeusEphemeris::new();
        let req = Request::local_date(
            GeoLocation::new(lat, lon, 0.0),
            date,
            Sighting::Evening,
            EvaluationTime::YallopBestTime,
        );
        compute(&eph, &req, &Settings::default())
    }

    #[test]
    fn best_time_and_lag_are_consistent() {
        // Mecca, 2024 March 11: new Moon 2024 March 10 09:00 UT.
        let o = evening(21.4225, 39.8262, CalendarDate::new(2024, 3, 11));
        let Outcome::Crescent(c) = o else {
            panic!("{o:?}")
        };
        let g = &c.geometry;
        let ts = c.sun_event.jd_ut;
        let tm = c.moon_event.unwrap().jd_ut;
        assert!((g.lag_minutes - (tm - ts) * 1440.0).abs() < 1e-9);
        assert!((c.evaluation.jd_ut - (ts + 4.0 / 9.0 * (tm - ts))).abs() < 1e-12);
        // Sunset in Mecca ~ 15:31 UT in March.
        let cal = c.sun_event.to_calendar();
        assert_eq!((cal.day, cal.hour), (11, 15));
        assert!(g.lag_minutes > 60.0 && g.age_hours > 24.0 && g.age_hours < 36.0);
        assert!((g.age_hours - (c.evaluation.jd_ut - c.conjunction.jd_ut) * 24.0).abs() < 1e-9);
        assert_eq!(o.status(), Status::Crescent);
        assert_eq!(o.evaluate(&Yallop).unwrap().zone, 0);
    }

    #[test]
    fn before_conjunction_and_moon_first() {
        // Evening of 2024 March 9 (new Moon March 10 09:00 UT): old Moon sets
        // before the Sun nearly everywhere.
        let o = evening(21.4, 39.8, CalendarDate::new(2024, 3, 9));
        assert_eq!(o.status(), Status::MoonSetsFirst, "{o:?}");
        assert!(o.geometry().unwrap().lag_minutes < 0.0);
        assert!(o.evaluate(&Yallop).is_none());
        // Evening of the conjunction day in the far east (Fiji, 178° E):
        // sunset is about 06:30 UT, before the 09:00 conjunction.
        let o = evening(-18.0, 178.0, CalendarDate::new(2024, 3, 10));
        match o {
            Outcome::BeforeConjunction(c) => assert!(c.geometry.age_hours < 0.0),
            Outcome::MoonSetsFirst(c) => assert!(c.geometry.age_hours < 0.0),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn polar_and_morning() {
        let o = evening(78.0, 15.0, CalendarDate::new(2024, 6, 10));
        assert_eq!(o, Outcome::NoSunEvent(Polar::MidnightSun));
        let o = evening(78.0, 15.0, CalendarDate::new(2024, 12, 10));
        assert_eq!(o, Outcome::NoSunEvent(Polar::PolarNight));
        // Morning of 2024 March 8 at Mecca: old crescent ~ 2 days before
        // conjunction, rising well before the Sun.
        let eph = MeeusEphemeris::new();
        let req = Request::local_date(
            GeoLocation::new(21.4225, 39.8262, 0.0),
            CalendarDate::new(2024, 3, 8),
            Sighting::Morning,
            EvaluationTime::YallopBestTime,
        );
        let o = compute(&eph, &req, &Settings::default());
        let Outcome::Crescent(c) = o else {
            panic!("{o:?}")
        };
        let (tr, tm) = (c.sun_event.jd_ut, c.moon_event.unwrap().jd_ut);
        assert!(tm < tr);
        assert!((c.geometry.lag_minutes - (tr - tm) * 1440.0).abs() < 1e-9);
        assert!((c.evaluation.jd_ut - (tr - 4.0 / 9.0 * (tr - tm))).abs() < 1e-12);
        assert!(c.geometry.age_hours > 36.0);
        assert!(c.conjunction.jd_ut > c.evaluation.jd_ut);
        // Sunrise at Mecca ~ 03:40 UT.
        assert_eq!(c.sun_event.to_calendar().hour, 3);
    }

    #[test]
    fn fast_events_match_robust_search_at_high_latitudes() {
        // The fast local solvers must pick exactly the events the robust
        // bracketing search defines: the first sunset after the transit and
        // the moonset nearest it, including near polar day and night.
        let eph = MeeusEphemeris::new();
        let mut checked = 0;
        for (y, m, d) in [(2024, 3, 11), (2024, 6, 7), (2024, 12, 1), (2025, 4, 28)] {
            let date = CalendarDate::new(y, m, d);
            let noon0 = Instant::from_jd_ut(date.jd0() + 0.5);
            let table =
                ApparentTable::with_default_step(&eph, noon0.add_days(-2.5), noon0.add_days(2.5));
            let moons = new_moons_around(&eph, noon0);
            for lat in (-89..=89).step_by(4) {
                for lon in (-180..180).step_by(40) {
                    let loc = GeoLocation::new(lat as f64, lon as f64, 0.0);
                    let req =
                        Request::local_date(loc, date, Sighting::Evening, EvaluationTime::Sunset);
                    let o = compute_with(&table, &moons, &req, &Settings::default());
                    let sun = Searcher::new(&table, Body::Sun, &loc, Horizon::STANDARD);
                    let tt = sun.transit_near(req.noon.jd_ut, false);
                    let sets: Vec<f64> = sun
                        .crossings(tt, tt + 1.0)
                        .into_iter()
                        .filter(|e| e.kind == EventKind::Set)
                        .map(|e| e.time.jd_ut)
                        .collect();
                    match (sets.first(), o.circumstances()) {
                        (None, _) => {
                            assert!(matches!(o, Outcome::NoSunEvent(_)), "{lat} {lon} {o:?}")
                        }
                        (Some(&ts), Some(c)) => {
                            assert!(
                                (c.sun_event.jd_ut - ts).abs() * 86_400.0 < 0.5,
                                "{lat} {lon}"
                            );
                            let moon = Searcher::new(&table, Body::Moon, &loc, Horizon::STANDARD);
                            let near = moon
                                .crossings(ts - 0.55, ts + 0.55)
                                .into_iter()
                                .filter(|e| e.kind == EventKind::Set)
                                .map(|e| e.time.jd_ut)
                                .min_by(|a, b| (a - ts).abs().total_cmp(&(b - ts).abs()));
                            match (near, c.moon_event) {
                                (Some(a), Some(b)) => {
                                    assert!((a - b.jd_ut).abs() * 86_400.0 < 0.5, "{lat} {lon}")
                                }
                                (a, b) => assert_eq!(a.is_some(), b.is_some(), "{lat} {lon}"),
                            }
                            checked += 1;
                        }
                        (Some(_), None) => {
                            assert!(matches!(
                                o,
                                Outcome::MoonAlwaysDown { .. } | Outcome::MoonAlwaysUp { .. }
                            ))
                        }
                    }
                }
            }
        }
        assert!(checked > 1000, "{checked}");
    }

    /// Meeus inside `[from, to]` (TT JD), NaN coordinates outside: a DE440
    /// file evaluated beyond its coverage.
    struct Clipped {
        inner: MeeusEphemeris,
        from: f64,
        to: f64,
    }

    impl Clipped {
        /// Covering `days_before` to `days_after` around `t`.
        fn around(t: Instant, days_before: f64, days_after: f64) -> Self {
            Self {
                inner: MeeusEphemeris::new(),
                from: t.jd_tt() - days_before,
                to: t.jd_tt() + days_after,
            }
        }

        fn ok(&self, jd_tt: f64) -> bool {
            (self.from..=self.to).contains(&jd_tt)
        }
    }

    impl Ephemeris for Clipped {
        fn name(&self) -> &str {
            "clipped Meeus"
        }
        fn valid_range(&self) -> (f64, f64) {
            (self.from, self.to)
        }
        fn sun_equatorial(&self, jd_tt: f64) -> mooncalc_core::Equatorial {
            let e = self.inner.sun_equatorial(jd_tt);
            if self.ok(jd_tt) { e } else { nan_eq() }
        }
        fn moon_equatorial(&self, jd_tt: f64) -> mooncalc_core::Equatorial {
            let e = self.inner.moon_equatorial(jd_tt);
            if self.ok(jd_tt) { e } else { nan_eq() }
        }
        fn sun_ecliptic(&self, jd_tt: f64) -> mooncalc_core::Ecliptic {
            let e = self.inner.sun_ecliptic(jd_tt);
            if self.ok(jd_tt) { e } else { nan_ecl() }
        }
        fn moon_ecliptic(&self, jd_tt: f64) -> mooncalc_core::Ecliptic {
            let e = self.inner.moon_ecliptic(jd_tt);
            if self.ok(jd_tt) { e } else { nan_ecl() }
        }
    }

    fn nan_eq() -> mooncalc_core::Equatorial {
        mooncalc_core::Equatorial {
            ra: f64::NAN,
            dec: f64::NAN,
            distance_km: f64::NAN,
        }
    }

    fn nan_ecl() -> mooncalc_core::Ecliptic {
        mooncalc_core::Ecliptic {
            lon: f64::NAN,
            lat: f64::NAN,
            distance_km: f64::NAN,
        }
    }

    fn mecca_evening(date: CalendarDate) -> Request {
        Request::local_date(
            GeoLocation::new(21.4225, 39.8262, 0.0),
            date,
            Sighting::Evening,
            EvaluationTime::YallopBestTime,
        )
    }

    #[test]
    fn outside_the_ephemeris_is_reported_not_polar() {
        let req = mecca_evening(CalendarDate::new(2024, 3, 11));
        let noon = req.noon;
        // Entirely outside: coverage ended ten days earlier. Before this
        // check the NaN positions came out as `NoSunEvent(MidnightSun)`.
        let eph = Clipped::around(noon, 40.0, -10.0);
        let o = compute(&eph, &req, &Settings::default());
        assert_eq!(o, Outcome::OutsideEphemeris);
        assert_eq!(o.status(), Status::OutsideEphemeris);
        assert!(o.circumstances().is_none());
        assert!(o.evaluate(&Yallop).is_none());
        // Partly outside: the coverage ends during the evening's searches.
        let eph = Clipped::around(noon, 40.0, 0.5);
        assert_eq!(
            compute(&eph, &req, &Settings::default()),
            Outcome::OutsideEphemeris
        );
        let eph = Clipped::around(noon, 1.0, 40.0);
        assert_eq!(
            compute(&eph, &req, &Settings::default()),
            Outcome::OutsideEphemeris
        );
        // Positions covered, but the nearest conjunction (2024 March 10 or
        // April 8 around the March 25 full Moon) is not.
        let full = mecca_evening(CalendarDate::new(2024, 3, 25));
        let eph = Clipped::around(full.noon, 4.0, 4.0);
        assert_eq!(
            compute(&eph, &full, &Settings::default()),
            Outcome::OutsideEphemeris
        );
    }

    #[test]
    fn inside_the_ephemeris_matches_the_unclipped_result() {
        let req = mecca_evening(CalendarDate::new(2024, 3, 11));
        let full = compute(&MeeusEphemeris::new(), &req, &Settings::default());
        assert_eq!(full.status(), Status::Crescent);
        // The conjunction (March 10) and ±2 days are covered; the outer new
        // Moons of `new_moons_around` (February 9, April 8) are NaN.
        let eph = Clipped::around(req.noon, 3.0, 3.0);
        let moons = new_moons_around(&eph, req.noon);
        assert!(moons.iter().filter(|t| t.jd_ut.is_nan()).count() == 2);
        assert_eq!(compute(&eph, &req, &Settings::default()), full);
        // And `Status::ALL` is indexed by the discriminant.
        for (i, s) in Status::ALL.iter().enumerate() {
            assert_eq!(*s as usize, i);
        }
    }

    #[test]
    fn sunset_and_sun_altitude_evaluation() {
        let eph = MeeusEphemeris::new();
        let loc = GeoLocation::new(51.48, 0.0, 0.0);
        let date = CalendarDate::new(2024, 3, 12);
        let at = |e| {
            compute(
                &eph,
                &Request::local_date(loc, date, Sighting::Evening, e),
                &Settings::default(),
            )
        };
        let s = *at(EvaluationTime::Sunset).circumstances().unwrap();
        assert_eq!(s.evaluation, s.sun_event);
        // Apparent Sun altitude at sunset: the upper limb on the horizon,
        // i.e. the centre ~16′ below the refracted horizon.
        assert!(
            (s.geometry.sun_alt_deg + 16.0 / 60.0).abs() < 0.1,
            "{}",
            s.geometry.sun_alt_deg
        );
        let a = *at(EvaluationTime::SunAltitude(-4.0))
            .circumstances()
            .unwrap();
        assert!(a.evaluation.jd_ut > a.sun_event.jd_ut);
        let airless = a.geometry.sun_alt_deg; // nominal below −1°, see docs
        assert!(airless < -3.0);
        assert!(a.geometry.arcv_topo_deg > s.geometry.arcv_topo_deg);
        // London midsummer: the Sun never reaches −18° (astronomical night).
        let june = compute(
            &eph,
            &Request::local_date(
                loc,
                CalendarDate::new(2024, 6, 21),
                Sighting::Evening,
                EvaluationTime::SunAltitude(-18.0),
            ),
            &Settings::default(),
        );
        assert_eq!(june.status(), Status::SunAltitudeNotReached);
    }
}
