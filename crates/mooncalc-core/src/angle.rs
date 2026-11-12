//! Small angle helpers.

use std::f64::consts::TAU;

pub const DEG: f64 = std::f64::consts::PI / 180.0;
pub const ARCMIN: f64 = DEG / 60.0;
pub const ARCSEC: f64 = DEG / 3600.0;

/// Normalise an angle to `[0, 2π)`.
pub fn norm_2pi(a: f64) -> f64 {
    let r = a.rem_euclid(TAU);
    if r >= TAU { 0.0 } else { r }
}

/// Normalise an angle to `(-π, π]`.
pub fn norm_pi(a: f64) -> f64 {
    let r = norm_2pi(a);
    if r > std::f64::consts::PI { r - TAU } else { r }
}

/// Angular separation between two points given as (longitude-like, latitude-like) pairs.
/// Uses the Vincenty formula, which is stable for all separations.
pub fn separation(lon1: f64, lat1: f64, lon2: f64, lat2: f64) -> f64 {
    let dl = lon2 - lon1;
    let (s1, c1) = lat1.sin_cos();
    let (s2, c2) = lat2.sin_cos();
    let (sdl, cdl) = dl.sin_cos();
    let num = ((c2 * sdl).powi(2) + (c1 * s2 - s1 * c2 * cdl).powi(2)).sqrt();
    let den = s1 * s2 + c1 * c2 * cdl;
    num.atan2(den)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisation() {
        assert!((norm_2pi(-0.5) - (TAU - 0.5)).abs() < 1e-15);
        assert!((norm_pi(3.5 * std::f64::consts::PI) + 0.5 * std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn separation_quarter_circle() {
        assert!((separation(0.0, 0.0, 90.0 * DEG, 0.0) - 90.0 * DEG).abs() < 1e-12);
        assert!((separation(0.0, 0.0, 0.0, 90.0 * DEG) - 90.0 * DEG).abs() < 1e-12);
    }
}
