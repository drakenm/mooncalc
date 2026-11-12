//! Illuminated fraction and bright-limb geometry of the Moon (Meeus,
//! *Astronomical Algorithms* 2nd ed., ch. 48). Works equally for
//! geocentric or topocentric positions, provided both bodies are given in
//! the same frame and from the same observer.

use crate::angle::{norm_2pi, separation};
use crate::coords::Equatorial;

/// Elongation ψ of the Moon from the Sun, radians in `[0, π]` (Meeus eq. 48.2).
#[inline]
pub fn elongation(sun: &Equatorial, moon: &Equatorial) -> f64 {
    separation(sun.ra, sun.dec, moon.ra, moon.dec)
}

/// Phase angle i (Sun–Moon–observer angle), radians, from the elongation
/// and the observer–Sun and observer–Moon distances (Meeus eq. 48.3).
#[inline]
pub fn phase_angle(elongation: f64, sun_distance_km: f64, moon_distance_km: f64) -> f64 {
    let (s, c) = elongation.sin_cos();
    (sun_distance_km * s).atan2(moon_distance_km - sun_distance_km * c)
}

/// Illuminated fraction k = (1 + cos i) / 2 (Meeus eq. 48.1).
#[inline]
pub fn illuminated_fraction(phase_angle: f64) -> f64 {
    0.5 * (1.0 + phase_angle.cos())
}

/// Position angle χ of the Moon's bright limb, radians in `[0, 2π)`,
/// measured eastward from the north point of the disk (Meeus eq. 48.5).
pub fn bright_limb_position_angle(sun: &Equatorial, moon: &Equatorial) -> f64 {
    let (sd0, cd0) = sun.dec.sin_cos();
    let (sd, cd) = moon.dec.sin_cos();
    let (sa, ca) = (sun.ra - moon.ra).sin_cos();
    norm_2pi((cd0 * sa).atan2(sd0 * cd - cd0 * sd * ca))
}

/// All the illumination quantities at once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Illumination {
    /// Elongation ψ, radians.
    pub elongation: f64,
    /// Phase angle i, radians.
    pub phase_angle: f64,
    /// Illuminated fraction k, 0–1.
    pub fraction: f64,
    /// Position angle of the bright limb χ, radians.
    pub bright_limb_angle: f64,
}

impl Illumination {
    /// Compute from the Sun's and Moon's equatorial positions (same frame
    /// and observer, distances in km).
    pub fn compute(sun: &Equatorial, moon: &Equatorial) -> Self {
        let elongation = elongation(sun, moon);
        let phase_angle = phase_angle(elongation, sun.distance_km, moon.distance_km);
        Self {
            elongation,
            phase_angle,
            fraction: illuminated_fraction(phase_angle),
            bright_limb_angle: bright_limb_position_angle(sun, moon),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::angle::DEG;

    #[test]
    fn meeus_48a() {
        // 1992 April 12, 0h TD. Moon: α = 134.688470°, δ = +13.768368°,
        // Δ = 368410 km. Sun: α = 20.6579°, δ = +8.6964°, R = 149971520 km.
        // ψ = 110.7929°, i = 69.0756°, k = 0.6786, χ = 285.0°.
        let moon = Equatorial {
            ra: 134.688_470 * DEG,
            dec: 13.768_368 * DEG,
            distance_km: 368_410.0,
        };
        let sun = Equatorial {
            ra: 20.6579 * DEG,
            dec: 8.6964 * DEG,
            distance_km: 149_971_520.0,
        };
        let ill = Illumination::compute(&sun, &moon);
        assert!((ill.elongation / DEG - 110.7929).abs() < 1e-4);
        assert!((ill.phase_angle / DEG - 69.0756).abs() < 1e-4);
        assert!((ill.fraction - 0.6786).abs() < 5e-5);
        assert!((ill.bright_limb_angle / DEG - 285.0).abs() < 0.05);
    }
}
