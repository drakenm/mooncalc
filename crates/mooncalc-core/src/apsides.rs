//! Perigee and apogee of the Moon.
//!
//! * [`apsis_meeus`]: Meeus, *Astronomical Algorithms* 2nd ed., ch. 50 —
//!   the mean apsis (eq. 50.1) corrected by the periodic terms of Table 50.A
//!   (time) and Table 50.B (equatorial horizontal parallax). Meeus quotes
//!   maximum errors against ELP-2000/82 of 31 min (perigee) and 3 min
//!   (apogee) in time, 0.124″ and 0.051″ in parallax (≈ 12 km and 6 km).
//! * [`refine_apsis`]: an ephemeris-agnostic refinement that minimises or
//!   maximises the geocentric distance returned by any [`Ephemeris`]
//!   (golden-section search, see `search`), starting from the ch. 50 value.

use crate::angle::{ARCSEC, DEG};
use crate::ephemeris::Ephemeris;
use crate::search;

/// Earth equatorial radius used by Meeus ch. 50 to turn parallax into
/// distance, km.
const MEEUS_EARTH_RADIUS_KM: f64 = 6378.14;

/// Which apsis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ApsisKind {
    Perigee,
    Apogee,
}

/// A lunar apsis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Apsis {
    pub kind: ApsisKind,
    /// Instant, TT Julian Day.
    pub jd_tt: f64,
    /// Geocentric distance of the Moon's centre, km.
    pub distance_km: f64,
    /// Equatorial horizontal parallax, radians.
    pub parallax: f64,
}

/// One periodic term: coefficient, its T rate, and multiples of D, M, F.
type Term = (f64, f64, i8, i8, i8);

/// Meeus Table 50.A, perigee: sine terms in days.
#[rustfmt::skip]
const PERIGEE_TIME: &[Term] = &[
    (-1.6769, 0.0, 2, 0, 0), (0.4589, 0.0, 4, 0, 0), (-0.1856, 0.0, 6, 0, 0),
    (0.0883, 0.0, 8, 0, 0), (-0.0773, 0.00019, 2, -1, 0), (0.0502, -0.00013, 0, 1, 0),
    (-0.0460, 0.0, 10, 0, 0), (0.0422, -0.00011, 4, -1, 0), (-0.0256, 0.0, 6, -1, 0),
    (0.0253, 0.0, 12, 0, 0), (0.0237, 0.0, 1, 0, 0), (0.0162, 0.0, 8, -1, 0),
    (-0.0145, 0.0, 14, 0, 0), (0.0129, 0.0, 0, 0, 2), (-0.0112, 0.0, 3, 0, 0),
    (-0.0104, 0.0, 10, -1, 0), (0.0086, 0.0, 16, 0, 0), (0.0069, 0.0, 12, -1, 0),
    (0.0066, 0.0, 5, 0, 0), (-0.0053, 0.0, 2, 0, 2), (-0.0052, 0.0, 18, 0, 0),
    (-0.0046, 0.0, 14, -1, 0), (-0.0041, 0.0, 7, 0, 0), (0.0040, 0.0, 2, 1, 0),
    (0.0032, 0.0, 20, 0, 0), (-0.0032, 0.0, 1, 1, 0), (0.0031, 0.0, 16, -1, 0),
    (-0.0029, 0.0, 4, 1, 0), (0.0027, 0.0, 9, 0, 0), (0.0027, 0.0, 4, 0, 2),
    (-0.0027, 0.0, 2, -2, 0), (0.0024, 0.0, 4, -2, 0), (-0.0021, 0.0, 6, -2, 0),
    (-0.0021, 0.0, 22, 0, 0), (-0.0021, 0.0, 18, -1, 0), (0.0019, 0.0, 6, 1, 0),
    (-0.0018, 0.0, 11, 0, 0), (-0.0014, 0.0, 8, 1, 0), (-0.0014, 0.0, 4, 0, -2),
    (-0.0014, 0.0, 6, 0, 2), (0.0014, 0.0, 3, 1, 0), (-0.0014, 0.0, 5, 1, 0),
    (0.0013, 0.0, 13, 0, 0), (0.0013, 0.0, 20, -1, 0), (0.0011, 0.0, 3, 2, 0),
    (-0.0011, 0.0, 4, -2, 2), (-0.0010, 0.0, 1, 2, 0), (-0.0009, 0.0, 22, -1, 0),
];

