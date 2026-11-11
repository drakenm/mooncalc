//! Nutation and the obliquity of the ecliptic.
//!
//! Meeus, *Astronomical Algorithms* 2nd ed., ch. 22: the IAU 1980 theory of
//! nutation (Wahr 1981; Seidelmann 1982) with the 63 terms of Table 22.A
//! (terms below 0.0003″ neglected), and the mean obliquity of Laskar (1986),
//! Meeus eq. 22.3.

use crate::angle::{ARCSEC, DEG};
use crate::matrix::{Mat3, mul, rot_x, rot_z};
use crate::time::centuries_since_j2000;

/// Nutation in longitude and obliquity, radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nutation {
    /// Δψ, nutation in longitude.
    pub dpsi: f64,
    /// Δε, nutation in obliquity.
    pub deps: f64,
}

/// Nutation together with the mean and true obliquity: everything needed
/// to go from mean to true equator and equinox of date.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NutationObliquity {
    /// Δψ, nutation in longitude, radians.
    pub dpsi: f64,
    /// Δε, nutation in obliquity, radians.
    pub deps: f64,
    /// ε₀, mean obliquity of the ecliptic, radians.
    pub eps0: f64,
    /// ε = ε₀ + Δε, true obliquity, radians.
    pub eps: f64,
}

impl NutationObliquity {
    /// Evaluate at a TT Julian Day.
    pub fn at(jd_tt: f64) -> Self {
        let n = nutation(jd_tt);
        let eps0 = mean_obliquity(jd_tt);
        Self {
            dpsi: n.dpsi,
            deps: n.deps,
            eps0,
            eps: eps0 + n.deps,
        }
    }

    /// The equation of the equinoxes Δψ cos ε (radians), i.e. apparent minus
    /// mean sidereal time (Meeus ch. 12).
    pub fn equation_of_equinoxes(&self) -> f64 {
        self.dpsi * self.eps.cos()
    }

    /// Nutation matrix: mean equator and equinox of date → true equator and
    /// equinox of date, `N = R1(−ε) · R3(−Δψ) · R1(ε₀)` (Explanatory
    /// Supplement to the Astronomical Almanac, 1992, §3.222).
    pub fn matrix(&self) -> Mat3 {
        mul(
            &rot_x(-self.eps),
            &mul(&rot_z(-self.dpsi), &rot_x(self.eps0)),
        )
    }
}

/// Nutation matrix at a TT Julian Day (see [`NutationObliquity::matrix`]).
pub fn nutation_matrix(jd_tt: f64) -> Mat3 {
    NutationObliquity::at(jd_tt).matrix()
}

/// Mean obliquity of the ecliptic, radians (Laskar 1986, Meeus eq. 22.3).
/// Accurate to 0.01″ over 1000–3000 and to a few arcseconds over 10 000
/// years either side of J2000.
pub fn mean_obliquity(jd_tt: f64) -> f64 {
    const C: [f64; 11] = [
        84_381.448, -4680.93, -1.55, 1999.25, -51.38, -249.67, -39.05, 7.12, 27.87, 5.79, 2.45,
    ];
    let u = centuries_since_j2000(jd_tt) / 100.0;
    C.iter().rev().fold(0.0, |acc, &k| acc * u + k) * ARCSEC
}

/// True obliquity ε₀ + Δε, radians.
pub fn true_obliquity(jd_tt: f64) -> f64 {
    NutationObliquity::at(jd_tt).eps
}

/// Fundamental arguments of the IAU 1980 nutation series (Meeus ch. 22),
/// radians: (D, M, M′, F, Ω).
fn fundamental_arguments(t: f64) -> [f64; 5] {
    let d = 297.850_36 + t * (445_267.111_48 + t * (-0.001_914_2 + t / 189_474.0));
    let m = 357.527_72 + t * (35_999.050_34 + t * (-0.000_160_3 - t / 300_000.0));
    let mp = 134.962_98 + t * (477_198.867_398 + t * (0.008_697_2 + t / 56_250.0));
    let f = 93.271_91 + t * (483_202.017_538 + t * (-0.003_682_5 + t / 327_270.0));
    let om = 125.044_52 + t * (-1_934.136_261 + t * (0.002_070_8 + t / 450_000.0));
    [
        (d % 360.0) * DEG,
        (m % 360.0) * DEG,
        (mp % 360.0) * DEG,
        (f % 360.0) * DEG,
        (om % 360.0) * DEG,
    ]
}

