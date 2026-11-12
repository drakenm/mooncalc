//! Observer geometry: geocentric observer position, topocentric parallax,
//! horizontal parallax and semi-diameters.
//!
//! References: Meeus, *Astronomical Algorithms* 2nd ed., ch. 11 (the Earth's
//! figure), ch. 40 (parallax in right ascension and declination, done here
//! rigorously with vectors, which is equivalent to Meeus eqs. 40.2/40.3 and
//! also yields the topocentric distance), ch. 55 (semi-diameters). The Earth
//! ellipsoid is WGS84 (a = 6378.137 km, f = 1/298.257223563).

use crate::angle::{ARCSEC, DEG};
use crate::coords::{Equatorial, GeoLocation};
use crate::matrix::{from_spherical, to_spherical};

/// WGS84 equatorial radius, km.
pub const EARTH_EQUATORIAL_RADIUS_KM: f64 = 6378.137;
/// WGS84 flattening.
pub const EARTH_FLATTENING: f64 = 1.0 / 298.257_223_563;
/// Astronomical unit, km (IAU 2012).
pub const AU_KM: f64 = 149_597_870.7;
/// Ratio of the Moon's radius to the Earth's equatorial radius used for
/// semi-diameters (Meeus ch. 55, k = 0.272481).
pub const MOON_RADIUS_RATIO: f64 = 0.272_481;
/// Solar semi-diameter at 1 AU, arcseconds (Meeus ch. 55).
pub const SUN_SEMIDIAMETER_1AU_ARCSEC: f64 = 959.63;

/// The observer's geocentric position expressed as `ρ sin φ′` and
/// `ρ cos φ′` in units of the Earth's equatorial radius (Meeus ch. 11).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObserverGeocentric {
    pub rho_sin_phi: f64,
    pub rho_cos_phi: f64,
}

impl ObserverGeocentric {
    /// From geodetic latitude (radians) and height above the ellipsoid (m).
    pub fn new(lat: f64, elevation_m: f64) -> Self {
        let ba = 1.0 - EARTH_FLATTENING;
        let u = (ba * lat.tan()).atan();
        let h = elevation_m / 1000.0 / EARTH_EQUATORIAL_RADIUS_KM;
        let (su, cu) = u.sin_cos();
        let (sp, cp) = lat.sin_cos();
        // Near the poles tan φ is huge but atan handles it; u keeps the sign.
        Self {
            rho_sin_phi: ba * su + h * sp,
            rho_cos_phi: cu + h * cp,
        }
    }

    /// From a [`GeoLocation`].
    pub fn from_location(loc: &GeoLocation) -> Self {
        Self::new(loc.lat_deg * DEG, loc.elevation_m)
    }

    /// Geocentric latitude φ′, radians.
    pub fn geocentric_latitude(&self) -> f64 {
        self.rho_sin_phi.atan2(self.rho_cos_phi)
    }

    /// Distance from the Earth's centre, km.
    pub fn radius_km(&self) -> f64 {
        self.rho_sin_phi.hypot(self.rho_cos_phi) * EARTH_EQUATORIAL_RADIUS_KM
    }
}

/// Topocentric equatorial coordinates from geocentric ones.
///
/// `geo` must be referred to the true equator of date with a finite
/// `distance_km`; `lst` is the observer's local **apparent** sidereal time.
/// The result's `distance_km` is the observer-to-body distance. Polar motion
/// is neglected.
pub fn topocentric(geo: &Equatorial, obs: &ObserverGeocentric, lst: f64) -> Equatorial {
    let r = from_spherical(geo.ra, geo.dec, geo.distance_km);
    let a = EARTH_EQUATORIAL_RADIUS_KM;
    let (sl, cl) = lst.sin_cos();
    let o = [
        a * obs.rho_cos_phi * cl,
        a * obs.rho_cos_phi * sl,
        a * obs.rho_sin_phi,
    ];
    let (ra, dec, distance_km) = to_spherical(&[r[0] - o[0], r[1] - o[1], r[2] - o[2]]);
    Equatorial {
        ra,
        dec,
        distance_km,
    }
}

/// Convenience wrapper of [`topocentric`] taking a [`GeoLocation`].
pub fn topocentric_for(geo: &Equatorial, loc: &GeoLocation, lst: f64) -> Equatorial {
    topocentric(geo, &ObserverGeocentric::from_location(loc), lst)
}

