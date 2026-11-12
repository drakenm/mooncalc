//! The default ephemeris: VSOP87D Sun and ELP-2000/82 (Meeus-truncated)
//! Moon, following Meeus, *Astronomical Algorithms* 2nd ed., ch. 25, 32 and
//! 47. No data files are needed.

pub mod moon;
pub mod sun;
mod vsop87d_earth;

pub use sun::SunSeries;

use crate::coords::{Ecliptic, Equatorial};
use crate::ephemeris::Ephemeris;
use crate::nutation::NutationObliquity;
use crate::transform::ecliptic_to_equatorial;

/// TT Julian Day of −2000 January 1.0 (Julian calendar).
pub const VALID_FROM_JD: f64 = 990_557.5;
/// TT Julian Day of +3000 December 31.0 (Gregorian calendar).
pub const VALID_TO_JD: f64 = 2_817_151.5;

/// Apparent positions of both bodies at one instant, sharing a single
/// nutation evaluation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunMoonApparent {
    pub nutation: NutationObliquity,
    pub sun_ecliptic: Ecliptic,
    pub sun_equatorial: Equatorial,
    pub moon_ecliptic: Ecliptic,
    pub moon_equatorial: Equatorial,
}

/// The Meeus/VSOP87/ELP-2000 ephemeris.
///
/// Accuracy: the Sun to about 1″ (limited by the FK5 conversion and the
/// aberration model, not the series); the Moon to about 10″ in longitude
/// and 4″ in latitude near the present (Meeus ch. 47), degrading slowly
/// away from J2000. The stated [`Ephemeris::valid_range`], years −2000 to
/// +3000, is where the polynomial arguments remain sensible; ΔT, not the
/// ephemeris, dominates the error for ancient dates.
///
/// The type is `Copy` and stateless; evaluation does not allocate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeeusEphemeris {
    pub sun_series: SunSeries,
}

impl MeeusEphemeris {
    /// The default configuration (truncated VSOP87D for speed).
    pub const fn new() -> Self {
        Self {
            sun_series: SunSeries::Truncated,
        }
    }

    /// Choose how much of the VSOP87D Earth series to evaluate.
    pub const fn with_sun_series(sun_series: SunSeries) -> Self {
        Self { sun_series }
    }

    /// Apparent ecliptic and equatorial Sun, reusing a nutation evaluation.
    pub fn sun_apparent(&self, jd_tt: f64, nut: &NutationObliquity) -> (Ecliptic, Equatorial) {
        let ecl = sun::sun_apparent_ecliptic(jd_tt, self.sun_series, nut);
        (ecl, ecliptic_to_equatorial(&ecl, nut.eps))
    }

    /// Apparent ecliptic and equatorial Moon, reusing a nutation evaluation.
    pub fn moon_apparent(&self, jd_tt: f64, nut: &NutationObliquity) -> (Ecliptic, Equatorial) {
        let ecl = moon::moon_apparent_ecliptic(jd_tt, nut);
        (ecl, ecliptic_to_equatorial(&ecl, nut.eps))
    }

    /// Both bodies at once (one nutation evaluation): the fast path for
    /// grid computations.
    pub fn sun_moon_apparent(&self, jd_tt: f64) -> SunMoonApparent {
        let nutation = NutationObliquity::at(jd_tt);
        let (sun_ecliptic, sun_equatorial) = self.sun_apparent(jd_tt, &nutation);
        let (moon_ecliptic, moon_equatorial) = self.moon_apparent(jd_tt, &nutation);
        SunMoonApparent {
            nutation,
            sun_ecliptic,
            sun_equatorial,
            moon_ecliptic,
            moon_equatorial,
        }
    }
}

impl Ephemeris for MeeusEphemeris {
    fn name(&self) -> &str {
        match self.sun_series {
            SunSeries::Truncated => "Meeus: VSOP87D (truncated) / ELP-2000/82 (truncated)",
            SunSeries::Full => "Meeus: VSOP87D (full) / ELP-2000/82 (truncated)",
        }
    }

    fn valid_range(&self) -> (f64, f64) {
        (VALID_FROM_JD, VALID_TO_JD)
    }

    fn sun_equatorial(&self, jd_tt: f64) -> Equatorial {
        self.sun_apparent(jd_tt, &NutationObliquity::at(jd_tt)).1
    }

    fn moon_equatorial(&self, jd_tt: f64) -> Equatorial {
        self.moon_apparent(jd_tt, &NutationObliquity::at(jd_tt)).1
    }

    fn sun_ecliptic(&self, jd_tt: f64) -> Ecliptic {
        sun::sun_apparent_ecliptic(jd_tt, self.sun_series, &NutationObliquity::at(jd_tt))
    }

    fn moon_ecliptic(&self, jd_tt: f64) -> Ecliptic {
        moon::moon_apparent_ecliptic(jd_tt, &NutationObliquity::at(jd_tt))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::angle::DEG;
    use crate::illumination::Illumination;
    use crate::time::julian_day;

    #[test]
    fn valid_range_dates() {
        assert_eq!(julian_day(-2000, 1, 1.0), VALID_FROM_JD);
        assert_eq!(julian_day(3000, 12, 31.0), VALID_TO_JD);
    }

    #[test]
    fn trait_object_and_meeus_48a() {
        // Meeus 48.a via the full pipeline: 1992 April 12, 0h TD.
        let eph: &dyn Ephemeris = &MeeusEphemeris::new();
        let jd = 2_448_724.5;
        let sun = eph.sun_equatorial(jd);
        let moon = eph.moon_equatorial(jd);
        // Sun: α = 20.6579°, δ = +8.6964°, R = 149971520 km.
        assert!((sun.ra / DEG - 20.6579).abs() < 1e-4);
        assert!((sun.dec / DEG - 8.6964).abs() < 1e-4);
        assert!((sun.distance_km - 149_971_520.0).abs() < 100.0);
        let ill = Illumination::compute(&sun, &moon);
        assert!((ill.fraction - 0.6786).abs() < 5e-5);
        let both = MeeusEphemeris::new().sun_moon_apparent(jd);
        assert_eq!(both.moon_equatorial, moon);
        assert_eq!(both.sun_equatorial, sun);
        assert_eq!(eph.moon_ecliptic(jd), both.moon_ecliptic);
    }
}
