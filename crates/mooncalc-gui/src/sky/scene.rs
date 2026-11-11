//! What the Sky and Trajectory tabs show, computed from an ephemeris:
//! topocentric Sun and Moon states ([`Snapshot`]), the evening or morning
//! of a crescent sighting ([`Night`]) and one local day ([`Day`]).
//!
//! Both scenes tabulate the ephemeris once ([`ApparentTable`], cubic
//! interpolation at 30-minute nodes, far below 0.001″ of error) and then
//! evaluate any instant cheaply, so the views can animate per frame.

use mooncalc_core::angle::norm_pi;
use mooncalc_core::events::phases::{Phase, phases_between};
use mooncalc_core::events::{
    ApparentTable, Body, CalendarDate, DayEvents, Horizon, Observer, PositionSource, Twilight,
    day_events, twilight,
};
use mooncalc_core::illumination::Illumination;
use mooncalc_core::matrix::to_spherical;
use mooncalc_core::nutation::NutationObliquity;
use mooncalc_core::refraction::{Atmosphere, apparent_altitude};
use mooncalc_core::transform::equatorial_to_ecliptic;
use mooncalc_core::{Ephemeris, Equatorial, GeoLocation, Instant};
use mooncalc_visibility::CrescentGeometry;
use mooncalc_visibility::circumstances::{self, Outcome, Request, Sighting, geometry_at};
use mooncalc_visibility::criteria::{Criterion, CriterionResult, EvaluationTime};

use super::geometry::{parallactic_angle, zenith_limb_angle};
use super::stars::{StarOfDate, stars_of_date};
use crate::backend::BackendChoice;
use crate::settings::{Crescent, Settings};

/// Atmosphere for apparent altitudes in the views.
pub const ATMOSPHERE: Atmosphere = Atmosphere::STANDARD;

/// One body seen by the observer at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyState {
    /// Apparent (refracted) altitude of the centre, radians.
    pub alt: f64,
    /// Airless topocentric altitude, radians.
    pub alt_airless: f64,
    /// Azimuth from north through east, radians.
    pub az: f64,
    /// Topocentric apparent equatorial coordinates (true equator and
    /// equinox of date); `distance_km` is observer-to-body.
    pub topo: Equatorial,
    /// Topocentric hour angle, radians in `(−π, π]`, west positive.
    pub hour_angle: f64,
}

/// The Sun and Moon for one observer at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    pub jd_ut: f64,
    /// Local apparent sidereal time, radians.
    pub lst: f64,
    pub sun: BodyState,
    pub moon: BodyState,
    /// Geocentric distance of the Moon, km.
    pub moon_geo_distance_km: f64,
    /// Topocentric illumination (elongation, phase angle, fraction,
    /// bright-limb position angle χ from north).
    pub illumination: Illumination,
    /// Parallactic angle of the Moon, radians.
    pub parallactic: f64,
    /// Position angle of the bright limb from the zenith, `χ − q`, radians.
    pub limb_zenith: f64,
    /// Geocentric elongation in ecliptic longitude `λ☾ − λ☉`, degrees in
    /// `[0, 360)`, which names the phase.
    pub elongation_lon_deg: f64,
}

fn body_state<S: PositionSource + ?Sized>(
    src: &S,
    frame: &mooncalc_core::events::LocalFrame,
    body: Body,
    jd_ut: f64,
    atm: &Atmosphere,
) -> BodyState {
    let geo = src.position(body, jd_ut);
    let t = frame.topocentric(&geo);
    let alt_airless = frame.altitude(&t);
    let (ra, dec, distance_km) = to_spherical(&t);
    BodyState {
        alt: apparent_altitude(alt_airless, atm),
        alt_airless,
        az: frame.azimuth(&t),
        topo: Equatorial {
            ra,
            dec,
            distance_km,
        },
        hour_angle: norm_pi(frame.lst - ra),
    }
}

