//! Rise, set, transit and twilight of the Sun and Moon.
//!
//! # Conventions
//!
//! A body rises or sets when the topocentric, airless altitude of its centre
//! crosses a target altitude `h0` given by a [`Horizon`]:
//!
//! * [`Horizon::STANDARD`] (USNO and HMNAO convention; Meeus,
//!   *Astronomical Algorithms* 2nd ed., ch. 15): the upper limb on a level
//!   sea horizon with the standard horizontal refraction of **34′**. For the
//!   Sun the conventional semi-diameter of 16′ is used, so
//!   `h0 = −0°50′ = −0.8333°`. For the Moon, `h0 = −34′ − s′`, where `s′` is
//!   the **topocentric** semi-diameter at that instant, and the altitude is
//!   the topocentric one, so the parallax is exact (rather than Meeus'
//!   geocentric `h0 = 0.7275 π − 0°34′`, which it approximates).
//! * The `dip` option lowers the horizon by the dip of the sea horizon for
//!   the observer's elevation, `1.76′ √h(m)` (the Nautical Almanac's dip
//!   table). Elevation always enters the parallax.
//! * [`Horizon::Centre`] puts the centre at a fixed airless altitude, used
//!   for twilight (Sun at −6°, −12°, −18°) and for "Sun at altitude x"
//!   evaluation times.
//!
//! Transits are the instants when the geocentric hour angle is 0 (upper) or
//! 180° (lower). At the meridian the parallax displacement lies in the
//! meridian plane, so topocentric transits coincide with these.
//!
//! # Method
//!
//! [`Searcher::crossings`] is the robust search: it samples the altitude
//! margin at most an hour apart, *including every transit in the window*
//! (where the altitude has its extrema), and refines every sign change with
//! Brent's method to 0.1 s. Including the transits means a body that just
//! grazes the horizon near culmination still yields its rise/set pair, and
//! polar days with no events are reported as such.
//!
//! [`Searcher::crossing_near`] is the fast local solver for when the event
//! is roughly known: hour-angle steps from `cos H0 = (sin h0 − sin φ sin δ)
//! / (cos φ cos δ)` (Meeus eq. 15.1) for global convergence, then Newton
//! steps on the exact topocentric altitude. It returns `None` when the
//! hour-angle relation has no solution (no event near) or the iteration
//! misbehaves, and callers then fall back to the robust search.

use std::f64::consts::{PI, TAU};

use crate::angle::{ARCMIN, DEG, norm_pi};
use crate::coords::GeoLocation;
use crate::time::Instant;
use crate::topocentric::moon_semidiameter;

use super::observer::{Look, Observer};
use super::solve::{brent, sign_changes};
use super::source::{Body, PositionSource};

/// Standard horizontal refraction, arc minutes.
pub const STANDARD_REFRACTION_ARCMIN: f64 = 34.0;
/// Conventional solar semi-diameter for rise/set, arc minutes.
pub const SUN_STANDARD_SEMIDIAMETER_ARCMIN: f64 = 16.0;

/// Sidereal rotation rate, revolutions per UT day (Meeus eq. 12.4).
const SIDEREAL_REV_PER_DAY: f64 = 1.002_737_909_35;
/// Mean rate of the Sun's hour angle, radians per day.
const SUN_HA_RATE: f64 = TAU * (SIDEREAL_REV_PER_DAY - 1.0 / 365.242_19);
/// Mean rate of the Moon's hour angle, radians per day.
const MOON_HA_RATE: f64 = TAU * (SIDEREAL_REV_PER_DAY - 1.0 / 27.321_582);

/// Refinement tolerance of the robust search, days (≈ 0.1 s).
const ROBUST_TOL_DAYS: f64 = 1.0e-6;
/// Maximum sampling interval of the robust search, days.
const SCAN_STEP_DAYS: f64 = 1.0 / 24.0;

/// Mean hour-angle rate of a body, radians per day.
pub fn hour_angle_rate(body: Body) -> f64 {
    match body {
        Body::Sun => SUN_HA_RATE,
        Body::Moon => MOON_HA_RATE,
    }
}

