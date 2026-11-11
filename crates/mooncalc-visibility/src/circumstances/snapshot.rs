//! [`CrescentGeometry`] at one instant for one observer.

use mooncalc_core::angle::DEG;
use mooncalc_core::events::{Body, Observer, PositionSource};
use mooncalc_core::illumination::{illuminated_fraction, phase_angle};
use mooncalc_core::matrix::Vec3;
use mooncalc_core::refraction::{Atmosphere, apparent_altitude};
use mooncalc_core::topocentric::EARTH_EQUATORIAL_RADIUS_KM;

use crate::geometry::CrescentGeometry;

/// Yallop (1997) eq. 3.8: the Moon's semi-diameter is `0.27245 π`.
pub const YALLOP_SD_PER_PARALLAX: f64 = 0.27245;

#[inline]
fn dot(a: &Vec3, b: &Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn norm(a: &Vec3) -> f64 {
    dot(a, a).sqrt()
}

/// Angle between two vectors, radians (stable for small angles).
#[inline]
fn angle_between(a: &Vec3, b: &Vec3) -> f64 {
    let c = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    norm(&c).atan2(dot(a, b))
}

/// Fill a [`CrescentGeometry`] at UT Julian Day `jd_ut`.
///
/// `lag_minutes` and `age_hours` are copied in (they depend on events, not
/// on the instant). Definitions, all at `jd_ut`:
///
/// * Geocentric quantities use the geocentric apparent directions of the
///   Sun and Moon, referred to the observer's horizon (geodetic zenith):
///   ARCL (`arcl_geo_deg`) is the angle at the Earth's centre between the
///   centres; ARCV (`arcv_deg`) is the geocentric Moon altitude minus the
///   geocentric Sun altitude, airless; DAZ (`daz_deg`) is the Sun's minus
///   the Moon's geocentric azimuth, in `(−180°, 180°]`. These are Yallop's
///   §2 definitions (NAO TN 69).
/// * `width_geo_arcmin` is `W = SD (1 − cos ARCL)` with `SD = 0.27245 π`
///   (eqs. 3.8, 3.10 without the dash), and `width_topo_arcmin` is
///   `W′ = SD′ (1 − cos ARCL)` with `SD′ = SD (1 + sin h sin π)`, `h` the
///   geocentric Moon altitude (eqs. 3.9–3.10; ARCL geocentric).
///   `semi_diameter_topo_arcmin` is that `SD′`. Yallop's own Table 4 W′
///   values confirm the geocentric ARCL (median residual 0.003′). Odeh
///   (2004) instead uses the topocentric elongation,
///   `W = SD′ (1 − cos ARCL′)` = `semi_diameter_topo_arcmin × (1 − cos
///   arcl_deg)`, about 2.5″ smaller near the Danjon limit (0.25 in V).
/// * Topocentric quantities use exact topocentric vectors (WGS84 observer,
///   elevation included): `arcl_deg` is the topocentric airless elongation;
///   `arcv_topo_deg` the topocentric airless Moon altitude minus the
///   topocentric airless Sun altitude (Odeh 2004, "airless and
///   topocentric"); `moon_alt_airless_deg`; `moon_az_deg`, `sun_az_deg`.
/// * `moon_alt_deg` and `sun_alt_deg` are apparent: the topocentric airless
///   altitude plus Sæmundsson's refraction for `atm` (Meeus eq. 16.4). The
///   formula is held at its −1° value below −1°, so apparent altitudes of
///   a body well below the horizon are nominal only.
/// * `illuminated_fraction` is topocentric (Meeus eq. 48.1 with the
///   observer's distances); `moon_distance_km` is geocentric.
pub fn geometry_at<S: PositionSource + ?Sized>(
    src: &S,
    obs: &Observer,
    jd_ut: f64,
    lag_minutes: f64,
    age_hours: f64,
    atm: &Atmosphere,
) -> CrescentGeometry {
    let frame = obs.frame_at(src, jd_ut);
    let sun = src.position(Body::Sun, jd_ut);
    let moon = src.position(Body::Moon, jd_ut);
    let sun_t = frame.topocentric(&sun);
    let moon_t = frame.topocentric(&moon);

    let hs_geo = frame.altitude(&sun);
    let hm_geo = frame.altitude(&moon);
    let as_geo = frame.azimuth(&sun);
    let am_geo = frame.azimuth(&moon);
    let hs_topo = frame.altitude(&sun_t);
    let hm_topo = frame.altitude(&moon_t);

    let arcl_geo = angle_between(&sun, &moon);
    let arcl_topo = angle_between(&sun_t, &moon_t);
    let moon_dist = norm(&moon);
    let parallax = (EARTH_EQUATORIAL_RADIUS_KM / moon_dist).asin();
    let sd = YALLOP_SD_PER_PARALLAX * parallax;
    let sd_topo = sd * (1.0 + hm_geo.sin() * parallax.sin());
    let one_minus_cos = 1.0 - arcl_geo.cos();
    let to_arcmin = 60.0 / DEG;
    let phase = phase_angle(arcl_topo, norm(&sun_t), norm(&moon_t));

    CrescentGeometry {
        arcl_deg: arcl_topo / DEG,
        arcl_geo_deg: arcl_geo / DEG,
        arcv_deg: (hm_geo - hs_geo) / DEG,
        daz_deg: mooncalc_core::angle::norm_pi(as_geo - am_geo) / DEG,
        width_topo_arcmin: sd_topo * one_minus_cos * to_arcmin,
        width_geo_arcmin: sd * one_minus_cos * to_arcmin,
        semi_diameter_topo_arcmin: sd_topo * to_arcmin,
        moon_alt_deg: apparent_altitude(hm_topo, atm) / DEG,
        moon_alt_airless_deg: hm_topo / DEG,
        sun_alt_deg: apparent_altitude(hs_topo, atm) / DEG,
        moon_az_deg: frame.azimuth(&moon_t) / DEG,
        sun_az_deg: frame.azimuth(&sun_t) / DEG,
        lag_minutes,
        age_hours,
        illuminated_fraction: illuminated_fraction(phase),
        moon_distance_km: moon_dist,
        arcv_topo_deg: (hm_topo - hs_topo) / DEG,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::GeoLocation;
    use mooncalc_core::events::DirectSource;
    use mooncalc_core::illumination::Illumination;
    use mooncalc_core::time::Instant;
    use mooncalc_core::topocentric::topocentric_for;
    use mooncalc_core::transform::equatorial_to_horizontal;
    use mooncalc_core::{Ephemeris, MeeusEphemeris};

    #[test]
    fn consistent_with_core_transforms() {
        let eph = MeeusEphemeris::new();
        let src = DirectSource::new(&eph);
        let loc = GeoLocation::new(21.4225, 39.8262, 277.0);
        let obs = Observer::new(&loc);
        let t = Instant::from_calendar(2024, 3, 11, 15, 40, 0.0);
        let g = geometry_at(&src, &obs, t.jd_ut, 30.0, 25.0, &Atmosphere::STANDARD);
        let jd_tt = t.jd_tt();
        let (sun, moon) = (eph.sun_equatorial(jd_tt), eph.moon_equatorial(jd_tt));
        let lst = t.local_apparent_sidereal_time(loc.lon_deg * DEG);
        let lat = loc.lat_deg * DEG;
        let hs = equatorial_to_horizontal(&sun, lat, lst);
        let hm = equatorial_to_horizontal(&moon, lat, lst);
        let tol = 1e-7;
        assert!((g.arcv_deg - (hm.alt - hs.alt) / DEG).abs() < tol);
        let daz = mooncalc_core::angle::norm_pi(hs.az - hm.az) / DEG;
        assert!((g.daz_deg - daz).abs() < tol);
        let ill_geo = Illumination::compute(&sun, &moon);
        assert!((g.arcl_geo_deg - ill_geo.elongation / DEG).abs() < tol);
        let mt = topocentric_for(&moon, &loc, lst);
        let st = topocentric_for(&sun, &loc, lst);
        let hmt = equatorial_to_horizontal(&mt, lat, lst);
        let hst = equatorial_to_horizontal(&st, lat, lst);
        assert!((g.moon_alt_airless_deg - hmt.alt / DEG).abs() < tol);
        assert!((g.arcv_topo_deg - (hmt.alt - hst.alt) / DEG).abs() < tol);
        assert!((g.moon_az_deg - hmt.az / DEG).abs() < tol);
        let ill_topo = Illumination::compute(&st, &mt);
        assert!((g.arcl_deg - ill_topo.elongation / DEG).abs() < tol);
        assert!((g.illuminated_fraction - ill_topo.fraction).abs() < 1e-9);
        // The Moon is lower topocentrically, by about its parallax × cos h.
        assert!(g.arcv_topo_deg < g.arcv_deg - 0.8);
        assert!(g.moon_alt_deg > g.moon_alt_airless_deg);
        assert!(g.width_topo_arcmin > g.width_geo_arcmin && g.width_geo_arcmin > 0.0);
        assert_eq!((g.lag_minutes, g.age_hours), (30.0, 25.0));
        assert!((g.moon_distance_km - moon.distance_km).abs() < 1e-6);
    }
}
