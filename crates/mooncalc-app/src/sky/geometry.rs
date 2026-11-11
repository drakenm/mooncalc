//! Frontend-independent geometry of the sky views: the orientation of the
//! crescent (parallactic angle, bright limb from the zenith), the crescent
//! outline, and small derived quantities (shadow length, phase and compass
//! names). Shared by the egui and Tauri frontends; projections and colours
//! live with each frontend's drawing code.
//!
//! Position angles are measured from north (or from the zenith) through
//! east, i.e. counter-clockwise as seen on the sky.

use mooncalc_core::angle::norm_2pi;

/// The parallactic angle `q` of an object at hour angle `ha` (west
/// positive) and declination `dec`, for an observer at geodetic latitude
/// `lat`, radians in `(−π, π]`.
///
/// `q` is the position angle of the zenith seen from the object: the angle
/// at the object between the directions to the north celestial pole and to
/// the zenith, positive when the zenith lies east of north (object west of
/// the meridian). Meeus, *Astronomical Algorithms* 2nd ed., eq. 14.1,
/// `tan q = sin H / (tan φ cos δ − sin δ cos H)`, here multiplied through by
/// `cos φ` so that it stays finite at the poles.
pub fn parallactic_angle(ha: f64, dec: f64, lat: f64) -> f64 {
    let (sh, ch) = ha.sin_cos();
    let (sd, cd) = dec.sin_cos();
    let (sp, cp) = lat.sin_cos();
    (sh * cp).atan2(sp * cd - cp * sd * ch)
}

/// Position angle of the Moon's bright limb measured from the **zenith**
/// direction (counter-clockwise as seen on the sky), radians in `[0, 2π)`:
/// `χ − q`, where `χ` is the bright limb's position angle from north
/// (Meeus eq. 48.5, [`mooncalc_core::illumination::bright_limb_position_angle`])
/// and `q` the [`parallactic_angle`]. Meeus ch. 48: "the angle χ − q is the
/// position angle of the bright limb measured from the vertical direction".
pub fn zenith_limb_angle(chi: f64, q: f64) -> f64 {
    norm_2pi(chi - q)
}

/// Bearing of point `b` seen from point `a` on the sphere, both given as
/// (altitude, azimuth), measured from the direction to the zenith and
/// positive toward increasing azimuth, radians in `(−π, π]`. On the sky
/// (seen from inside, not mirrored), increasing azimuth is to the right of
/// "up" when facing the horizon, so this is a **clockwise** angle from up.
///
/// It is the standard initial great-circle bearing with altitude as
/// latitude, mirrored; it gives an independent check of `χ − q` in the
/// tests.
pub fn bearing_from_zenith(alt_a: f64, az_a: f64, alt_b: f64, az_b: f64) -> f64 {
    let d = az_b - az_a;
    let y = d.sin() * alt_b.cos();
    let x = alt_a.cos() * alt_b.sin() - alt_a.sin() * alt_b.cos() * d.cos();
    y.atan2(x)
}

/// One step of a crescent outline in the Moon's own frame (unit radius;
/// `u` toward the bright limb, `v` perpendicular): a point on the bright
/// limb and the point of the terminator at the same height.
pub type CrescentRow = ([f64; 2], [f64; 2]);

/// The lit part of a disc of unit radius with illuminated fraction `k`, as
/// `n + 1` rows from one cusp to the other. Fill the quads between
/// consecutive rows.
///
/// The terminator is the projection of a great circle: an ellipse with
/// semi-axis `|1 − 2k|` along `u` (Meeus eq. 48.1 relates `k` to the phase
/// angle `i` by `k = (1 + cos i) / 2`, so the semi-axis is `cos i`).
pub fn crescent_rows(k: f64, n: usize) -> Vec<CrescentRow> {
    let k = k.clamp(0.0, 1.0);
    let n = n.max(2);
    (0..=n)
        .map(|j| {
            let t = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * j as f64 / n as f64;
            let (s, c) = t.sin_cos();
            ([c, s], [(1.0 - 2.0 * k) * c, s])
        })
        .collect()
}

