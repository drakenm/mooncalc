//! Precession of equatorial coordinates.
//!
//! * [`precession_matrix`] — rigorous precession between two arbitrary
//!   epochs with the IAU 1976 angles ζ, z, θ (Lieske et al. 1977), Meeus,
//!   *Astronomical Algorithms* 2nd ed., ch. 21, eqs. 21.2 and 21.4.
//! * [`precession_matrix_iau2006`] — J2000 mean equator to mean equator of
//!   date with the IAU 2006 (P03) angles of Capitaine, Wallace & Chapront
//!   (2003), IERS Conventions (2010) eq. 5.40, for the DE440 backend.
//! * [`frame_bias_matrix`] — ICRS → J2000 mean equator and equinox, IERS
//!   Conventions (2010) §5.4.4.
//!
//! All matrices act on column vectors: `v_to = P v_from`.

use crate::angle::ARCSEC;
use crate::coords::Equatorial;
use crate::matrix::{self, Mat3, mul, rot_x, rot_y, rot_z};
use crate::nutation::NutationObliquity;
use crate::time::{DAYS_PER_CENTURY, J2000};

/// The IAU 1976 precession angles (ζ, z, θ) in radians for precessing from
/// the mean equator of `jd_from` to that of `jd_to` (both TT), Meeus eq. 21.2.
pub fn precession_angles(jd_from: f64, jd_to: f64) -> (f64, f64, f64) {
    let big_t = (jd_from - J2000) / DAYS_PER_CENTURY;
    let t = (jd_to - jd_from) / DAYS_PER_CENTURY;
    let a = 2306.2181 + big_t * (1.39656 - 0.000139 * big_t);
    let zeta = t * (a + t * ((0.30188 - 0.000344 * big_t) + 0.017998 * t));
    let z = t * (a + t * ((1.09468 + 0.000066 * big_t) + 0.018203 * t));
    let theta = t
        * ((2004.3109 + big_t * (-0.85330 - 0.000217 * big_t))
            + t * (-(0.42665 + 0.000217 * big_t) - 0.041833 * t));
    (zeta * ARCSEC, z * ARCSEC, theta * ARCSEC)
}

/// Rigorous precession matrix (IAU 1976) from the mean equator and equinox
/// of `jd_from` to that of `jd_to` (TT Julian Days):
/// `P = R3(−z) · R2(θ) · R3(−ζ)`, equivalent to Meeus eq. 21.4.
pub fn precession_matrix(jd_from: f64, jd_to: f64) -> Mat3 {
    let (zeta, z, theta) = precession_angles(jd_from, jd_to);
    mul(&rot_z(-z), &mul(&rot_y(theta), &rot_z(-zeta)))
}

/// IAU 2006 precession matrix from the J2000.0 mean equator and equinox to
/// the mean equator and equinox of date (TT). Uses the equatorial angles
/// ζ_A, z_A, θ_A of Capitaine et al. (2003), IERS Conventions (2010) eq. 5.40.
pub fn precession_matrix_iau2006(jd_tt: f64) -> Mat3 {
    let t = (jd_tt - J2000) / DAYS_PER_CENTURY;
    let poly = |c: [f64; 6]| c.iter().rev().fold(0.0, |acc, &k| acc * t + k) * ARCSEC;
    let zeta = poly([
        2.650545,
        2306.083227,
        0.2988499,
        0.01801828,
        -0.000005971,
        -0.0000003173,
    ]);
    let z = poly([
        -2.650545,
        2306.077181,
        1.0927348,
        0.01826837,
        -0.000028596,
        -0.0000002904,
    ]);
    let theta = poly([
        0.0,
        2004.191903,
        -0.4294934,
        -0.04182264,
        -0.000007089,
        -0.0000001274,
    ]);
    mul(&rot_z(-z), &mul(&rot_y(theta), &rot_z(-zeta)))
}

/// Frame bias matrix from the ICRS to the J2000.0 mean equator and equinox
/// (dynamical), IERS Conventions (2010) eq. 5.33 with
/// ξ₀ = −16.617 mas, η₀ = −6.819 mas, dα₀ = −14.6 mas:
/// `B = R1(−η₀) · R2(ξ₀) · R3(dα₀)`.
pub fn frame_bias_matrix() -> Mat3 {
    let xi0 = -0.016_617 * ARCSEC;
    let eta0 = -0.006_819 * ARCSEC;
    let da0 = -0.014_6 * ARCSEC;
    mul(&rot_x(-eta0), &mul(&rot_y(xi0), &rot_z(da0)))
}

