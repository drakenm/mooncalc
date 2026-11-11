//! Atmospheric refraction (Meeus, *Astronomical Algorithms* 2nd ed., ch. 16).
//!
//! * Bennett (1982), Meeus eq. 16.3: refraction from the **apparent**
//!   altitude, accurate to 0.07′ for 0°–90°.
//! * Sæmundsson (1986), Meeus eq. 16.4: refraction from the **true**
//!   (airless) altitude, consistent with Bennett to about 4″.
//!
//! Both include Meeus' small constants that make the refraction vanish at
//! the zenith, and scale by `(P / 1010) · (283 / (273 + T))` for pressure
//! `P` (millibar) and temperature `T` (°C), Meeus p. 107. Below −1° altitude
//! the formulas are not physically meaningful; the value at −1° is returned.

use crate::angle::{ARCMIN, DEG};

/// Lowest altitude (degrees) at which the formulas are evaluated.
const MIN_ALT_DEG: f64 = -1.0;

/// Atmospheric conditions at the observer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    pub pressure_mbar: f64,
    pub temperature_c: f64,
}

impl Atmosphere {
    /// The standard conditions assumed by Bennett and Sæmundsson:
    /// 1010 mbar and 10 °C.
    pub const STANDARD: Self = Self {
        pressure_mbar: 1010.0,
        temperature_c: 10.0,
    };

    /// No atmosphere: every refraction is zero.
    pub const NONE: Self = Self {
        pressure_mbar: 0.0,
        temperature_c: 10.0,
    };

    /// Multiplicative correction to the standard refraction.
    #[inline]
    pub fn factor(&self) -> f64 {
        (self.pressure_mbar / 1010.0) * (283.0 / (273.0 + self.temperature_c))
    }
}

impl Default for Atmosphere {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// Refraction R (radians) for an **apparent** altitude `h0` (radians),
/// Bennett's formula (Meeus eq. 16.3). True altitude = `h0 − R`.
pub fn refraction_from_apparent(h0: f64, atm: &Atmosphere) -> f64 {
    let h = (h0 / DEG).max(MIN_ALT_DEG);
    let r_arcmin = 1.0 / ((h + 7.31 / (h + 4.4)) * DEG).tan() + 0.001_351_5;
    r_arcmin.max(0.0) * ARCMIN * atm.factor()
}

/// Refraction R (radians) for a **true** (airless) altitude `h` (radians),
/// Sæmundsson's formula (Meeus eq. 16.4). Apparent altitude = `h + R`.
pub fn refraction_from_true(h: f64, atm: &Atmosphere) -> f64 {
    let hd = (h / DEG).max(MIN_ALT_DEG);
    let r_arcmin = 1.02 / ((hd + 10.3 / (hd + 5.11)) * DEG).tan() + 0.001_927_9;
    r_arcmin.max(0.0) * ARCMIN * atm.factor()
}

/// Apparent altitude for a true altitude (both radians).
#[inline]
pub fn apparent_altitude(h_true: f64, atm: &Atmosphere) -> f64 {
    h_true + refraction_from_true(h_true, atm)
}

/// True altitude for an apparent altitude (both radians).
#[inline]
pub fn true_altitude(h_apparent: f64, atm: &Atmosphere) -> f64 {
    h_apparent - refraction_from_apparent(h_apparent, atm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::angle::ARCSEC;

    #[test]
    fn bennett_low_altitude() {
        // Apparent altitude 0°30′ (0.5°, the setting Sun's lower limb):
        // eq. 16.3 gives 1/tan(1.99184°) = 28.754′ (+0.001′ zenith constant).
        let atm = Atmosphere::STANDARD;
        let r = refraction_from_apparent(0.5 * DEG, &atm) / ARCMIN;
        assert!((r - 28.754).abs() < 0.003, "{r}");
        // Sæmundsson inverts it to within a few arcseconds.
        let h_true = 0.5 * DEG - r * ARCMIN;
        let r2 = refraction_from_true(h_true, &atm);
        assert!(
            (r2 - r * ARCMIN).abs() < 4.0 * ARCSEC,
            "{}",
            (r2 / ARCMIN - r)
        );
    }

    #[test]
    fn horizon_and_zenith() {
        let atm = Atmosphere::STANDARD;
        // The classical horizon refraction is about 34′–35′.
        let r0 = refraction_from_apparent(0.0, &atm) / ARCMIN;
        assert!((r0 - 34.5).abs() < 0.2, "{r0}");
        assert!(refraction_from_apparent(90.0 * DEG, &atm).abs() < 0.01 * ARCSEC);
        assert!(refraction_from_true(90.0 * DEG, &atm).abs() < 0.01 * ARCSEC);
        assert_eq!(refraction_from_true(0.3, &Atmosphere::NONE), 0.0);
    }

    #[test]
    fn consistency_across_altitudes() {
        let atm = Atmosphere::STANDARD;
        for hd in [0.0, 1.0, 2.0, 5.0, 10.0, 20.0, 45.0, 80.0] {
            let h0 = hd * DEG;
            let h = true_altitude(h0, &atm);
            let back = apparent_altitude(h, &atm);
            assert!(
                (back - h0).abs() < 5.0 * ARCSEC,
                "{hd}: {}",
                (back - h0) / ARCSEC
            );
        }
    }

    #[test]
    fn pressure_temperature_scaling() {
        let cold = Atmosphere {
            pressure_mbar: 1010.0,
            temperature_c: -10.0,
        };
        let r_std = refraction_from_true(0.0, &Atmosphere::STANDARD);
        let r_cold = refraction_from_true(0.0, &cold);
        assert!((r_cold / r_std - 283.0 / 263.0).abs() < 1e-12);
    }
}