/// Dip of the sea horizon for an elevation in metres, radians:
/// `1.76′ √h` (Nautical Almanac dip table, including terrestrial refraction).
pub fn horizon_dip(elevation_m: f64) -> f64 {
    1.76 * ARCMIN * elevation_m.max(0.0).sqrt()
}

/// What "on the horizon" means for a rise/set search. See the module docs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Horizon {
    /// Upper limb on the horizon with the given refraction; optionally
    /// lowered by the dip for the observer's elevation.
    UpperLimb { refraction_arcmin: f64, dip: bool },
    /// Centre at a fixed topocentric airless altitude, degrees.
    Centre { altitude_deg: f64 },
}

impl Horizon {
    /// Upper limb, 34′ refraction, level horizon.
    pub const STANDARD: Self = Self::UpperLimb {
        refraction_arcmin: STANDARD_REFRACTION_ARCMIN,
        dip: false,
    };

    /// The Sun's centre at a twilight depression.
    pub const fn twilight(kind: Twilight) -> Self {
        Self::Centre {
            altitude_deg: kind.altitude_deg(),
        }
    }

    /// Target topocentric airless altitude of the body's centre (radians),
    /// for a topocentric distance in km and an observer elevation in m.
    #[inline]
    pub fn target(&self, body: Body, distance_topo_km: f64, elevation_m: f64) -> f64 {
        match *self {
            Self::UpperLimb {
                refraction_arcmin,
                dip,
            } => {
                let sd = match body {
                    Body::Sun => SUN_STANDARD_SEMIDIAMETER_ARCMIN * ARCMIN,
                    Body::Moon => moon_semidiameter(distance_topo_km),
                };
                let d = if dip { horizon_dip(elevation_m) } else { 0.0 };
                -(refraction_arcmin * ARCMIN + sd + d)
            }
            Self::Centre { altitude_deg } => altitude_deg * DEG,
        }
    }
}

impl Default for Horizon {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// Twilight kinds: the Sun's centre at −6°, −12° or −18°.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Twilight {
    Civil,
    Nautical,
    Astronomical,
}

impl Twilight {
    pub const fn altitude_deg(self) -> f64 {
        match self {
            Self::Civil => -6.0,
            Self::Nautical => -12.0,
            Self::Astronomical => -18.0,
        }
    }
}

/// Kinds of horizon and meridian events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventKind {
    Rise,
    Set,
    UpperTransit,
    LowerTransit,
}

/// One event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    pub time: Instant,
}

/// A body that neither rises nor sets during a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Circumpolar {
    AlwaysUp,
    AlwaysDown,
}

/// Rise/set/transit search for one body, observer and horizon.
#[derive(Clone, Copy)]
pub struct Searcher<'a, S: PositionSource + ?Sized> {
    pub source: &'a S,
    pub body: Body,
    pub observer: Observer,
    pub horizon: Horizon,
}

impl<'a, S: PositionSource + ?Sized> Searcher<'a, S> {
    pub fn new(source: &'a S, body: Body, location: &GeoLocation, horizon: Horizon) -> Self {
        Self::with_observer(source, body, Observer::new(location), horizon)
    }

    pub fn with_observer(source: &'a S, body: Body, observer: Observer, horizon: Horizon) -> Self {
        Self {
            source,
            body,
            observer,
            horizon,
        }
    }

    /// Topocentric look at the body and its altitude margin above the target
    /// (radians; positive = above the rise/set altitude).
    #[inline]
    pub fn margin(&self, jd_ut: f64) -> (f64, Look) {
        let look = self.observer.look(self.source, self.body, jd_ut);
        let target = self.horizon.target(
            self.body,
            look.distance_km,
            self.observer.location.elevation_m,
        );
        (look.alt - target, look)
    }

    /// The transit nearest to `jd_ut` (within about half a day): upper when
    /// `lower` is false, lower otherwise. Converges to about 0.01 s.
    pub fn transit_near(&self, jd_ut: f64, lower: bool) -> f64 {
        let target = if lower { PI } else { 0.0 };
        let rate = hour_angle_rate(self.body);
        let mut t = jd_ut;
        for _ in 0..12 {
            let frame = self.observer.frame_at(self.source, t);
            let h = frame.hour_angle(&self.source.position(self.body, t));
            let dt = norm_pi(h - target) / rate;
            t -= dt;
            if dt.abs() < 1e-7 {
                break;
            }
        }
        t
    }