/// Continuation of Table 50.A, perigee (smallest terms).
#[rustfmt::skip]
const PERIGEE_TIME_SMALL: &[Term] = &[
    (-0.0008, 0.0, 0, 0, 4), (0.0008, 0.0, 6, 0, -2), (0.0008, 0.0, 2, 1, -2),
    (0.0007, 0.0, 0, 2, 0), (0.0007, 0.0, 0, -1, 2), (0.0007, 0.0, 2, 0, 4),
    (-0.0006, 0.0, 0, -2, 2), (-0.0006, 0.0, 2, 2, -2), (0.0006, 0.0, 24, 0, 0),
    (0.0005, 0.0, 4, 0, -4), (0.0005, 0.0, 2, 2, 0), (-0.0004, 0.0, 1, -1, 0),
];

/// Meeus Table 50.A, apogee: sine terms in days.
#[rustfmt::skip]
const APOGEE_TIME: &[Term] = &[
    (0.4392, 0.0, 2, 0, 0), (0.0684, 0.0, 4, 0, 0), (0.0456, -0.00011, 0, 1, 0),
    (0.0426, -0.00011, 2, -1, 0), (0.0212, 0.0, 0, 0, 2), (-0.0189, 0.0, 1, 0, 0),
    (0.0144, 0.0, 6, 0, 0), (0.0113, 0.0, 4, -1, 0), (0.0047, 0.0, 2, 0, 2),
    (0.0036, 0.0, 1, 1, 0), (0.0035, 0.0, 8, 0, 0), (0.0034, 0.0, 6, -1, 0),
    (-0.0034, 0.0, 2, 0, -2), (0.0022, 0.0, 2, -2, 0), (-0.0017, 0.0, 3, 0, 0),
    (0.0013, 0.0, 4, 0, 2), (0.0011, 0.0, 8, -1, 0), (0.0010, 0.0, 4, -2, 0),
    (0.0009, 0.0, 10, 0, 0), (0.0007, 0.0, 3, 1, 0), (0.0006, 0.0, 0, 2, 0),
    (0.0005, 0.0, 2, 1, 0), (0.0005, 0.0, 2, 2, 0), (0.0004, 0.0, 6, 0, 2),
    (0.0004, 0.0, 6, -2, 0), (0.0004, 0.0, 10, -1, 0), (-0.0004, 0.0, 5, 0, 0),
    (-0.0004, 0.0, 4, 0, -2), (0.0003, 0.0, 0, 1, 2), (0.0003, 0.0, 12, 0, 0),
    (0.0003, 0.0, 2, -1, 2), (-0.0003, 0.0, 1, -1, 0),
];

/// Meeus Table 50.B, perigee: cosine terms of the parallax in arcseconds
/// (constant term 3629.215″).
#[rustfmt::skip]
const PERIGEE_PARALLAX: &[Term] = &[
    (63.224, 0.0, 2, 0, 0), (-6.990, 0.0, 4, 0, 0), (2.834, -0.0071, 2, -1, 0),
    (1.927, 0.0, 6, 0, 0), (-1.263, 0.0, 1, 0, 0), (-0.702, 0.0, 8, 0, 0),
    (0.696, -0.0017, 0, 1, 0), (-0.690, 0.0, 0, 0, 2), (-0.629, 0.0016, 4, -1, 0),
    (-0.392, 0.0, 2, 0, -2), (0.297, 0.0, 10, 0, 0), (0.260, 0.0, 6, -1, 0),
    (0.201, 0.0, 3, 0, 0), (-0.161, 0.0, 2, 1, 0), (0.157, 0.0, 1, 1, 0),
    (-0.138, 0.0, 12, 0, 0), (-0.127, 0.0, 8, -1, 0), (0.104, 0.0, 2, 0, 2),
    (0.104, 0.0, 2, -2, 0), (-0.079, 0.0, 5, 0, 0), (0.068, 0.0, 14, 0, 0),
    (0.067, 0.0, 10, -1, 0), (0.054, 0.0, 4, 1, 0), (-0.038, 0.0, 12, -1, 0),
    (-0.038, 0.0, 4, -2, 0), (0.037, 0.0, 7, 0, 0), (-0.037, 0.0, 4, 0, 2),
    (-0.035, 0.0, 16, 0, 0), (-0.030, 0.0, 3, 1, 0), (0.029, 0.0, 1, -1, 0),
    (-0.025, 0.0, 6, 1, 0), (0.023, 0.0, 0, 2, 0), (0.023, 0.0, 14, -1, 0),
    (-0.023, 0.0, 2, 2, 0), (0.022, 0.0, 6, -2, 0), (-0.021, 0.0, 2, -1, -2),
    (-0.020, 0.0, 9, 0, 0), (0.019, 0.0, 18, 0, 0), (0.017, 0.0, 6, 0, 2),
    (0.014, 0.0, 0, -1, 2), (-0.014, 0.0, 16, -1, 0), (0.013, 0.0, 4, 0, -2),
    (0.012, 0.0, 8, 1, 0), (0.011, 0.0, 11, 0, 0), (0.010, 0.0, 5, 1, 0),
    (-0.010, 0.0, 20, 0, 0),
];