/// The Sun and Moon seen by `obs` at UT Julian Day `jd_ut`.
pub fn snapshot<S: PositionSource + ?Sized>(
    src: &S,
    obs: &Observer,
    jd_ut: f64,
    atm: &Atmosphere,
) -> Snapshot {
    let frame = obs.frame_at(src, jd_ut);
    let sun = body_state(src, &frame, Body::Sun, jd_ut, atm);
    let moon = body_state(src, &frame, Body::Moon, jd_ut, atm);
    let illumination = Illumination::compute(&sun.topo, &moon.topo);
    let parallactic = parallactic_angle(moon.hour_angle, moon.topo.dec, obs.lat);
    let eps = NutationObliquity::at(Instant::from_jd_ut(jd_ut).jd_tt()).eps;
    let sun_geo = src.equatorial(Body::Sun, jd_ut);
    let moon_geo = src.equatorial(Body::Moon, jd_ut);
    let dlon =
        equatorial_to_ecliptic(&moon_geo, eps).lon - equatorial_to_ecliptic(&sun_geo, eps).lon;
    Snapshot {
        jd_ut,
        lst: frame.lst,
        sun,
        moon,
        moon_geo_distance_km: moon_geo.distance_km,
        illumination,
        parallactic,
        limb_zenith: zenith_limb_angle(illumination.bright_limb_angle, parallactic),
        elongation_lon_deg: dlon.to_degrees().rem_euclid(360.0),
    }
}

/// The ephemeris used by the Sky and Trajectory tabs.
///
/// **Single hook.** Until the shared backend (`crate::backend`) hands out
/// its `Arc<dyn Ephemeris>`, this builds the Meeus/ELP ephemeris locally;
/// point it at the shared backend then. The scene keys include
/// `settings.backend.effective()`, so a backend change recomputes them.
pub fn ephemeris(_settings: &Settings) -> std::sync::Arc<dyn Ephemeris> {
    std::sync::Arc::new(mooncalc_core::MeeusEphemeris::new())
}

/// UT of civil noon on local date `date` in a zone `tz_hours` ahead of UT.
pub fn civil_noon(date: CalendarDate, tz_hours: f64) -> f64 {
    date.jd0() + 0.5 - tz_hours / 24.0
}

/// The local calendar date of a UT Julian Day.
pub fn local_date(jd_ut: f64, tz_hours: f64) -> CalendarDate {
    CalendarDate::from_jd(jd_ut + tz_hours / 24.0)
}

/// Inputs that determine a [`Night`].
#[derive(Clone, Debug, PartialEq)]
pub struct NightKey {
    pub date: CalendarDate,
    pub tz_hours: f64,
    pub location: GeoLocation,
    pub crescent: Crescent,
    pub criterion_id: String,
    pub backend: BackendChoice,
}

impl NightKey {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            date: local_date(s.time.jd_ut, s.time.tz_offset_hours),
            tz_hours: s.time.tz_offset_hours,
            location: s.observer(),
            crescent: s.crescent,
            criterion_id: s.criterion_id.clone(),
            backend: s.backend.effective(),
        }
    }
}

/// Spacing of the precomputed path samples, days (2 minutes).
pub const NIGHT_SAMPLE_STEP: f64 = 2.0 / 1440.0;
/// Path samples extend this far beyond the slider window, days.
const NIGHT_PATH_MARGIN: f64 = 1.0 / 24.0;

/// The evening (or morning) of a crescent sighting on one local date.
pub struct Night {
    pub key: NightKey,
    pub table: ApparentTable,
    pub observer: Observer,
    pub outcome: Outcome,
    /// The selected criterion and its verdict at its evaluation time.
    pub criterion: Option<Box<dyn Criterion>>,
    pub verdict: Option<CriterionResult>,
    /// Sunset (evening) or sunrise (morning), UT JD.
    pub sun_event: Option<f64>,
    /// Moonset (evening) or moonrise (morning), UT JD.
    pub moon_event: Option<f64>,
    /// The criterion's evaluation instant ("best time"), UT JD.
    pub evaluation: Option<f64>,
    /// The time-slider span, UT JD: sunset → moonset (evening) or
    /// moonrise → sunrise (morning), with a few minutes either side.
    pub start: f64,
    pub end: f64,
    /// Samples every [`NIGHT_SAMPLE_STEP`] over the slider span plus an
    /// hour either side, for the paths.
    pub samples: Vec<Snapshot>,
    /// Bright stars of date.
    pub stars: Vec<StarOfDate>,
    pub ephemeris_name: String,
}