/// Length of the shadow cast on level ground by a vertical object of
/// height `height` lit from apparent altitude `alt` (radians): `h / tan
/// alt`. `None` when the light source is not above the horizon.
pub fn shadow_length(height: f64, alt: f64) -> Option<f64> {
    (alt > 0.0).then(|| height / alt.tan())
}

/// The name of the Moon's phase for an elongation in ecliptic longitude
/// `λ☾ − λ☉` in degrees (any range). The principal phases are named within
/// ±6° of exact (about half a day); between them, the intermediate phases.
pub fn phase_name(elongation_lon_deg: f64) -> &'static str {
    let e = elongation_lon_deg.rem_euclid(360.0);
    const TOL: f64 = 6.0;
    let near = |x: f64| (e - x).abs() <= TOL;
    if e <= TOL || e >= 360.0 - TOL {
        "New Moon"
    } else if near(90.0) {
        "First quarter"
    } else if near(180.0) {
        "Full Moon"
    } else if near(270.0) {
        "Last quarter"
    } else if e < 90.0 {
        "Waxing crescent"
    } else if e < 180.0 {
        "Waxing gibbous"
    } else if e < 270.0 {
        "Waning gibbous"
    } else {
        "Waning crescent"
    }
}

/// A sixteen-point compass name for an azimuth in degrees.
pub fn compass_point(az_deg: f64) -> &'static str {
    const NAMES: [&str; 16] = [
        "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW",
        "NW", "NNW",
    ];
    NAMES[((az_deg.rem_euclid(360.0) / 22.5).round() as usize) % 16]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::Equatorial;
    use mooncalc_core::angle::{DEG, norm_pi};
    use mooncalc_core::illumination::bright_limb_position_angle;
    use mooncalc_core::transform::{equatorial_to_horizontal, hour_angle};
    use std::f64::consts::{FRAC_PI_2, PI};

    fn close(a: f64, b: f64, tol: f64) -> bool {
        norm_pi(a - b).abs() < tol
    }

    #[test]
    fn parallactic_angle_known_cases() {
        // On the meridian it is 0 (south of the zenith) or π (north of it).
        assert!(parallactic_angle(0.0, 10.0 * DEG, 40.0 * DEG).abs() < 1e-12);
        assert!(close(
            parallactic_angle(0.0, 60.0 * DEG, 40.0 * DEG),
            PI,
            1e-12
        ));
        // At the equator, for any object on the celestial equator, the
        // equator is vertical: q = ±90° (west / east of the meridian).
        assert!(close(parallactic_angle(1.0, 0.0, 0.0), FRAC_PI_2, 1e-12));
        assert!(close(parallactic_angle(-1.0, 0.0, 0.0), -FRAC_PI_2, 1e-12));
        // At the north pole the zenith is the celestial pole, so the two
        // directions coincide: q → 0, and the formula stays finite there.
        let q = parallactic_angle(0.7, 20.0 * DEG, 90.0 * DEG);
        assert!(q.abs() < 1e-12, "{q}");
        // At the south pole, the north celestial pole is straight down.
        let q = parallactic_angle(0.7, -20.0 * DEG, -90.0 * DEG);
        assert!(close(q, PI, 1e-12), "{q}");
        // Agrees with the textbook tan form of Meeus eq. 14.1.
        let (ha, dec, lat) = (0.9_f64, -0.3_f64, 0.6_f64);
        let tan_form = ha.sin().atan2(lat.tan() * dec.cos() - dec.sin() * ha.cos());
        assert!(close(parallactic_angle(ha, dec, lat), tan_form, 1e-12));
    }

    #[test]
    fn evening_crescent_at_the_equator_points_down() {
        // Equinox, equator: the Sun just set due west, the Moon 15° east of
        // it on the celestial equator, i.e. 15° straight above it. The lit
        // limb must point straight down (χ − q = 180°), toward the Sun.
        let lst = 0.0;
        let sun = Equatorial {
            ra: lst - FRAC_PI_2 - 0.01,
            dec: 0.0,
            distance_km: 1.496e8,
        };
        let moon = Equatorial {
            ra: sun.ra + 15.0 * DEG,
            dec: 0.0,
            distance_km: 384_400.0,
        };
        let chi = bright_limb_position_angle(&sun, &moon);
        assert!(close(chi, 1.5 * PI, 1e-9), "χ = {}", chi / DEG);
        let q = parallactic_angle(hour_angle(lst, moon.ra), moon.dec, 0.0);
        assert!(close(zenith_limb_angle(chi, q), PI, 1e-9));
    }

    #[test]
    fn limb_angle_matches_the_horizontal_bearing_of_the_sun() {
        // For arbitrary configurations, χ − q (counter-clockwise from the
        // zenith) must equal minus the bearing of the Sun from the Moon in
        // the horizontal frame (clockwise from the zenith).
        let cases = [
            // (lat, lst, sun ra, sun dec, moon ra, moon dec), degrees
            (51.5, 30.0, 350.0, -4.0, 5.0, 2.0),
            (21.4, 200.0, 110.0, 23.0, 125.0, 20.0),
            (-33.9, 100.0, 20.0, 8.0, 40.0, 15.0),
            (-33.9, 300.0, 230.0, -18.0, 200.0, -25.0),
            (65.0, 10.0, 280.0, -23.0, 300.0, -15.0),
        ];
        for (lat, lst, sra, sdec, mra, mdec) in cases {
            let (lat, lst) = (lat * DEG, lst * DEG);
            let sun = Equatorial {
                ra: sra * DEG,
                dec: sdec * DEG,
                distance_km: 1.496e8,
            };
            let moon = Equatorial {
                ra: mra * DEG,
                dec: mdec * DEG,
                distance_km: 384_400.0,
            };
            let chi = bright_limb_position_angle(&sun, &moon);
            let q = parallactic_angle(hour_angle(lst, moon.ra), moon.dec, lat);
            let zeta = zenith_limb_angle(chi, q);
            let hs = equatorial_to_horizontal(&sun, lat, lst);
            let hm = equatorial_to_horizontal(&moon, lat, lst);
            let bearing = bearing_from_zenith(hm.alt, hm.az, hs.alt, hs.az);
            assert!(
                close(zeta, -bearing, 1e-9),
                "lat {} : χ−q {} vs bearing {}",
                lat / DEG,
                zeta / DEG,
                bearing / DEG
            );
        }
    }

    #[test]
    fn crescent_outline() {
        // New: the terminator coincides with the limb; full: it is the
        // opposite limb; quarter: a straight line through the centre.
        for (k, x) in [(0.0, 1.0), (1.0, -1.0), (0.5, 0.0)] {
            let rows = crescent_rows(k, 8);
            assert_eq!(rows.len(), 9);
            for (limb, term) in &rows {
                assert!((limb[0] * limb[0] + limb[1] * limb[1] - 1.0).abs() < 1e-12);
                assert!((term[0] - x * limb[0]).abs() < 1e-12 && term[1] == limb[1]);
            }
        }
        // The lit width at the centre line is 2k (in radii).
        let rows = crescent_rows(0.1, 2);
        let (limb, term) = rows[1];
        assert!((limb[0] - term[0] - 0.2).abs() < 1e-12);
    }

    #[test]
    fn small_helpers() {
        assert!((shadow_length(1.0, 45.0 * DEG).unwrap() - 1.0).abs() < 1e-12);
        assert!((shadow_length(1.0, 30.0 * DEG).unwrap() - 3f64.sqrt()).abs() < 1e-12);
        assert!(shadow_length(1.0, -0.1).is_none());
        assert_eq!(phase_name(2.0), "New Moon");
        assert_eq!(phase_name(-3.0), "New Moon");
        assert_eq!(phase_name(14.0), "Waxing crescent");
        assert_eq!(phase_name(92.0), "First quarter");
        assert_eq!(phase_name(130.0), "Waxing gibbous");
        assert_eq!(phase_name(185.0), "Full Moon");
        assert_eq!(phase_name(230.0), "Waning gibbous");
        assert_eq!(phase_name(268.0), "Last quarter");
        assert_eq!(phase_name(340.0), "Waning crescent");
        assert_eq!(compass_point(0.0), "N");
        assert_eq!(compass_point(359.0), "N");
        assert_eq!(compass_point(250.0), "WSW");
        assert_eq!(compass_point(270.0), "W");
    }
}