/// Nutation in longitude and obliquity at a TT Julian Day (Meeus ch. 22,
/// IAU 1980, Table 22.A). Accuracy 0.0003″ relative to the full IAU 1980
/// series.
pub fn nutation(jd_tt: f64) -> Nutation {
    let t = centuries_since_j2000(jd_tt);
    let [d, m, mp, f, om] = fundamental_arguments(t);
    let mut dpsi = 0.0;
    let mut deps = 0.0;
    // Smallest terms first, for marginally better rounding.
    for row in TABLE_22A.iter().rev() {
        let arg = row[0] * d + row[1] * m + row[2] * mp + row[3] * f + row[4] * om;
        let (s, c) = arg.sin_cos();
        dpsi += (row[5] + row[6] * t) * s;
        deps += (row[7] + row[8] * t) * c;
    }
    Nutation {
        dpsi: dpsi * 1e-4 * ARCSEC,
        deps: deps * 1e-4 * ARCSEC,
    }
}

/// Meeus Table 22.A: multiples of D, M, M′, F, Ω, then the coefficients of
/// sin (Δψ) and cos (Δε) in units of 0.0001″, constant and T terms.
#[rustfmt::skip]
const TABLE_22A: [[f64; 9]; 63] = [
    [0.0, 0.0, 0.0, 0.0, 1.0, -171996.0, -174.2, 92025.0, 8.9],
    [-2.0, 0.0, 0.0, 2.0, 2.0, -13187.0, -1.6, 5736.0, -3.1],
    [0.0, 0.0, 0.0, 2.0, 2.0, -2274.0, -0.2, 977.0, -0.5],
    [0.0, 0.0, 0.0, 0.0, 2.0, 2062.0, 0.2, -895.0, 0.5],
    [0.0, 1.0, 0.0, 0.0, 0.0, 1426.0, -3.4, 54.0, -0.1],
    [0.0, 0.0, 1.0, 0.0, 0.0, 712.0, 0.1, -7.0, 0.0],
    [-2.0, 1.0, 0.0, 2.0, 2.0, -517.0, 1.2, 224.0, -0.6],
    [0.0, 0.0, 0.0, 2.0, 1.0, -386.0, -0.4, 200.0, 0.0],
    [0.0, 0.0, 1.0, 2.0, 2.0, -301.0, 0.0, 129.0, -0.1],
    [-2.0, -1.0, 0.0, 2.0, 2.0, 217.0, -0.5, -95.0, 0.3],
    [-2.0, 0.0, 1.0, 0.0, 0.0, -158.0, 0.0, 0.0, 0.0],
    [-2.0, 0.0, 0.0, 2.0, 1.0, 129.0, 0.1, -70.0, 0.0],
    [0.0, 0.0, -1.0, 2.0, 2.0, 123.0, 0.0, -53.0, 0.0],
    [2.0, 0.0, 0.0, 0.0, 0.0, 63.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0, 1.0, 63.0, 0.1, -33.0, 0.0],
    [2.0, 0.0, -1.0, 2.0, 2.0, -59.0, 0.0, 26.0, 0.0],
    [0.0, 0.0, -1.0, 0.0, 1.0, -58.0, -0.1, 32.0, 0.0],
    [0.0, 0.0, 1.0, 2.0, 1.0, -51.0, 0.0, 27.0, 0.0],
    [-2.0, 0.0, 2.0, 0.0, 0.0, 48.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, -2.0, 2.0, 1.0, 46.0, 0.0, -24.0, 0.0],
    [2.0, 0.0, 0.0, 2.0, 2.0, -38.0, 0.0, 16.0, 0.0],
    [0.0, 0.0, 2.0, 2.0, 2.0, -31.0, 0.0, 13.0, 0.0],
    [0.0, 0.0, 2.0, 0.0, 0.0, 29.0, 0.0, 0.0, 0.0],
    [-2.0, 0.0, 1.0, 2.0, 2.0, 29.0, 0.0, -12.0, 0.0],
    [0.0, 0.0, 0.0, 2.0, 0.0, 26.0, 0.0, 0.0, 0.0],
    [-2.0, 0.0, 0.0, 2.0, 0.0, -22.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, -1.0, 2.0, 1.0, 21.0, 0.0, -10.0, 0.0],
    [0.0, 2.0, 0.0, 0.0, 0.0, 17.0, -0.1, 0.0, 0.0],
    [2.0, 0.0, -1.0, 0.0, 1.0, 16.0, 0.0, -8.0, 0.0],
    [-2.0, 2.0, 0.0, 2.0, 2.0, -16.0, 0.1, 7.0, 0.0],
    [0.0, 1.0, 0.0, 0.0, 1.0, -15.0, 0.0, 9.0, 0.0],
    [-2.0, 0.0, 1.0, 0.0, 1.0, -13.0, 0.0, 7.0, 0.0],
    [0.0, -1.0, 0.0, 0.0, 1.0, -12.0, 0.0, 6.0, 0.0],
    [0.0, 0.0, 2.0, -2.0, 0.0, 11.0, 0.0, 0.0, 0.0],
    [2.0, 0.0, -1.0, 2.0, 1.0, -10.0, 0.0, 5.0, 0.0],
    [2.0, 0.0, 1.0, 2.0, 2.0, -8.0, 0.0, 3.0, 0.0],
    [0.0, 1.0, 0.0, 2.0, 2.0, 7.0, 0.0, -3.0, 0.0],
    [-2.0, 1.0, 1.0, 0.0, 0.0, -7.0, 0.0, 0.0, 0.0],
    [0.0, -1.0, 0.0, 2.0, 2.0, -7.0, 0.0, 3.0, 0.0],
    [2.0, 0.0, 0.0, 2.0, 1.0, -7.0, 0.0, 3.0, 0.0],
    [2.0, 0.0, 1.0, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0],
    [-2.0, 0.0, 2.0, 2.0, 2.0, 6.0, 0.0, -3.0, 0.0],
    [-2.0, 0.0, 1.0, 2.0, 1.0, 6.0, 0.0, -3.0, 0.0],
    [2.0, 0.0, -2.0, 0.0, 1.0, -6.0, 0.0, 3.0, 0.0],
    [2.0, 0.0, 0.0, 0.0, 1.0, -6.0, 0.0, 3.0, 0.0],
    [0.0, -1.0, 1.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0],
    [-2.0, -1.0, 0.0, 2.0, 1.0, -5.0, 0.0, 3.0, 0.0],
    [-2.0, 0.0, 0.0, 0.0, 1.0, -5.0, 0.0, 3.0, 0.0],
    [0.0, 0.0, 2.0, 2.0, 1.0, -5.0, 0.0, 3.0, 0.0],
    [-2.0, 0.0, 2.0, 0.0, 1.0, 4.0, 0.0, 0.0, 0.0],
    [-2.0, 1.0, 0.0, 2.0, 1.0, 4.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, -2.0, 0.0, 4.0, 0.0, 0.0, 0.0],
    [-1.0, 0.0, 1.0, 0.0, 0.0, -4.0, 0.0, 0.0, 0.0],
    [-2.0, 1.0, 0.0, 0.0, 0.0, -4.0, 0.0, 0.0, 0.0],
    [1.0, 0.0, 0.0, 0.0, 0.0, -4.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 2.0, 0.0, 3.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, -2.0, 2.0, 2.0, -3.0, 0.0, 0.0, 0.0],
    [-1.0, -1.0, 1.0, 0.0, 0.0, -3.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 1.0, 0.0, 0.0, -3.0, 0.0, 0.0, 0.0],
    [0.0, -1.0, 1.0, 2.0, 2.0, -3.0, 0.0, 0.0, 0.0],
    [2.0, -1.0, -1.0, 2.0, 2.0, -3.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 3.0, 2.0, 2.0, -3.0, 0.0, 0.0, 0.0],
    [2.0, -1.0, 0.0, 2.0, 2.0, -3.0, 0.0, 0.0, 0.0],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeus_22a() {
        // 1987 April 10, 0h TD: Δψ = −3.788″, Δε = +9.443″,
        // ε₀ = 23°26′27.407″, ε = 23°26′36.850″.
        let n = NutationObliquity::at(2_446_895.5);
        assert!(
            (n.dpsi / ARCSEC + 3.788).abs() < 0.0005,
            "{}",
            n.dpsi / ARCSEC
        );
        assert!(
            (n.deps / ARCSEC - 9.443).abs() < 0.0005,
            "{}",
            n.deps / ARCSEC
        );
        let e0 = (23.0 * 3600.0 + 26.0 * 60.0 + 27.407) * ARCSEC;
        let e = (23.0 * 3600.0 + 26.0 * 60.0 + 36.850) * ARCSEC;
        assert!((n.eps0 - e0).abs() < 0.0005 * ARCSEC);
        assert!((n.eps - e).abs() < 0.001 * ARCSEC);
    }

    #[test]
    fn matrix_matches_series_in_longitude() {
        // A direction at mean ecliptic longitude 0 (the mean equinox) has
        // true ecliptic longitude +Δψ.
        let n = NutationObliquity::at(2_460_000.5);
        let m = n.matrix();
        // A point on the mean ecliptic at longitude 0 (the mean equinox).
        let v = crate::matrix::apply(&m, &[1.0, 0.0, 0.0]);
        // Convert to true-ecliptic longitude: rotate by R1(ε).
        let w = crate::matrix::apply(&rot_x(n.eps), &v);
        let lon = w[1].atan2(w[0]);
        assert!((lon - n.dpsi).abs() < 1e-11);
    }
}