impl Night {
    pub fn compute(key: NightKey, eph: &dyn Ephemeris) -> Self {
        let noon = civil_noon(key.date, key.tz_hours);
        let evening = key.crescent == Crescent::Evening;
        let criterion = mooncalc_visibility::criteria::by_id(&key.criterion_id);
        let evaluation_rule = criterion
            .as_ref()
            .map_or(EvaluationTime::YallopBestTime, |c| c.evaluation_time());
        let table = ApparentTable::with_default_step(
            eph,
            Instant::from_jd_ut(noon - 2.0),
            Instant::from_jd_ut(noon + 2.0),
        );
        let new_moons = circumstances::new_moons_around(eph, Instant::from_jd_ut(noon));
        let req = Request {
            location: key.location,
            noon: Instant::from_jd_ut(noon),
            sighting: if evening {
                Sighting::Evening
            } else {
                Sighting::Morning
            },
            evaluation: evaluation_rule,
        };
        let outcome = circumstances::compute_with(&table, &new_moons, &req, &Default::default());
        let verdict = criterion
            .as_ref()
            .and_then(|c| outcome.evaluate(c.as_ref()));
        let observer = Observer::new(&key.location);

        let (sun_event, moon_event, evaluation) = match &outcome {
            Outcome::MoonAlwaysUp {
                sun_event,
                circumstances,
            } => (
                Some(sun_event.jd_ut),
                None,
                circumstances.map(|c| c.evaluation.jd_ut),
            ),
            Outcome::MoonAlwaysDown { sun_event }
            | Outcome::SunAltitudeNotReached { sun_event } => (Some(sun_event.jd_ut), None, None),
            Outcome::NoSunEvent(_) => (None, None, None),
            other => {
                let c = other
                    .circumstances()
                    .expect("circumstances for this outcome");
                (
                    Some(c.sun_event.jd_ut),
                    c.moon_event.map(|t| t.jd_ut),
                    Some(c.evaluation.jd_ut),
                )
            }
        };

        const PAD: f64 = 10.0 / 1440.0;
        const DEFAULT_SPAN: f64 = 2.0 / 24.0;
        const MAX_SPAN: f64 = 8.0 / 24.0;
        let (start, end) = match (sun_event, moon_event) {
            (Some(s), Some(m)) if evening && m > s => (s - PAD, s + (m - s).min(MAX_SPAN) + PAD),
            (Some(s), Some(m)) if !evening && m < s => (s - (s - m).min(MAX_SPAN) - PAD, s + PAD),
            (Some(s), _) if evening => (s - PAD, s + DEFAULT_SPAN),
            (Some(s), _) => (s - DEFAULT_SPAN, s + PAD),
            // Polar day or night: the evening or morning hours by the clock.
            (None, _) if evening => (noon + 5.0 / 24.0, noon + 9.0 / 24.0),
            (None, _) => (noon - 9.0 / 24.0, noon - 5.0 / 24.0),
        };

        let (p0, p1) = (start - NIGHT_PATH_MARGIN, end + NIGHT_PATH_MARGIN);
        let n = ((p1 - p0) / NIGHT_SAMPLE_STEP).ceil() as usize;
        let samples = (0..=n)
            .map(|i| {
                snapshot(
                    &table,
                    &observer,
                    p0 + i as f64 * NIGHT_SAMPLE_STEP,
                    &ATMOSPHERE,
                )
            })
            .collect();
        let stars = stars_of_date(Instant::from_jd_ut(noon).jd_tt());

        Self {
            key,
            table,
            observer,
            outcome,
            criterion,
            verdict,
            sun_event,
            moon_event,
            evaluation,
            start,
            end,
            samples,
            stars,
            ephemeris_name: eph.name().to_owned(),
        }
    }

    /// The Sun and Moon at any instant near the night.
    pub fn snapshot(&self, jd_ut: f64) -> Snapshot {
        snapshot(&self.table, &self.observer, jd_ut, &ATMOSPHERE)
    }

    /// The crescent geometry (ARCL, ARCV, DAZ, width ...) at `jd_ut`, with
    /// the lag and the age (from the conjunction nearest the evaluation)
    /// of this night.
    pub fn geometry(&self, jd_ut: f64) -> Option<CrescentGeometry> {
        let c = self.outcome.circumstances()?;
        let dir = if self.key.crescent == Crescent::Evening {
            1.0
        } else {
            -1.0
        };
        let age_hours = dir * (jd_ut - c.conjunction.jd_ut) * 24.0;
        Some(geometry_at(
            &self.table,
            &self.observer,
            jd_ut,
            c.geometry.lag_minutes,
            age_hours,
            &ATMOSPHERE,
        ))
    }

    /// The instant the slider starts at: the evaluation ("best") time,
    /// else the Sun event, else the start of the span.
    pub fn default_time(&self) -> f64 {
        self.evaluation
            .or(self.sun_event)
            .unwrap_or(self.start)
            .clamp(self.start, self.end)
    }
}