    /// All transits in `[t0, t1)`, in time order.
    pub fn transits(&self, t0: f64, t1: f64) -> Vec<Event> {
        let rate = hour_angle_rate(self.body);
        let period = TAU / rate;
        let mut out = Vec::new();
        for (lower, kind) in [
            (false, EventKind::UpperTransit),
            (true, EventKind::LowerTransit),
        ] {
            let target = if lower { PI } else { 0.0 };
            let frame = self.observer.frame_at(self.source, t0);
            let h = frame.hour_angle(&self.source.position(self.body, t0));
            let ahead = (target - h).rem_euclid(TAU) / rate;
            let mut t = self.transit_near(t0 + ahead, lower);
            if t < t0 {
                t = self.transit_near(t + period, lower);
            }
            while t < t1 {
                out.push(Event {
                    kind,
                    time: Instant::from_jd_ut(t),
                });
                let next = self.transit_near(t + period, lower);
                if next <= t + 0.5 * period {
                    break; // cannot happen for the Sun or Moon; guards loops
                }
                t = next;
            }
        }
        out.sort_by(|a, b| a.time.jd_ut.total_cmp(&b.time.jd_ut));
        out
    }

    /// Every rise and set in `[t0, t1]` by the robust search (module docs).
    pub fn crossings(&self, t0: f64, t1: f64) -> Vec<Event> {
        let n = ((t1 - t0) / SCAN_STEP_DAYS).ceil().max(1.0) as usize;
        let mut nodes: Vec<f64> = (0..=n)
            .map(|k| t0 + (t1 - t0) * k as f64 / n as f64)
            .collect();
        nodes.extend(
            self.transits(t0, t1)
                .iter()
                .map(|e| e.time.jd_ut)
                .filter(|&t| t > t0 && t < t1),
        );
        nodes.sort_by(f64::total_cmp);
        nodes.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        sign_changes(|t| self.margin(t).0, &nodes)
            .into_iter()
            .map(|(a, b, fa, fb)| Event {
                kind: if fa < 0.0 {
                    EventKind::Rise
                } else {
                    EventKind::Set
                },
                time: Instant::from_jd_ut(brent(
                    |t| self.margin(t).0,
                    a,
                    b,
                    fa,
                    fb,
                    ROBUST_TOL_DAYS,
                )),
            })
            .collect()
    }

    /// Rises, sets and transits in `[t0, t1)`, in time order.
    pub fn events(&self, t0: f64, t1: f64) -> Vec<Event> {
        let mut ev = self.crossings(t0, t1);
        ev.retain(|e| e.time.jd_ut < t1);
        ev.extend(self.transits(t0, t1));
        ev.sort_by(|a, b| a.time.jd_ut.total_cmp(&b.time.jd_ut));
        ev
    }

    /// Fast local solver: the rise (`kind = Rise`) or set (`kind = Set`)
    /// whose hour-angle phase is nearest to `jd_ut`, i.e. within about half
    /// a day of it. `None` if there is no such event (circumpolar or never
    /// rising near that time) or the iteration fails; use [`Self::crossings`]
    /// then. Converges to about 0.02 s.
    pub fn crossing_near(&self, jd_ut: f64, kind: EventKind) -> Option<f64> {
        let sign = match kind {
            EventKind::Rise => -1.0,
            EventKind::Set => 1.0,
            _ => return None,
        };
        let rate = hour_angle_rate(self.body);
        let (sp, cp) = self.observer.lat.sin_cos();
        let mut t = jd_ut;
        for iter in 0..16 {
            let geo = self.source.position(self.body, t);
            let frame = self.observer.frame_at(self.source, t);
            let look = frame.look(&geo);
            let target = self.horizon.target(
                self.body,
                look.distance_km,
                self.observer.location.elevation_m,
            );
            let f = look.alt - target;
            let (sd, cd) = look.dec.sin_cos();
            let sh = look.hour_angle.sin();
            let dt = if iter < 2 || f.abs() > DEG {
                // Hour-angle step towards the event, with the target altitude
                // expressed geocentrically (adding the parallax in altitude).
                let h0 = target + (frame.altitude(&geo) - look.alt);
                let cos_h0 = (h0.sin() - sp * sd) / (cp * cd);
                if !(-1.0..=1.0).contains(&cos_h0) {
                    return None;
                }
                norm_pi(sign * cos_h0.acos() - look.hour_angle) / rate
            } else {
                // Newton step on the exact topocentric altitude:
                // dh/dt = −(dH/dt) cos φ cos δ sin H / cos h.
                let dhdt = -rate * cp * cd * sh / look.alt.cos();
                if dhdt * sign >= 0.0 || dhdt.abs() < 1e-3 {
                    return None;
                }
                -f / dhdt
            };
            t += dt;
            if (t - jd_ut).abs() > 0.75 {
                return None;
            }
            if dt.abs() < 2e-7 {
                // Accept only a crossing in the requested direction.
                return (sh * sign > 0.0 || cp.abs() < 1e-9).then_some(t);
            }
        }
        None
    }
}