/// Combined matrix from the J2000.0 mean equator and equinox to the **true**
/// equator and equinox of date: `N(jd) · P(J2000 → jd)` with IAU 1976
/// precession and IAU 1980 nutation (the Meeus model).
pub fn j2000_to_true_of_date_matrix(jd_tt: f64) -> Mat3 {
    let p = precession_matrix(J2000, jd_tt);
    mul(&NutationObliquity::at(jd_tt).matrix(), &p)
}

/// Inverse of [`j2000_to_true_of_date_matrix`].
pub fn true_of_date_to_j2000_matrix(jd_tt: f64) -> Mat3 {
    matrix::transpose(&j2000_to_true_of_date_matrix(jd_tt))
}

/// Apply a rotation matrix to equatorial coordinates, preserving distance.
pub fn rotate_equatorial(m: &Mat3, eq: &Equatorial) -> Equatorial {
    let v = matrix::apply(m, &matrix::from_spherical(eq.ra, eq.dec, 1.0));
    let (ra, dec, _) = matrix::to_spherical(&v);
    Equatorial {
        ra,
        dec,
        distance_km: eq.distance_km,
    }
}

/// Precess mean equatorial coordinates from the mean equator and equinox of
/// `jd_from` to that of `jd_to` (TT), rigorously (Meeus ch. 21). Proper
/// motion, if any, must be applied by the caller first.
pub fn precess_equatorial(eq: &Equatorial, jd_from: f64, jd_to: f64) -> Equatorial {
    rotate_equatorial(&precession_matrix(jd_from, jd_to), eq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::angle::DEG;
    use crate::matrix::transpose;

    fn hms(h: f64, m: f64, s: f64) -> f64 {
        (h + m / 60.0 + s / 3600.0) * 15.0 * DEG
    }
    fn dms(d: f64, m: f64, s: f64) -> f64 {
        (d + m / 60.0 + s / 3600.0) * DEG
    }

    #[test]
    fn meeus_21b() {
        // θ Persei, J2000.0: α = 2h44m11.986s, δ = +49°13′42.48″,
        // μα = +0.03425 s/yr, μδ = −0.0895″/yr. Target 2028 Nov 13.19 TD
        // (JD 2462088.69): α = 2h46m11.331s, δ = +49°20′54.54″.
        let jd = 2_462_088.69;
        let years = (jd - J2000) / 365.25;
        let ra0 = hms(2.0, 44.0, 11.986 + 0.03425 * years);
        let dec0 = dms(49.0, 13.0, 42.48 - 0.0895 * years);
        let eq = precess_equatorial(
            &Equatorial {
                ra: ra0,
                dec: dec0,
                distance_km: 1.0,
            },
            J2000,
            jd,
        );
        assert!((eq.ra - hms(2.0, 46.0, 11.331)).abs() < 0.001 * 15.0 * ARCSEC);
        assert!((eq.dec - dms(49.0, 20.0, 54.54)).abs() < 0.01 * ARCSEC);
        // And back again.
        let back = precess_equatorial(&eq, jd, J2000);
        assert!((back.ra - ra0).abs() < 1e-11 && (back.dec - dec0).abs() < 1e-11);
    }

    #[test]
    fn general_epoch_composition() {
        // P(a→c) = P(b→c) · P(a→b) to within the accuracy of the expansion.
        let (a, b, c) = (2_415_020.313_5, 2_433_282.423_5, 2_469_807.5);
        let direct = precession_matrix(a, c);
        let composed = mul(&precession_matrix(b, c), &precession_matrix(a, b));
        for i in 0..3 {
            for j in 0..3 {
                assert!((direct[i][j] - composed[i][j]).abs() < 1e-9);
            }
        }
        let i = mul(
            &precession_matrix(a, c),
            &transpose(&precession_matrix(a, c)),
        );
        assert!((i[1][1] - 1.0).abs() < 1e-14);
    }

    #[test]
    fn iau2006_close_to_iau1976() {
        // The models differ mainly by the −0.30″/cy correction to the
        // precession rate in longitude; over ±1 century they agree to 0.5″.
        for t in [-1.0, -0.3, 0.4, 1.0] {
            let jd = J2000 + t * DAYS_PER_CENTURY;
            let v = [0.3, -0.5, 0.81];
            let a = matrix::apply(&precession_matrix(J2000, jd), &v);
            let b = matrix::apply(&precession_matrix_iau2006(jd), &v);
            let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            assert!(d / 0.9997 < 0.5 * ARCSEC, "t={t}: {}", d / ARCSEC);
        }
    }

    #[test]
    fn frame_bias_is_tiny() {
        let b = frame_bias_matrix();
        assert!((b[0][1] - (-0.0146 * ARCSEC)).abs() < 1e-12);
        assert!((b[0][0] - 1.0).abs() < 1e-14);
    }
}
