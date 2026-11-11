//! Backend-agnostic rows for daily/hourly Sun and Moon ephemeris tables
//! (the GUI's Ephemeris tab). Rows are plain numbers in the crate's units
//! (radians, km, TT/UT Julian Days); formatting is the caller's job.
//!
//! Each quantity comes from an existing building block, which cites its
//! source: apparent places from the [`Ephemeris`]; semi-diameters and
//! horizontal parallax from [`crate::topocentric`] (Meeus ch. 40, 55);
//! elongation, phase angle, illuminated fraction and bright-limb angle from
//! [`crate::illumination`] (ch. 48); librations from [`crate::libration`]
//! (ch. 53); topocentric places and refraction from
//! [`crate::topocentric`], [`crate::transform`] and [`crate::refraction`]
//! (ch. 13, 16, 40). The Moon's age is measured from the geocentric
//! conjunction in apparent ecliptic longitude: the mean new moon of Meeus
//! eq. 49.1 refined by Newton iteration against the ephemeris, or a
//! conjunction instant supplied by the caller.

use crate::angle::{DEG, norm_2pi, norm_pi};
use crate::coords::{Ecliptic, Equatorial, GeoLocation, Horizontal};
use crate::eclipses::{lunation_at, mean_lunation_jde};
use crate::ephemeris::Ephemeris;
use crate::illumination::Illumination;
use crate::libration::{MoonPhysical, physical_ephemeris, topocentric_physical_ephemeris};
use crate::nutation::NutationObliquity;
use crate::refraction::{Atmosphere, apparent_altitude};
use crate::time::Instant;
use crate::topocentric::{
    horizontal_parallax, moon_semidiameter, sun_semidiameter, topocentric_for,
};
use crate::transform::equatorial_to_horizontal;

/// How the Moon's age is computed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum MoonAgeReference {
    /// From the last geocentric conjunction found with the ephemeris.
    #[default]
    Automatic,
    /// From this conjunction instant (TT Julian Day), e.g. one computed
    /// by the lunar-phases module.
    Conjunction(f64),
}

/// Options for table generation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TableOptions {
    /// Observer for the topocentric columns; `None` for geocentric only.
    pub location: Option<GeoLocation>,
    /// Atmosphere for refracted altitudes.
    pub atmosphere: Atmosphere,
    pub moon_age: MoonAgeReference,
}

/// Topocentric columns of one body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopocentricPlace {
    /// Topocentric apparent RA/Dec, distance from the observer.
    pub equatorial: Equatorial,
    /// Airless altitude and azimuth.
    pub horizontal: Horizontal,
    /// Altitude including refraction for the table's atmosphere.
    pub apparent_altitude: f64,
    /// Topocentric semi-diameter, radians.
    pub semidiameter: f64,
}

/// Columns of one body (Sun or Moon).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyRow {
    /// Geocentric apparent RA/Dec and distance (km).
    pub equatorial: Equatorial,
    /// Geocentric apparent ecliptic longitude/latitude (true equinox).
    pub ecliptic: Ecliptic,
    /// Geocentric semi-diameter, radians.
    pub semidiameter: f64,
    /// Equatorial horizontal parallax, radians.
    pub horizontal_parallax: f64,
    pub topocentric: Option<TopocentricPlace>,
}

/// One table row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EphemerisRow {
    pub instant: Instant,
    pub jd_tt: f64,
    pub sun: BodyRow,
    pub moon: BodyRow,
    /// Geocentric elongation, phase angle, illuminated fraction and
    /// bright-limb position angle.
    pub illumination: Illumination,
    /// The same seen by the observer, when a location is given.
    pub topocentric_illumination: Option<Illumination>,
    /// Days since the reference conjunction (TT).
    pub moon_age_days: f64,
    /// The conjunction the age is counted from, TT JD.
    pub conjunction_jd_tt: f64,
    /// Geocentric librations, position angle and the Sun's selenographic
    /// position.
    pub physical: MoonPhysical,
    /// Topocentric librations, when a location is given.
    pub topocentric_physical: Option<MoonPhysical>,
}

/// Geocentric conjunction in apparent ecliptic longitude nearest the mean
/// new moon `k` (Meeus eq. 49.1), by Newton iteration on λ☾ − λ☉.
fn conjunction(eph: &dyn Ephemeris, k: i64) -> f64 {
    let elong = |jd: f64| norm_pi(eph.moon_ecliptic(jd).lon - eph.sun_ecliptic(jd).lon);
    let mut jd = mean_lunation_jde(k as f64);
    for _ in 0..10 {
        let h = 1e-3;
        let rate = (elong(jd + h) - elong(jd - h)) / (2.0 * h);
        let step = elong(jd) / rate;
        jd -= step;
        if step.abs() < 1e-8 {
            break;
        }
    }
    jd
}

