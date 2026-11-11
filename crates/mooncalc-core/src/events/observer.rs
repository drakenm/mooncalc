//! An observer on the rotating Earth, precomputed for repeated evaluation.
//!
//! Horizontal coordinates are computed with vectors: the observer's local
//! up/north/east unit vectors (geodetic latitude, WGS84) and geocentric
//! position at a given sidereal time. That is equivalent to Meeus,
//! *Astronomical Algorithms* 2nd ed., eqs. 13.5–13.6 (horizontal
//! coordinates) and ch. 40 (topocentric parallax), and needs only one
//! `sin_cos` per instant, which matters inside root finding.

use crate::angle::{DEG, norm_2pi};
use crate::coords::GeoLocation;
use crate::matrix::Vec3;
use crate::topocentric::{EARTH_EQUATORIAL_RADIUS_KM, ObserverGeocentric};

use super::source::{Body, PositionSource};

/// A [`GeoLocation`] with its trigonometry and geocentric position cached.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Observer {
    pub location: GeoLocation,
    /// East longitude, radians.
    pub lon: f64,
    /// Geodetic latitude, radians.
    pub lat: f64,
    sin_lat: f64,
    cos_lat: f64,
    /// `a ρ cos φ′`, km.
    rho_cos_km: f64,
    /// `a ρ sin φ′`, km.
    rho_sin_km: f64,
}

impl Observer {
    pub fn new(location: &GeoLocation) -> Self {
        let lat = location.lat_deg * DEG;
        let g = ObserverGeocentric::new(lat, location.elevation_m);
        let (sin_lat, cos_lat) = lat.sin_cos();
        Self {
            location: *location,
            lon: location.lon_deg * DEG,
            lat,
            sin_lat,
            cos_lat,
            rho_cos_km: g.rho_cos_phi * EARTH_EQUATORIAL_RADIUS_KM,
            rho_sin_km: g.rho_sin_phi * EARTH_EQUATORIAL_RADIUS_KM,
        }
    }

    /// The local frame at Greenwich apparent sidereal time `gast` (radians).
    #[inline]
    pub fn frame(&self, gast: f64) -> LocalFrame {
        let (st, ct) = (gast + self.lon).sin_cos();
        let (sp, cp) = (self.sin_lat, self.cos_lat);
        LocalFrame {
            lst: gast + self.lon,
            up: [cp * ct, cp * st, sp],
            north: [-sp * ct, -sp * st, cp],
            east: [-st, ct, 0.0],
            position: [self.rho_cos_km * ct, self.rho_cos_km * st, self.rho_sin_km],
        }
    }

    /// The local frame at UT Julian Day `jd_ut`, using `src`'s sidereal time.
    #[inline]
    pub fn frame_at<S: PositionSource + ?Sized>(&self, src: &S, jd_ut: f64) -> LocalFrame {
        self.frame(src.gast(jd_ut))
    }

    /// Topocentric airless state of `body` at `jd_ut`.
    #[inline]
    pub fn look<S: PositionSource + ?Sized>(&self, src: &S, body: Body, jd_ut: f64) -> Look {
        let frame = self.frame_at(src, jd_ut);
        frame.look(&src.position(body, jd_ut))
    }
}

/// An observer's local frame at one sidereal time. Vectors are in the true
/// equator and equinox of date, km for positions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalFrame {
    /// Local apparent sidereal time, radians (not normalised).
    pub lst: f64,
    pub up: Vec3,
    pub north: Vec3,
    pub east: Vec3,
    /// The observer's geocentric position, km.
    pub position: Vec3,
}

#[inline]
fn dot(a: &Vec3, b: &Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn norm(a: &Vec3) -> f64 {
    dot(a, a).sqrt()
}

impl LocalFrame {
    /// Altitude (radians) of direction `v`, which need not be a unit vector.
    #[inline]
    pub fn altitude(&self, v: &Vec3) -> f64 {
        (dot(v, &self.up) / norm(v)).clamp(-1.0, 1.0).asin()
    }

    /// Azimuth (radians, north through east, `[0, 2π)`) of direction `v`.
    #[inline]
    pub fn azimuth(&self, v: &Vec3) -> f64 {
        norm_2pi(dot(v, &self.east).atan2(dot(v, &self.north)))
    }

    /// Topocentric vector of a geocentric position `geo` (km).
    #[inline]
    pub fn topocentric(&self, geo: &Vec3) -> Vec3 {
        [
            geo[0] - self.position[0],
            geo[1] - self.position[1],
            geo[2] - self.position[2],
        ]
    }

    /// Hour angle (radians, `(−π, π]`, west positive) of geocentric `geo`.
    #[inline]
    pub fn hour_angle(&self, geo: &Vec3) -> f64 {
        crate::angle::norm_pi(self.lst - geo[1].atan2(geo[0]))
    }

    /// Topocentric airless altitude, distance and geocentric declination of
    /// a body at geocentric position `geo`.
    #[inline]
    pub fn look(&self, geo: &Vec3) -> Look {
        let t = self.topocentric(geo);
        let d = norm(&t);
        Look {
            alt: (dot(&t, &self.up) / d).clamp(-1.0, 1.0).asin(),
            distance_km: d,
            hour_angle: self.hour_angle(geo),
            dec: (geo[2] / norm(geo)).clamp(-1.0, 1.0).asin(),
        }
    }
}

/// The minimal per-instant state used by rise/set searches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Topocentric airless altitude of the centre, radians.
    pub alt: f64,
    /// Topocentric distance, km.
    pub distance_km: f64,
    /// Geocentric hour angle, radians in `(−π, π]`.
    pub hour_angle: f64,
    /// Geocentric declination, radians.
    pub dec: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::Equatorial;
    use crate::matrix::from_spherical;
    use crate::topocentric::topocentric;
    use crate::transform::equatorial_to_horizontal;

    #[test]
    fn matches_transform_and_topocentric() {
        let loc = GeoLocation::new(-33.9, 18.4, 1500.0);
        let obs = Observer::new(&loc);
        let gast = 1.234;
        let frame = obs.frame(gast);
        let lst = gast + obs.lon;
        let eq = Equatorial {
            ra: 0.9,
            dec: -0.2,
            distance_km: 380_000.0,
        };
        let v = from_spherical(eq.ra, eq.dec, eq.distance_km);
        // Geocentric direction, no parallax.
        let h = equatorial_to_horizontal(&eq, obs.lat, lst);
        assert!((frame.altitude(&v) - h.alt).abs() < 1e-12);
        assert!((frame.azimuth(&v) - h.az).abs() < 1e-12);
        // Topocentric.
        let topo = topocentric(&eq, &ObserverGeocentric::from_location(&loc), lst);
        let ht = equatorial_to_horizontal(&topo, obs.lat, lst);
        let t = frame.topocentric(&v);
        assert!((frame.altitude(&t) - ht.alt).abs() < 1e-12);
        assert!((frame.azimuth(&t) - ht.az).abs() < 1e-12);
        let look = frame.look(&v);
        assert!((look.alt - ht.alt).abs() < 1e-12);
        assert!((look.distance_km - topo.distance_km).abs() < 1e-6);
        assert!((look.dec - eq.dec).abs() < 1e-12);
        assert!((look.hour_angle - crate::angle::norm_pi(lst - eq.ra)).abs() < 1e-12);
    }
}