/// Meeus Table 50.B, apogee: cosine terms of the parallax in arcseconds
/// (constant term 3245.251″).
#[rustfmt::skip]
const APOGEE_PARALLAX: &[Term] = &[
    (-9.147, 0.0, 2, 0, 0), (-0.841, 0.0, 1, 0, 0), (0.697, 0.0, 0, 0, 2),
    (-0.656, 0.0016, 0, 1, 0), (0.355, 0.0, 4, 0, 0), (0.159, 0.0, 2, -1, 0),
    (0.127, 0.0, 1, 1, 0), (0.065, 0.0, 4, -1, 0), (0.052, 0.0, 6, 0, 0),
    (0.043, 0.0, 2, 1, 0), (0.031, 0.0, 2, 0, 2), (-0.023, 0.0, 2, 0, -2),
    (0.022, 0.0, 2, -2, 0), (0.019, 0.0, 2, 2, 0), (-0.016, 0.0, 0, 2, 0),
    (0.014, 0.0, 6, -1, 0), (0.010, 0.0, 8, 0, 0),
];

fn sum_terms(terms: &[Term], t: f64, d: f64, m: f64, f: f64, trig: fn(f64) -> f64) -> f64 {
    terms
        .iter()
        .map(|&(c, ct, nd, nm, nf)| {
            (c + ct * t) * trig(nd as f64 * d + nm as f64 * m + nf as f64 * f)
        })
        .sum()
}

/// Mean apsis JDE for lunation-of-anomaly number `k` (Meeus eq. 50.1).
pub fn mean_apsis_jde(k: f64) -> f64 {
    let t = k / 1325.55;
    2_451_534.669_8
        + 27.554_549_89 * k
        + t * t * (-0.000_669_1 + t * (-0.000_001_098 + t * 0.000_000_005_2))
}

/// The apsis number nearest a TT Julian Day: an integer for a perigee, an
/// integer + 0.5 for an apogee (Meeus eq. 50.2, k = 0 at the perigee of
/// 1999 December 22).
pub fn apsis_number_near(jd_tt: f64, kind: ApsisKind) -> f64 {
    let k = (jd_tt - 2_451_534.669_8) / 27.554_549_89;
    match kind {
        ApsisKind::Perigee => k.round(),
        ApsisKind::Apogee => (k - 0.5).round() + 0.5,
    }
}

/// Apsis number `k` by Meeus ch. 50 (integer `k`: perigee; `k + 0.5`:
/// apogee). Time from eq. 50.1 + Table 50.A, parallax from Table 50.B,
/// distance `6378.14 km / sin π`.
pub fn apsis_meeus(k: f64) -> Apsis {
    let t = k / 1325.55;
    let d = (171.917_9
        + 335.910_604_6 * k
        + t * t * (-0.010_038_3 + t * (-0.000_011_56 + t * 0.000_000_055)))
        * DEG;
    let m = (347.347_7 + 27.157_772_1 * k + t * t * (-0.000_813 - t * 0.000_001)) * DEG;
    let f = (316.610_9 + 364.528_791_1 * k + t * t * (-0.012_505_3 - t * 0.000_014_8)) * DEG;
    let perigee = k.rem_euclid(1.0) < 0.25;
    let (kind, dt, par_arcsec) = if perigee {
        (
            ApsisKind::Perigee,
            sum_terms(PERIGEE_TIME, t, d, m, f, f64::sin)
                + sum_terms(PERIGEE_TIME_SMALL, t, d, m, f, f64::sin),
            3629.215 + sum_terms(PERIGEE_PARALLAX, t, d, m, f, f64::cos),
        )
    } else {
        (
            ApsisKind::Apogee,
            sum_terms(APOGEE_TIME, t, d, m, f, f64::sin),
            3245.251 + sum_terms(APOGEE_PARALLAX, t, d, m, f, f64::cos),
        )
    };
    let parallax = par_arcsec * ARCSEC;
    Apsis {
        kind,
        jd_tt: mean_apsis_jde(k) + dt,
        distance_km: MEEUS_EARTH_RADIUS_KM / parallax.sin(),
        parallax,
    }
}

