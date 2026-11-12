//! The Sun from VSOP87D (Meeus, *Astronomical Algorithms* 2nd ed., ch. 25
//! "higher accuracy" method and ch. 32).
//!
//! The heliocentric ecliptic coordinates of the Earth (L, B, R) come from
//! the VSOP87D series (Bretagnon & Francou 1988), referred to the dynamical
//! ecliptic and equinox of date. The geocentric Sun is Θ = L + 180°,
//! β = −B; then the conversion to the FK5 system (Meeus eq. 32.3), nutation
//! in longitude, and annual aberration −20.4898″ / R (which with the use of
//! geometric coordinates also accounts for light time, Meeus ch. 25).

use super::vsop87d_earth as v;
use crate::angle::{ARCSEC, DEG, norm_2pi};
use crate::coords::Ecliptic;
use crate::nutation::NutationObliquity;
use crate::time::{DAYS_PER_CENTURY, J2000};
use crate::topocentric::AU_KM;

/// How much of the VSOP87D Earth series to evaluate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SunSeries {
    /// Skip every term whose contribution `A · |τ|^k` is below
    /// [`TRUNCATION_THRESHOLD`] at the requested date (the series are sorted
    /// by amplitude, so summation simply stops early). Near the present
    /// this evaluates about 230 of the 2425 terms and agrees with the full
    /// series to within 0.12″ in L and B and 10⁻⁶ AU in R over 1000–3000
    /// (see tests).
    #[default]
    Truncated,
    /// The complete VSOP87D series (2425 terms), accurate to ~0.01″
    /// near J2000.
    Full,
}

/// Amplitude threshold (radians or AU) of [`SunSeries::Truncated`].
pub const TRUNCATION_THRESHOLD: f64 = 5e-8;

/// Heliocentric ecliptic position of the Earth (VSOP87D, dynamical ecliptic
/// and equinox of date): `(L, B, R)` with L in `[0, 2π)`, B in radians and
/// R in AU.
pub fn earth_heliocentric(jd_tt: f64, series: SunSeries) -> (f64, f64, f64) {
    let tau = (jd_tt - J2000) / 365_250.0;
    let th = match series {
        SunSeries::Truncated => TRUNCATION_THRESHOLD,
        SunSeries::Full => 0.0,
    };
    let l = combine(tau, th, &[&v::L0, &v::L1, &v::L2, &v::L3, &v::L4, &v::L5]);
    let b = combine(tau, th, &[&v::B0, &v::B1, &v::B2, &v::B3, &v::B4]);
    let r = combine(tau, th, &[&v::R0, &v::R1, &v::R2, &v::R3, &v::R4, &v::R5]);
    (norm_2pi(l), b, r)
}

/// `Σ_k τ^k Σ_i A cos(B + C τ)` (Meeus eq. 32.2 generalised), stopping each
/// amplitude-sorted series once `A · |τ|^k < threshold`.
#[inline]
fn combine(tau: f64, threshold: f64, series: &[&[[f64; 3]]]) -> f64 {
    let mut total = 0.0;
    let mut weight = 1.0;
    let mut tau_k = 1.0;
    for s in series {
        let mut sum = 0.0;
        for t in s.iter() {
            if t[0] * weight < threshold {
                break;
            }
            sum += t[0] * (t[1] + t[2] * tau).cos();
        }
        total += sum * tau_k;
        tau_k *= tau;
        weight *= tau.abs();
    }
    total
}

/// Geometric geocentric ecliptic coordinates of the Sun referred to the
/// FK5 system and the mean equinox of date (no nutation, no aberration):
/// `(λ, β, R in AU)`. Meeus ch. 25 (higher accuracy) and eq. 32.3.
pub fn sun_geometric(jd_tt: f64, series: SunSeries) -> (f64, f64, f64) {
    let (l, b, r) = earth_heliocentric(jd_tt, series);
    let theta = l + std::f64::consts::PI;
    let beta = -b;
    let t = (jd_tt - J2000) / DAYS_PER_CENTURY;
    // Conversion from the VSOP dynamical frame to FK5 (Meeus eq. 32.3).
    let lp = theta - (1.397 * t + 0.000_31 * t * t) * DEG;
    let (slp, clp) = lp.sin_cos();
    let dtheta = -0.090_33 * ARCSEC;
    let dbeta = 0.039_16 * ARCSEC * (clp - slp);
    (norm_2pi(theta + dtheta), beta + dbeta, r)
}

