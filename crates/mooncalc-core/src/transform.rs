//! Coordinate transformations between ecliptic, equatorial and horizontal
//! frames (Meeus, *Astronomical Algorithms* 2nd ed., ch. 13).
//!
//! Azimuth is measured from north through east (Meeus measures it from the
//! south; the formulas here are rotated by 180°).

use crate::angle::{norm_2pi, norm_pi};
use crate::coords::{Ecliptic, Equatorial, Horizontal};

/// Ecliptic → equatorial for obliquity `eps` (Meeus eqs. 13.3, 13.4).
/// Use the true obliquity for apparent coordinates of date. Distance is kept.
pub fn ecliptic_to_equatorial(ecl: &Ecliptic, eps: f64) -> Equatorial {
    let (se, ce) = eps.sin_cos();
    let (sl, cl) = ecl.lon.sin_cos();
    let (sb, cb) = ecl.lat.sin_cos();
    let ra = norm_2pi((sl * ce * cb - sb * se).atan2(cl * cb));
    let dec = (sb * ce + cb * se * sl).clamp(-1.0, 1.0).asin();
    Equatorial {
        ra,
        dec,
        distance_km: ecl.distance_km,
    }
}

/// Equatorial → ecliptic for obliquity `eps` (Meeus eqs. 13.1, 13.2).
pub fn equatorial_to_ecliptic(eq: &Equatorial, eps: f64) -> Ecliptic {
    let (se, ce) = eps.sin_cos();
    let (sa, ca) = eq.ra.sin_cos();
    let (sd, cd) = eq.dec.sin_cos();
    let lon = norm_2pi((sa * ce * cd + sd * se).atan2(ca * cd));
    let lat = (sd * ce - cd * se * sa).clamp(-1.0, 1.0).asin();
    Ecliptic {
        lon,
        lat,
        distance_km: eq.distance_km,
    }
}

/// Local hour angle `LST − α`, normalised to `(−π, π]` (west of the
/// meridian is positive).
#[inline]
pub fn hour_angle(lst: f64, ra: f64) -> f64 {
    norm_pi(lst - ra)
}

/// Horizontal coordinates from hour angle `ha`, declination `dec` and
/// geographic latitude `lat` (Meeus eqs. 13.5, 13.6, azimuth from north).
/// No refraction is applied.
pub fn hadec_to_horizontal(ha: f64, dec: f64, lat: f64) -> Horizontal {
    let (sh, ch) = ha.sin_cos();
    let (sd, cd) = dec.sin_cos();
    let (sp, cp) = lat.sin_cos();
    let alt = (sp * sd + cp * cd * ch).clamp(-1.0, 1.0).asin();
    let az = norm_2pi((-cd * sh).atan2(sd * cp - cd * ch * sp));
    Horizontal { alt, az }
}

/// Inverse of [`hadec_to_horizontal`]: `(hour angle in (−π, π], declination)`.
pub fn horizontal_to_hadec(h: &Horizontal, lat: f64) -> (f64, f64) {
    let (sa, ca) = h.alt.sin_cos();
    let (sz, cz) = h.az.sin_cos();
    let (sp, cp) = lat.sin_cos();
    let dec = (sp * sa + cp * ca * cz).clamp(-1.0, 1.0).asin();
    let ha = (-sz * ca).atan2(cp * sa - sp * ca * cz);
    (norm_pi(ha), dec)
}

/// Horizontal coordinates of an equatorial position for an observer at
/// geographic latitude `lat` and local sidereal time `lst` (use apparent
/// sidereal time with apparent coordinates). No refraction is applied.
pub fn equatorial_to_horizontal(eq: &Equatorial, lat: f64, lst: f64) -> Horizontal {
    hadec_to_horizontal(lst - eq.ra, eq.dec, lat)
}

/// Inverse of [`equatorial_to_horizontal`]: `(ra in [0, 2π), dec)`.
pub fn horizontal_to_equatorial(h: &Horizontal, lat: f64, lst: f64) -> (f64, f64) {
    let (ha, dec) = horizontal_to_hadec(h, lat);
    (norm_2pi(lst - ha), dec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::angle::{ARCSEC, DEG};

    #[test]
    fn meeus_13a_ecliptic() {
        // Pollux: α = 116.328942°, δ = +28.026183°, ε = 23.4392911°
        // → λ = 113.215630°, β = +6.684170°.
        let eps = 23.439_291_1 * DEG;
        let eq = Equatorial {
            ra: 116.328_942 * DEG,
            dec: 28.026_183 * DEG,
            distance_km: 1.0,
        };
        let ecl = equatorial_to_ecliptic(&eq, eps);
        assert!((ecl.lon / DEG - 113.215_630).abs() < 2e-6);
        assert!((ecl.lat / DEG - 6.684_170).abs() < 2e-6);
        let back = ecliptic_to_equatorial(&ecl, eps);
        assert!((back.ra - eq.ra).abs() < 1e-12 && (back.dec - eq.dec).abs() < 1e-12);
    }

    #[test]
    fn meeus_13b_horizontal() {
        // Venus from Washington, 1987 April 10, 19h21m UT:
        // H = 64.352133°, δ = −6°43′11.61″, φ = +38°55′17″
        // → A = 68.0337° from the south (248.0337° from north), h = 15.1249°.
        let lat = (38.0 + 55.0 / 60.0 + 17.0 / 3600.0) * DEG;
        let dec = -(6.0 + 43.0 / 60.0 + 11.61 / 3600.0) * DEG;
        let hz = hadec_to_horizontal(64.352_133 * DEG, dec, lat);
        assert!((hz.az / DEG - 248.0337).abs() < 1e-4);
        assert!((hz.alt / DEG - 15.1249).abs() < 1e-4);
        let (ha, d) = horizontal_to_hadec(&hz, lat);
        assert!((ha / DEG - 64.352_133).abs() < 1e-9 && (d - dec).abs() < 1e-12);
    }

    #[test]
    fn meridian_and_round_trip() {
        let lat = 40.0 * DEG;
        let south = hadec_to_horizontal(0.0, 10.0 * DEG, lat);
        assert!((south.az - std::f64::consts::PI).abs() < 1e-12);
        assert!((south.alt - 60.0 * DEG).abs() < 1e-12);
        let eq = Equatorial {
            ra: 5.0,
            dec: -0.3,
            distance_km: 1.0,
        };
        let h = equatorial_to_horizontal(&eq, lat, 1.2);
        let (ra, dec) = horizontal_to_equatorial(&h, lat, 1.2);
        assert!((ra - 5.0).abs() < 1e-10 && (dec + 0.3).abs() < 1e-10);
        assert!(
            hour_angle(0.1, 6.2).abs() < 0.3
                && (hour_angle(0.1, 6.2) - (0.1 - 6.2 + std::f64::consts::TAU)).abs() < ARCSEC
        );
    }
}