/// Equatorial horizontal parallax for a geocentric distance (km), radians:
/// `sin π = a / Δ` (Meeus ch. 40 and 47).
#[inline]
pub fn horizontal_parallax(distance_km: f64) -> f64 {
    (EARTH_EQUATORIAL_RADIUS_KM / distance_km).asin()
}

/// The Moon's semi-diameter seen from a point at `distance_km` (radians):
/// `sin s = k · a / Δ` (Meeus ch. 55). Pass the geocentric distance for the
/// geocentric semi-diameter, or the topocentric distance (from
/// [`topocentric`]) for the topocentric one; this is the exact form of
/// Meeus' augmentation formula.
#[inline]
pub fn moon_semidiameter(distance_km: f64) -> f64 {
    (MOON_RADIUS_RATIO * EARTH_EQUATORIAL_RADIUS_KM / distance_km).asin()
}

/// The Sun's semi-diameter for a distance in km, radians (Meeus ch. 55).
#[inline]
pub fn sun_semidiameter(distance_km: f64) -> f64 {
    SUN_SEMIDIAMETER_1AU_ARCSEC * ARCSEC * AU_KM / distance_km
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hms(h: f64, m: f64, s: f64) -> f64 {
        (h + m / 60.0 + s / 3600.0) * 15.0 * DEG
    }

    #[test]
    fn meeus_11a() {
        // Palomar: φ = +33°21′22″, H = 1706 m → ρ sin φ′ = 0.546861,
        // ρ cos φ′ = 0.836339 (Meeus' IAU 1976 ellipsoid; WGS84 differs by
        // < 1e-6 here).
        let lat = (33.0 + 21.0 / 60.0 + 22.0 / 3600.0) * DEG;
        let o = ObserverGeocentric::new(lat, 1706.0);
        assert!((o.rho_sin_phi - 0.546_861).abs() < 2e-6);
        assert!((o.rho_cos_phi - 0.836_339).abs() < 2e-6);
    }

    #[test]
    fn meeus_40a() {
        // Mars from Palomar, 2003 August 28, 3h17m UT: α = 339.530208°,
        // δ = −15.771083°, Δ = 0.37276 AU, apparent sidereal time at
        // Greenwich 1h40m45s, L = +7h47m27s (west).
        // Result: α′ = 22h38m08.54s, δ′ = −15°46′30.0″.
        let lat = (33.0 + 21.0 / 60.0 + 22.0 / 3600.0) * DEG;
        let o = ObserverGeocentric::new(lat, 1706.0);
        let lst = hms(1.0, 40.0, 45.0) - hms(7.0, 47.0, 27.0);
        let geo = Equatorial {
            ra: 339.530_208 * DEG,
            dec: -15.771_083 * DEG,
            distance_km: 0.37276 * AU_KM,
        };
        let t = topocentric(&geo, &o, lst);
        assert!((t.ra - hms(22.0, 38.0, 8.54)).abs() < 0.01 * 15.0 * ARCSEC);
        let dec = -(15.0 + 46.0 / 60.0 + 30.0 / 3600.0) * DEG;
        assert!(
            (t.dec - dec).abs() < 0.1 * ARCSEC,
            "{}",
            (t.dec - dec) / ARCSEC
        );
    }

    #[test]
    fn moon_parallax_and_semidiameter() {
        // Meeus 47.a: Δ = 368409.7 km → π = 0.991990°.
        assert!((horizontal_parallax(368_409.7) / DEG - 0.991_990).abs() < 2e-6);
        // Meeus ch. 55: s = 358473400″ / Δ, to the approximation sin x = x.
        let s = moon_semidiameter(384_400.0) / ARCSEC;
        assert!((s - 358_473_400.0 / 384_400.0).abs() < 0.1, "{s}");
        // The Sun at 1 AU.
        assert!((sun_semidiameter(AU_KM) / ARCSEC - 959.63).abs() < 1e-9);
    }

    #[test]
    fn zenith_moon_is_closer() {
        // Moon at the observer's zenith: topocentric distance ≈ Δ − ρa.
        let o = ObserverGeocentric::new(0.0, 0.0);
        let geo = Equatorial {
            ra: 1.0,
            dec: 0.0,
            distance_km: 384_400.0,
        };
        let t = topocentric(&geo, &o, 1.0);
        assert!((t.distance_km - (384_400.0 - EARTH_EQUATORIAL_RADIUS_KM)).abs() < 1e-6);
        assert!((t.ra - 1.0).abs() < 1e-12);
    }
}