/// Apparent geocentric ecliptic coordinates of the Sun, true equinox of
/// date, distance in km (Meeus ch. 25).
pub fn sun_apparent_ecliptic(jd_tt: f64, series: SunSeries, nut: &NutationObliquity) -> Ecliptic {
    let (lon, lat, r) = sun_geometric(jd_tt, series);
    let aberration = -20.4898 * ARCSEC / r;
    Ecliptic {
        lon: norm_2pi(lon + nut.dpsi + aberration),
        lat,
        distance_km: r * AU_KM,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::ecliptic_to_equatorial;

    #[test]
    fn meeus_25b_earth_heliocentric() {
        // Meeus example 25.b (1992 October 13.0 TD). From the book's truncated
        // tables: L = −43.63484796 rad (= 19.907372°), B = −0.00000312 rad,
        // R = 0.99760775 AU; the complete VSOP87 gives R = 0.99760853 AU.
        // The book's truncation is coarser than ours (64 terms in L0, 5 in
        // B0), so L and B are compared at the level of its truncation error.
        let jd = 2_448_908.5;
        for series in [SunSeries::Truncated, SunSeries::Full] {
            let (l, b, r) = earth_heliocentric(jd, series);
            assert!(
                (l / DEG - 19.907_372).abs() < 0.5 / 3600.0,
                "{series:?} {}",
                l / DEG
            );
            assert!((b + 0.000_003_12).abs() < 6e-7, "{series:?} {b}");
            assert!((r - 0.997_608_53).abs() < 1e-7, "{series:?} {r}");
        }
        let (_, _, r) = earth_heliocentric(jd, SunSeries::Full);
        assert!((r - 0.997_608_53).abs() < 1e-8, "{r}");
    }

    #[test]
    fn meeus_25b() {
        // 1992 October 13.0 TD. With the complete VSOP87 Meeus gives
        // apparent λ = 199°54′21.56″, β = +0.72″, α = 13h13m30.749s,
        // δ = −7°47′01.74″ (with his truncated tables λ = 199°54′21.818″).
        let jd = 2_448_908.5;
        let nut = NutationObliquity::at(jd);
        for series in [SunSeries::Truncated, SunSeries::Full] {
            let ecl = sun_apparent_ecliptic(jd, series, &nut);
            let lon = (199.0 + 54.0 / 60.0 + 21.56 / 3600.0) * DEG;
            assert!(
                (ecl.lon - lon).abs() < 0.02 * ARCSEC,
                "{series:?} dlon = {}\"",
                (ecl.lon - lon) / ARCSEC
            );
            assert!(
                (ecl.lat / ARCSEC - 0.72).abs() < 0.04,
                "{series:?} lat = {}\"",
                ecl.lat / ARCSEC
            );
            let eq = ecliptic_to_equatorial(&ecl, nut.eps);
            let ra = (13.0 + 13.0 / 60.0 + 30.749 / 3600.0) * 15.0 * DEG;
            let dec = -(7.0 + 47.0 / 60.0 + 1.74 / 3600.0) * DEG;
            assert!(
                (eq.ra - ra).abs() < 0.01 * 15.0 * ARCSEC,
                "{series:?} dra = {}s",
                (eq.ra - ra) / (15.0 * ARCSEC)
            );
            assert!(
                (eq.dec - dec).abs() < 0.1 * ARCSEC,
                "{series:?} ddec = {}\"",
                (eq.dec - dec) / ARCSEC
            );
        }
    }

    #[test]
    fn meeus_25a_low_accuracy_agreement() {
        // Example 25.a: apparent λ = 199.90895°, R = 0.99766 AU (low accuracy
        // method, ±0.01°).
        let jd = 2_448_908.5;
        let nut = NutationObliquity::at(jd);
        let ecl = sun_apparent_ecliptic(jd, SunSeries::Truncated, &nut);
        assert!((ecl.lon / DEG - 199.908_95).abs() < 0.01);
        assert!((ecl.distance_km / AU_KM - 0.99766).abs() < 1e-4);
    }

    #[test]
    fn truncation_error_is_small() {
        let mut max_l: f64 = 0.0;
        let mut max_b: f64 = 0.0;
        let mut max_r: f64 = 0.0;
        // 1000–3000 CE in steps of ~7.3 years with a varying day offset.
        for i in 0..275 {
            let jd = 2_086_302.5 + i as f64 * 2_663.3;
            let (l1, b1, r1) = earth_heliocentric(jd, SunSeries::Truncated);
            let (l2, b2, r2) = earth_heliocentric(jd, SunSeries::Full);
            max_l = max_l.max(crate::angle::norm_pi(l1 - l2).abs());
            max_b = max_b.max((b1 - b2).abs());
            max_r = max_r.max((r1 - r2).abs());
        }
        assert!(max_l < 0.12 * ARCSEC, "L {}\"", max_l / ARCSEC);
        assert!(max_b < 0.12 * ARCSEC, "B {}\"", max_b / ARCSEC);
        assert!(max_r < 1e-6, "R {max_r}");
    }
}