/// The last geocentric conjunction at or before `jd_tt` and the next one.
fn bracketing_conjunctions(eph: &dyn Ephemeris, jd_tt: f64) -> (f64, f64) {
    // The true conjunction is within a day of the mean one, so start one
    // lunation ahead and step back.
    let mut k = lunation_at(jd_tt) + 2;
    let mut next = conjunction(eph, k);
    loop {
        let c = conjunction(eph, k - 1);
        if c <= jd_tt {
            return (c, next);
        }
        next = c;
        k -= 1;
    }
}

fn body_row(
    eq: Equatorial,
    ecl: Ecliptic,
    semidiameter: fn(f64) -> f64,
    topo: Option<(&GeoLocation, f64, &Atmosphere)>,
) -> BodyRow {
    let topocentric = topo.map(|(loc, lst, atm)| {
        let t = topocentric_for(&eq, loc, lst);
        let horizontal = equatorial_to_horizontal(&t, loc.lat_deg * DEG, lst);
        TopocentricPlace {
            equatorial: t,
            horizontal,
            apparent_altitude: apparent_altitude(horizontal.alt, atm),
            semidiameter: semidiameter(t.distance_km),
        }
    });
    BodyRow {
        equatorial: eq,
        ecliptic: ecl,
        semidiameter: semidiameter(eq.distance_km),
        horizontal_parallax: horizontal_parallax(eq.distance_km),
        topocentric,
    }
}

fn row_with_conjunction(
    eph: &dyn Ephemeris,
    instant: Instant,
    opts: &TableOptions,
    conj: f64,
) -> EphemerisRow {
    let jd_tt = instant.jd_tt();
    let nut = NutationObliquity::at(jd_tt);
    let sun_eq = eph.sun_equatorial(jd_tt);
    let moon_eq = eph.moon_equatorial(jd_tt);
    let lst = opts
        .location
        .map(|loc| norm_2pi(instant.gast_with(&nut) + loc.lon_deg * DEG));
    let topo = opts
        .location
        .as_ref()
        .zip(lst)
        .map(|(loc, lst)| (loc, lst, &opts.atmosphere));
    let sun = body_row(sun_eq, eph.sun_ecliptic(jd_tt), sun_semidiameter, topo);
    let moon = body_row(moon_eq, eph.moon_ecliptic(jd_tt), moon_semidiameter, topo);
    let topocentric_illumination = sun
        .topocentric
        .zip(moon.topocentric)
        .map(|(s, m)| Illumination::compute(&s.equatorial, &m.equatorial));
    EphemerisRow {
        instant,
        jd_tt,
        sun,
        moon,
        illumination: Illumination::compute(&sun_eq, &moon_eq),
        topocentric_illumination,
        moon_age_days: jd_tt - conj,
        conjunction_jd_tt: conj,
        physical: physical_ephemeris(eph, jd_tt),
        topocentric_physical: opts
            .location
            .map(|loc| topocentric_physical_ephemeris(eph, instant, &loc)),
    }
}

/// One row at `instant`.
pub fn ephemeris_row(eph: &dyn Ephemeris, instant: Instant, opts: &TableOptions) -> EphemerisRow {
    let conj = match opts.moon_age {
        MoonAgeReference::Conjunction(c) => c,
        MoonAgeReference::Automatic => bracketing_conjunctions(eph, instant.jd_tt()).0,
    };
    row_with_conjunction(eph, instant, opts, conj)
}