/// Refine an apsis with an ephemeris: the extremum of the Moon's
/// geocentric distance within ±1 day of `approx`, to ~0.1 s. The parallax
/// is recomputed as `asin(6378.14 km / Δ)`.
pub fn refine_apsis(eph: &dyn Ephemeris, approx: &Apsis) -> Apsis {
    let dist = |jd: f64| eph.moon_ecliptic(jd).distance_km;
    let (a, b, tol) = (approx.jd_tt - 1.0, approx.jd_tt + 1.0, 1e-6);
    let (jd_tt, distance_km) = match approx.kind {
        ApsisKind::Perigee => search::minimize(dist, a, b, tol),
        ApsisKind::Apogee => search::maximize(dist, a, b, tol),
    };
    Apsis {
        kind: approx.kind,
        jd_tt,
        distance_km,
        parallax: (MEEUS_EARTH_RADIUS_KM / distance_km).asin(),
    }
}

/// Compute apsis `k` by Meeus ch. 50 and, if an ephemeris is given,
/// refine it.
fn apsis(eph: Option<&dyn Ephemeris>, k: f64) -> Apsis {
    let a = apsis_meeus(k);
    match eph {
        Some(e) => refine_apsis(e, &a),
        None => a,
    }
}

/// The first apsis strictly after `jd_tt` (of `kind`, or of either kind if
/// `None`). With `eph`, instants and distances are refined against it;
/// without, they are Meeus ch. 50 values.
pub fn next_apsis(eph: Option<&dyn Ephemeris>, jd_tt: f64, kind: Option<ApsisKind>) -> Apsis {
    let step = if kind.is_some() { 1.0 } else { 0.5 };
    let start = kind.unwrap_or(ApsisKind::Perigee);
    let mut k = apsis_number_near(jd_tt, start) - 2.0 * step;
    loop {
        let a = apsis(eph, k);
        if a.jd_tt > jd_tt {
            return a;
        }
        k += step;
    }
}

/// The last apsis strictly before `jd_tt` (see [`next_apsis`]).
pub fn previous_apsis(eph: Option<&dyn Ephemeris>, jd_tt: f64, kind: Option<ApsisKind>) -> Apsis {
    let step = if kind.is_some() { 1.0 } else { 0.5 };
    let start = kind.unwrap_or(ApsisKind::Perigee);
    let mut k = apsis_number_near(jd_tt, start) + 2.0 * step;
    loop {
        let a = apsis(eph, k);
        if a.jd_tt < jd_tt {
            return a;
        }
        k -= step;
    }
}