/// Inputs that determine a [`Day`].
#[derive(Clone, Debug, PartialEq)]
pub struct DayKey {
    pub date: CalendarDate,
    pub tz_hours: f64,
    pub location: GeoLocation,
    pub backend: BackendChoice,
}

impl DayKey {
    pub fn new(s: &Settings, jd_ut: f64) -> Self {
        Self {
            date: local_date(jd_ut, s.time.tz_offset_hours),
            tz_hours: s.time.tz_offset_hours,
            location: s.observer(),
            backend: s.backend.effective(),
        }
    }
}

/// Spacing of the daily curves, days (5 minutes).
pub const DAY_SAMPLE_STEP: f64 = 5.0 / 1440.0;

/// One local calendar day for the Trajectory tab.
pub struct Day {
    pub key: DayKey,
    pub table: ApparentTable,
    pub observer: Observer,
    /// Local midnight starting the day, UT JD; the day is `[start, start + 1)`.
    pub start: f64,
    /// Samples every [`DAY_SAMPLE_STEP`] over the day, both ends included.
    pub samples: Vec<Snapshot>,
    pub sun: DayEvents,
    pub moon: DayEvents,
    /// Morning start and evening end of civil, nautical and astronomical
    /// twilight.
    pub twilights: [(Option<Instant>, Option<Instant>); 3],
    /// Principal phases from about a month before to two months after.
    pub phases: Vec<(Phase, Instant)>,
    pub ephemeris_name: String,
}

impl Day {
    pub fn compute(key: DayKey, eph: &dyn Ephemeris) -> Self {
        let start = key.date.jd0() - key.tz_hours / 24.0;
        let table = ApparentTable::with_default_step(
            eph,
            Instant::from_jd_ut(start - 1.0),
            Instant::from_jd_ut(start + 2.0),
        );
        let observer = Observer::new(&key.location);
        let n = (1.0 / DAY_SAMPLE_STEP).round() as usize;
        let samples = (0..=n)
            .map(|i| {
                snapshot(
                    &table,
                    &observer,
                    start + i as f64 * DAY_SAMPLE_STEP,
                    &ATMOSPHERE,
                )
            })
            .collect();
        let (t0, t1) = (Instant::from_jd_ut(start), Instant::from_jd_ut(start + 1.0));
        let loc = &key.location;
        let sun = day_events(&table, Body::Sun, loc, Horizon::STANDARD, t0, t1);
        let moon = day_events(&table, Body::Moon, loc, Horizon::STANDARD, t0, t1);
        let twilights = [Twilight::Civil, Twilight::Nautical, Twilight::Astronomical]
            .map(|k| twilight(&table, loc, k, t0, t1));
        let phases = phases_between(eph, t0.add_days(-32.0), t0.add_days(62.0));
        Self {
            key,
            table,
            observer,
            start,
            samples,
            sun,
            moon,
            twilights,
            phases,
            ephemeris_name: eph.name().to_owned(),
        }
    }

    pub fn snapshot(&self, jd_ut: f64) -> Snapshot {
        snapshot(&self.table, &self.observer, jd_ut, &ATMOSPHERE)
    }

    /// The first `phase` strictly after `jd_ut`.
    pub fn next_phase(&self, phase: Phase, jd_ut: f64) -> Option<Instant> {
        self.phases
            .iter()
            .find(|(p, t)| *p == phase && t.jd_ut > jd_ut)
            .map(|(_, t)| *t)
    }