/// `count` rows starting at `start`, every `step_days` (e.g. 1.0 for a
/// daily table, 1/24 for hourly). With [`MoonAgeReference::Automatic`]
/// the conjunctions are found once per lunation.
pub fn ephemeris_table(
    eph: &dyn Ephemeris,
    start: Instant,
    step_days: f64,
    count: usize,
    opts: &TableOptions,
) -> Vec<EphemerisRow> {
    let mut span: Option<(f64, f64)> = None;
    (0..count)
        .map(|i| {
            let instant = start.add_days(step_days * i as f64);
            let conj = match opts.moon_age {
                MoonAgeReference::Conjunction(c) => c,
                MoonAgeReference::Automatic => {
                    let jd = instant.jd_tt();
                    match span {
                        Some((a, b)) if jd >= a && jd < b => a,
                        _ => {
                            let s = bracketing_conjunctions(eph, jd);
                            span = Some(s);
                            s.0
                        }
                    }
                }
            };
            row_with_conjunction(eph, instant, opts, conj)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeus::MeeusEphemeris;
    use crate::time::julian_day;

    #[test]
    fn meeus_47a_48a_row() {
        // 1992 April 12, 0h TD: Moon α = 134.688470°, δ = +13.768368°,
        // Δ = 368409.7 km, π = 0.991990°, apparent λ = 133.167265°,
        // β = −3.229126° (Meeus 47.a); k = 0.6786, χ = 285.0° (48.a).
        let eph = MeeusEphemeris::new();
        let row = ephemeris_row(
            &eph,
            Instant::from_jd_tt(2_448_724.5),
            &TableOptions::default(),
        );
        let m = row.moon;
        assert!((m.equatorial.ra / DEG - 134.688_470).abs() < 1e-5);
        assert!((m.equatorial.dec / DEG - 13.768_368).abs() < 1e-5);
        assert!((m.equatorial.distance_km - 368_409.7).abs() < 0.1);
        assert!((m.horizontal_parallax / DEG - 0.991_990).abs() < 2e-6);
        assert!((m.ecliptic.lon / DEG - 133.167_265).abs() < 1e-5);
        assert!((m.ecliptic.lat / DEG + 3.229_126).abs() < 1e-5);
        assert!((row.illumination.fraction - 0.6786).abs() < 5e-5);
        assert!((row.illumination.bright_limb_angle / DEG - 285.0).abs() < 0.05);
        // Meeus 53.a at the same instant.
        assert!((row.physical.libration.lon / DEG + 1.23).abs() < 0.005);
        assert!(m.topocentric.is_none() && row.topocentric_physical.is_none());
        // Waxing gibbous (k = 0.68): the last new moon was on 1992 April 3.
        let c = Instant::from_jd_tt(row.conjunction_jd_tt).to_calendar();
        assert_eq!((c.year, c.month, c.day), (1992, 4, 3));
        assert!(row.moon_age_days > 8.0 && row.moon_age_days < 9.5);
    }

    #[test]
    fn conjunction_matches_nasa() {
        // NASA (Espenak), 2024 April 8: ecliptic conjunction 18:22:00.4 TT,
        // JD 2460409.265282.
        let eph = MeeusEphemeris::new();
        let c = conjunction(&eph, 300);
        assert!((c - 2_460_409.265_282).abs() * 86_400.0 < 20.0, "{c}");
        let (a, b) = bracketing_conjunctions(&eph, julian_day(2024, 4, 20.0));
        assert_eq!(a, c);
        assert!(b - a > 29.0 && b - a < 30.0);
        let (a, _) = bracketing_conjunctions(&eph, c + 1e-6);
        assert_eq!(a, c);
        let (a, _) = bracketing_conjunctions(&eph, c - 1e-6);
        assert!(c - a > 29.0);
    }

    #[test]
    fn hourly_table_with_location() {
        let eph = MeeusEphemeris::new();
        let loc = GeoLocation::new(21.4225, 39.8262, 277.0); // Makkah
        let opts = TableOptions {
            location: Some(loc),
            ..TableOptions::default()
        };
        let start = Instant::from_calendar(2024, 4, 7, 0, 0, 0.0);
        let rows = ephemeris_table(&eph, start, 1.0 / 24.0, 72, &opts);
        assert_eq!(rows.len(), 72);
        // Ages run continuously and restart at the conjunction.
        let conj = conjunction(&eph, 300);
        for r in &rows {
            let single = ephemeris_row(&eph, r.instant, &opts);
            assert_eq!(single.conjunction_jd_tt, r.conjunction_jd_tt);
            assert!(r.moon_age_days >= 0.0);
            assert_eq!(r.jd_tt >= conj, r.conjunction_jd_tt == conj);
            let t = r.moon.topocentric.unwrap();
            // Refraction lifts the altitude; topocentric Moon is lower.
            assert!(t.apparent_altitude >= t.horizontal.alt);
            assert!(r.moon.equatorial.distance_km - t.equatorial.distance_km < 6400.0);
            assert!(t.semidiameter > r.moon.semidiameter * 0.98);
        }
        // Sun near the meridian at local noon: altitude ≈ 90° − |φ − δ☉|.
        let noon = ephemeris_row(&eph, Instant::from_calendar(2024, 4, 7, 9, 21, 0.0), &opts);
        let s = noon.sun.topocentric.unwrap();
        let expect = 90.0 - (loc.lat_deg - noon.sun.equatorial.dec / DEG).abs();
        assert!(
            (s.horizontal.alt / DEG - expect).abs() < 0.3,
            "{}",
            s.horizontal.alt / DEG
        );
        // Fixed reference conjunction.
        let fixed = TableOptions {
            moon_age: MoonAgeReference::Conjunction(conj),
            ..opts
        };
        let r = ephemeris_row(&eph, start, &fixed);
        assert!((r.moon_age_days - (r.jd_tt - conj)).abs() < 1e-12 && r.moon_age_days < 0.0);
    }
}