/// Every apsis (both kinds, chronological) with `jd_from <= jd_tt < jd_to`.
pub fn apsides_between(eph: Option<&dyn Ephemeris>, jd_from: f64, jd_to: f64) -> Vec<Apsis> {
    let mut out = Vec::new();
    let mut k = apsis_number_near(jd_from, ApsisKind::Perigee) - 1.0;
    let k_end = apsis_number_near(jd_to, ApsisKind::Perigee) + 1.0;
    while k <= k_end {
        let a = apsis(eph, k);
        if a.jd_tt >= jd_from && a.jd_tt < jd_to {
            out.push(a);
        }
        k += 0.5;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeus::MeeusEphemeris;
    use crate::time::julian_day;

    #[test]
    fn meeus_50a() {
        // Apogee of 1988 October: k = −148.5, mean JDE = 2447442.8191,
        // JDE = 2447442.3543 (October 7, 20h30m TD), π = 3240.679″.
        assert_eq!(
            apsis_number_near(julian_day(1988, 10, 1.0), ApsisKind::Apogee),
            -148.5
        );
        assert!((mean_apsis_jde(-148.5) - 2_447_442.819_1).abs() < 1e-4);
        let a = apsis_meeus(-148.5);
        assert_eq!(a.kind, ApsisKind::Apogee);
        assert!((a.jd_tt - 2_447_442.354_3).abs() < 1e-4, "{}", a.jd_tt);
        assert!(
            (a.parallax / ARCSEC - 3240.679).abs() < 1e-3,
            "{}",
            a.parallax / ARCSEC
        );
        assert!((a.distance_km - 6378.14 / (3240.679 * ARCSEC).sin()).abs() < 0.1);
    }

    #[test]
    fn meeus_extreme_perigees() {
        // Meeus ch. 50: perigees of 1997 Dec 9 16.9h, 1998 Jan 3 8.5h,
        // 1990 Dec 2 10.8h, 1990 Dec 30 23.8h (TD); Meeus' max error 31 min.
        let cases = [
            (1997, 12, 9.0 + 16.9 / 24.0),
            (1998, 1, 3.0 + 8.5 / 24.0),
            (1990, 12, 2.0 + 10.8 / 24.0),
            (1990, 12, 30.0 + 23.8 / 24.0),
        ];
        let eph = MeeusEphemeris::new();
        for (y, m, d) in cases {
            let expected = julian_day(y, m, d);
            let k = apsis_number_near(expected, ApsisKind::Perigee);
            let a = apsis_meeus(k);
            assert!(
                (a.jd_tt - expected).abs() < 31.0 / 1440.0,
                "{y}-{m}: {}",
                a.jd_tt
            );
            let r = refine_apsis(&eph, &a);
            // These listed instants are given to 0.1 h and agree with the
            // ch. 50 series to within that rounding; the truncated-ELP
            // refinement lands within ~6 min of them.
            assert!(
                (r.jd_tt - expected).abs() < 10.0 / 1440.0,
                "{y}-{m}: {}",
                r.jd_tt
            );
        }
    }

    #[test]
    fn refinement_agrees_with_meeus_tables() {
        // Over 1990–2030 the refined instants stay within Meeus' stated
        // errors (31 min perigee, 3 min apogee, plus slack for the truncated
        // ELP), and the distances within ~15 km.
        let eph = MeeusEphemeris::new();
        let list = apsides_between(None, 2_447_892.5, 2_462_502.5);
        assert!(list.len() > 1050);
        for a in list {
            let r = refine_apsis(&eph, &a);
            let dt_min = (r.jd_tt - a.jd_tt).abs() * 1440.0;
            let limit = if a.kind == ApsisKind::Perigee {
                35.0
            } else {
                6.0
            };
            assert!(dt_min < limit, "{:?} {}: {dt_min} min", a.kind, a.jd_tt);
            assert!(
                (r.distance_km - a.distance_km).abs() < 15.0,
                "{:?} {}",
                a.kind,
                a.jd_tt
            );
        }
    }

    #[test]
    fn search_api() {
        let eph = MeeusEphemeris::new();
        let jd = julian_day(2024, 1, 1.0);
        let n = next_apsis(Some(&eph), jd, None);
        let p = previous_apsis(Some(&eph), jd, None);
        assert!(p.jd_tt < jd && n.jd_tt > jd && n.jd_tt - p.jd_tt < 16.0);
        assert_ne!(n.kind, p.kind);
        let np = next_apsis(None, jd, Some(ApsisKind::Perigee));
        assert_eq!(np.kind, ApsisKind::Perigee);
        assert!(np.jd_tt > jd && np.jd_tt - jd < 28.0);
        let r = refine_apsis(&eph, &np);
        assert!(r.distance_km > 356_000.0 && r.distance_km < 370_500.0);
        let year = apsides_between(
            Some(&eph),
            julian_day(2024, 1, 1.0),
            julian_day(2025, 1, 1.0),
        );
        assert!(year.len() == 26 || year.len() == 27);
        assert!(
            year.windows(2)
                .all(|w| w[0].kind != w[1].kind && w[1].jd_tt > w[0].jd_tt)
        );
    }
}