    /// The Moon's age at `jd_ut`: days since the last new Moon.
    pub fn moon_age_days(&self, jd_ut: f64) -> Option<f64> {
        self.phases
            .iter()
            .rev()
            .find(|(p, t)| *p == Phase::New && t.jd_ut <= jd_ut)
            .map(|(_, t)| jd_ut - t.jd_ut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::MeeusEphemeris;
    use mooncalc_core::angle::DEG;

    fn mecca() -> GeoLocation {
        GeoLocation::new(21.4225, 39.8262, 277.0)
    }

    fn night_key(date: CalendarDate, loc: GeoLocation, tz: f64) -> NightKey {
        NightKey {
            date,
            tz_hours: tz,
            location: loc,
            crescent: Crescent::Evening,
            criterion_id: "yallop".into(),
            backend: BackendChoice::Meeus,
        }
    }

    #[test]
    fn young_crescent_evening_at_mecca() {
        // New Moon 2024 March 10, 09:00 UT. On the evening of March 11 at
        // Mecca the ~1.5-day crescent sets well after the Sun.
        let eph = MeeusEphemeris::new();
        let night = Night::compute(
            night_key(CalendarDate::new(2024, 3, 11), mecca(), 3.0),
            &eph,
        );
        let (ss, ms) = (night.sun_event.unwrap(), night.moon_event.unwrap());
        // Sunset about 18:35 local (15:35 UT).
        let local = (ss + 3.0 / 24.0 + 0.5).fract() * 24.0;
        assert!((18.3..18.9).contains(&local), "sunset {local} h");
        assert!(ms > ss + 1.0 / 24.0, "lag {} min", (ms - ss) * 1440.0);
        assert!(night.start < ss && night.end > ms);
        assert!(night.samples.len() > 60);
        let t = night.default_time();
        let snap = night.snapshot(t);
        // Thin evening crescent above the western horizon, Sun below.
        assert!(
            snap.illumination.fraction < 0.06,
            "{}",
            snap.illumination.fraction
        );
        assert!(snap.moon.alt > 0.0 && snap.sun.alt < 0.0);
        assert!((240.0..300.0).contains(&(snap.moon.az / DEG)));
        // The lit limb points down toward the Sun: χ − q near 180° (the
        // Sun is below and slightly to one side).
        let zeta = snap.limb_zenith / DEG;
        assert!((120.0..240.0).contains(&zeta), "χ−q = {zeta}");
        // And the geometry agrees with the circumstances at their instant.
        let g = night.geometry(night.evaluation.unwrap()).unwrap();
        let g0 = night.outcome.geometry().unwrap();
        assert!((g.arcv_deg - g0.arcv_deg).abs() < 1e-9);
        assert!((g.age_hours - g0.age_hours).abs() < 1e-6);
        assert!(snap.elongation_lon_deg < 40.0);
    }

    #[test]
    fn morning_crescent_span_precedes_sunrise() {
        // Old crescent on the morning of 2024 March 8 at Mecca.
        let eph = MeeusEphemeris::new();
        let mut key = night_key(CalendarDate::new(2024, 3, 8), mecca(), 3.0);
        key.crescent = Crescent::Morning;
        let night = Night::compute(key, &eph);
        let (sr, mr) = (night.sun_event.unwrap(), night.moon_event.unwrap());
        assert!(mr < sr);
        assert!(night.start < mr && night.end > sr);
        let snap = night.snapshot(night.default_time());
        assert!((60.0..130.0).contains(&(snap.moon.az / DEG)));
        assert!(snap.elongation_lon_deg > 320.0);
    }

    #[test]
    fn day_events_in_london() {
        let eph = MeeusEphemeris::new();
        let key = DayKey {
            date: CalendarDate::new(2024, 3, 11),
            tz_hours: 0.0,
            location: GeoLocation::new(51.5074, -0.1278, 11.0),
            backend: BackendChoice::Meeus,
        };
        let day = Day::compute(key, &eph);
        assert_eq!(day.samples.len(), 289);
        // Sunrise 06:20, sunset 18:00 UT (HMNAO/timeanddate: 06:19, 18:00).
        let h = |t: Instant| (t.jd_ut - day.start) * 24.0;
        assert!((h(day.sun.rise.unwrap()) - 6.33).abs() < 0.1);
        assert!((h(day.sun.set.unwrap()) - 18.0).abs() < 0.1);
        // The young Moon rises in the morning after the Sun and sets after it.
        let mr = h(day.moon.rise.unwrap());
        let ms = h(day.moon.set.unwrap());
        assert!(mr > 6.3 && mr < 9.0, "moonrise {mr}");
        assert!(ms > 19.0 && ms < 23.0, "moonset {ms}");
        // Phases around the day.
        let nm = day.next_phase(Phase::New, day.start).unwrap();
        assert!((nm.jd_ut - day.start - 28.6).abs() < 1.0);
        let age = day.moon_age_days(day.start + 0.5).unwrap();
        assert!((age - 1.13).abs() < 0.1, "{age}");
        let full = day.next_phase(Phase::Full, day.start).unwrap();
        // Full Moon 2024 March 25, 07:00 UT.
        assert!((full.jd_ut - (day.start + 14.29)).abs() < 0.1);
        assert!(day.twilights[0].1.unwrap().jd_ut > day.sun.set.unwrap().jd_ut);
    }
}