/// The events of one body during one window (normally a local day).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DayEvents {
    /// First rise in the window.
    pub rise: Option<Instant>,
    /// First set in the window.
    pub set: Option<Instant>,
    /// First upper transit in the window.
    pub upper_transit: Option<Instant>,
    /// First lower transit in the window.
    pub lower_transit: Option<Instant>,
    /// Set when the body neither rises nor sets in the window.
    pub circumpolar: Option<Circumpolar>,
}

/// Rise, set and transits of `body` for `location` in `[t0, t1)`, robustly.
///
/// For a local calendar day pass its UT bounds, e.g.
/// [`super::CalendarDate::local_day`]. The Moon misses a rise or a set on
/// about one day a month without being circumpolar; `circumpolar` is set
/// only when there is neither a rise nor a set.
pub fn day_events<S: PositionSource + ?Sized>(
    source: &S,
    body: Body,
    location: &GeoLocation,
    horizon: Horizon,
    t0: Instant,
    t1: Instant,
) -> DayEvents {
    let s = Searcher::new(source, body, location, horizon);
    let mut out = DayEvents::default();
    for e in s.events(t0.jd_ut, t1.jd_ut) {
        let slot = match e.kind {
            EventKind::Rise => &mut out.rise,
            EventKind::Set => &mut out.set,
            EventKind::UpperTransit => &mut out.upper_transit,
            EventKind::LowerTransit => &mut out.lower_transit,
        };
        slot.get_or_insert(e.time);
    }
    if out.rise.is_none() && out.set.is_none() {
        out.circumpolar = Some(if s.margin(t0.jd_ut).0 >= 0.0 {
            Circumpolar::AlwaysUp
        } else {
            Circumpolar::AlwaysDown
        });
    }
    out
}

/// Morning start and evening end of a twilight in `[t0, t1)`: the first
/// instants the Sun's centre rises through / sets through the twilight
/// altitude. `None` where the Sun does not cross it.
pub fn twilight<S: PositionSource + ?Sized>(
    source: &S,
    location: &GeoLocation,
    kind: Twilight,
    t0: Instant,
    t1: Instant,
) -> (Option<Instant>, Option<Instant>) {
    let d = day_events(source, Body::Sun, location, Horizon::twilight(kind), t0, t1);
    (d.rise, d.set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::source::{ApparentTable, DirectSource};
    use crate::meeus::MeeusEphemeris;

    #[test]
    fn standard_altitudes() {
        let h = Horizon::STANDARD;
        assert!((h.target(Body::Sun, 1.5e8, 0.0) / DEG + 0.8333).abs() < 1e-4);
        // Moon at mean distance: s′ ≈ 15.5′, so h0 ≈ −49.5′.
        let m = h.target(Body::Moon, 384_400.0, 0.0) / ARCMIN;
        assert!((m + 34.0 + 15.54).abs() < 0.05, "{m}");
        let dipped = Horizon::UpperLimb {
            refraction_arcmin: 34.0,
            dip: true,
        };
        let d =
            (h.target(Body::Sun, 1.5e8, 100.0) - dipped.target(Body::Sun, 1.5e8, 100.0)) / ARCMIN;
        assert!((d - 17.6).abs() < 1e-9);
        assert_eq!(
            Horizon::twilight(Twilight::Nautical).target(Body::Sun, 1.0, 0.0),
            -12.0 * DEG
        );
    }

    #[test]
    fn fast_solver_agrees_with_robust_search() {
        let eph = MeeusEphemeris::new();
        let t0 = Instant::from_calendar(2024, 3, 10, 0, 0, 0.0);
        let table = ApparentTable::with_default_step(&eph, t0.add_days(-1.0), t0.add_days(3.0));
        let mut checked = 0;
        for lat in (-60..=60).step_by(15) {
            for lon in (-180..180).step_by(45) {
                let loc = GeoLocation::new(lat as f64, lon as f64, 0.0);
                for body in [Body::Sun, Body::Moon] {
                    let s = Searcher::new(&table, body, &loc, Horizon::STANDARD);
                    for e in s.crossings(t0.jd_ut, t0.jd_ut + 1.0) {
                        let guess = e.time.jd_ut + 0.1;
                        let fast = s.crossing_near(guess, e.kind).expect("fast solver");
                        assert!(
                            (fast - e.time.jd_ut).abs() < 0.5 / 86_400.0,
                            "{lat} {lon} {body:?} {:?}: {} s",
                            e.kind,
                            (fast - e.time.jd_ut) * 86_400.0
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 200, "{checked}");
    }

    #[test]
    fn direct_and_table_sources_agree() {
        let eph = MeeusEphemeris::new();
        let t0 = Instant::from_calendar(2025, 8, 5, 0, 0, 0.0);
        let table = ApparentTable::with_default_step(&eph, t0, t0.add_days(1.0));
        let direct = DirectSource::new(&eph);
        let loc = GeoLocation::new(51.4769, -0.0015, 0.0);
        for body in [Body::Sun, Body::Moon] {
            let a = day_events(&table, body, &loc, Horizon::STANDARD, t0, t0.add_days(1.0));
            let b = day_events(&direct, body, &loc, Horizon::STANDARD, t0, t0.add_days(1.0));
            let pairs = [
                (a.rise, b.rise),
                (a.set, b.set),
                (a.upper_transit, b.upper_transit),
            ];
            assert!(pairs.iter().filter(|p| p.0.is_some()).count() >= 2);
            for (x, y) in pairs {
                assert_eq!(x.is_some(), y.is_some());
                if let (Some(x), Some(y)) = (x, y) {
                    assert!((x.jd_ut - y.jd_ut).abs() * 86_400.0 < 0.2);
                }
            }
        }
    }

    #[test]
    fn polar_day_and_night() {
        let eph = MeeusEphemeris::new();
        let direct = DirectSource::new(&eph);
        let tromso = GeoLocation::new(69.6496, 18.9560, 0.0);
        let summer = Instant::from_calendar(2024, 6, 21, 0, 0, 0.0);
        let d = day_events(
            &direct,
            Body::Sun,
            &tromso,
            Horizon::STANDARD,
            summer,
            summer.add_days(1.0),
        );
        assert_eq!(d.circumpolar, Some(Circumpolar::AlwaysUp));
        assert!(d.upper_transit.is_some() && d.lower_transit.is_some());
        let winter = Instant::from_calendar(2024, 12, 21, 0, 0, 0.0);
        let d = day_events(
            &direct,
            Body::Sun,
            &tromso,
            Horizon::STANDARD,
            winter,
            winter.add_days(1.0),
        );
        assert_eq!(d.circumpolar, Some(Circumpolar::AlwaysDown));
        // Civil twilight still happens in the polar night at 69.6° N.
        let (b, e) = twilight(
            &direct,
            &tromso,
            Twilight::Civil,
            winter,
            winter.add_days(1.0),
        );
        assert!(b.is_some() && e.is_some());
        // The fast solver declines where there is no event.
        let s = Searcher::new(&direct, Body::Sun, &tromso, Horizon::STANDARD);
        assert_eq!(s.crossing_near(summer.jd_ut + 0.8, EventKind::Set), None);
    }
}
