//! Pure geometry for the sky views: projections, the orientation of the
//! crescent, sky colours and small derived quantities. Nothing here draws,
//! so all of it is unit-tested.
//!
//! # Orientation on screen
//!
//! Every view is drawn **as seen on the sky** (not mirrored): position
//! angles, measured from north through east, turn counter-clockwise on the
//! screen. A direction on screen is a [`Vec2`] with `y` pointing down.

use egui::{Pos2, Vec2, pos2, vec2};
use mooncalc_core::angle::{DEG, norm_2pi, norm_pi};

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

/// Rotate a screen direction counter-clockwise (as seen on screen, `y`
/// down) by `angle` radians.
pub fn rotate_ccw(v: Vec2, angle: f64) -> Vec2 {
    let (s, c) = (angle.sin() as f32, angle.cos() as f32);
    vec2(v.x * c + v.y * s, -v.x * s + v.y * c)
}

/// The screen direction of the bright limb, given the screen direction of
/// the reference point of the position angle (`reference`: the zenith for
/// [`zenith_limb_angle`], north for `χ`) and the angle itself.
pub fn limb_direction(reference: Vec2, angle: f64) -> Vec2 {
    rotate_ccw(reference.normalized(), angle)
}

/// Bearing of point `b` seen from point `a` on the sphere, both given as
/// (altitude, azimuth), measured from the direction to the zenith and
/// positive toward increasing azimuth, radians in `(−π, π]`. On the sky
/// (seen from inside, not mirrored), increasing azimuth is to the right of
/// "up" when facing the horizon, so this is a **clockwise** angle from up.
///
/// It is the standard initial great-circle bearing with altitude as
/// latitude, mirrored; it gives an independent check of
/// `χ − q` in the tests.
pub fn bearing_from_zenith(alt_a: f64, az_a: f64, alt_b: f64, az_b: f64) -> f64 {
    let d = az_b - az_a;
    let y = d.sin() * alt_b.cos();
    let x = alt_a.cos() * alt_b.sin() - alt_a.sin() * alt_b.cos() * d.cos();
    // atan2(y, x) is the azimuth-like bearing from "up" toward increasing
    // azimuth.
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

/// Map a point of the Moon's frame (unit radius) to the screen.
pub fn moon_frame_to_screen(center: Pos2, radius: f32, limb_dir: Vec2, p: [f64; 2]) -> Pos2 {
    let u = limb_dir.normalized();
    // `v` is `u` turned 90° counter-clockwise on screen.
    let v = vec2(u.y, -u.x);
    center + radius * (u * p[0] as f32 + v * p[1] as f32)
}

/// Equirectangular panorama of the sky: azimuth increases to the right,
/// altitude upward. Angles in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizonProjection {
    /// Azimuth at `center_x`.
    pub center_az: f64,
    pub center_x: f32,
    /// Screen `y` of the horizon (altitude 0).
    pub horizon_y: f32,
    /// Scale, pixels per radian.
    pub px_per_rad: f32,
}

impl HorizonProjection {
    pub fn project(&self, alt: f64, az: f64) -> Pos2 {
        pos2(
            self.center_x + (norm_pi(az - self.center_az) as f32) * self.px_per_rad,
            self.horizon_y - (alt as f32) * self.px_per_rad,
        )
    }

    /// (altitude, azimuth in `[0, 2π)`) at a screen point.
    pub fn unproject(&self, p: Pos2) -> (f64, f64) {
        let az = self.center_az + f64::from((p.x - self.center_x) / self.px_per_rad);
        let alt = f64::from((self.horizon_y - p.y) / self.px_per_rad);
        (alt, norm_2pi(az))
    }

    /// Screen direction toward the zenith (always straight up).
    pub fn zenith_direction(&self) -> Vec2 {
        vec2(0.0, -1.0)
    }
}

/// Azimuthal equidistant all-sky view: the zenith at the centre, the
/// horizon on a circle, north up and east to the **left** (the sky seen
/// from below, like a planisphere).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AllSkyProjection {
    pub center: Pos2,
    /// Radius of the horizon circle, pixels.
    pub radius: f32,
}

impl AllSkyProjection {
    pub fn project(&self, alt: f64, az: f64) -> Pos2 {
        let r = self.radius
            * ((std::f64::consts::FRAC_PI_2 - alt) / std::f64::consts::FRAC_PI_2) as f32;
        let (s, c) = (az.sin() as f32, az.cos() as f32);
        pos2(self.center.x - r * s, self.center.y - r * c)
    }

    /// Screen direction toward the zenith from `p` (toward the centre).
    pub fn zenith_direction(&self, p: Pos2) -> Vec2 {
        let d = self.center - p;
        if d.length_sq() < 1e-6 {
            vec2(0.0, -1.0)
        } else {
            d.normalized()
        }
    }
}

/// Polar "compass" diagram seen from **above** (a map): north up, east to
/// the right, the zenith at the centre and the horizon on the circle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompassProjection {
    pub center: Pos2,
    pub radius: f32,
}

impl CompassProjection {
    pub fn project(&self, alt: f64, az: f64) -> Pos2 {
        let r = self.radius
            * ((std::f64::consts::FRAC_PI_2 - alt.max(0.0)) / std::f64::consts::FRAC_PI_2) as f32;
        let (s, c) = (az.sin() as f32, az.cos() as f32);
        pos2(self.center.x + r * s, self.center.y - r * c)
    }

    /// The point on the horizon circle (scaled by `scale`) at azimuth `az`.
    pub fn rim(&self, az: f64, scale: f32) -> Pos2 {
        let (s, c) = (az.sin() as f32, az.cos() as f32);
        pos2(
            self.center.x + scale * self.radius * s,
            self.center.y - scale * self.radius * c,
        )
    }
}

/// Plate carrée star chart: right ascension increases to the **left** and
/// declination upward (north up, east left, as seen on the sky).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StarChartProjection {
    pub center: Pos2,
    /// Right ascension at the centre, radians.
    pub ra0: f64,
    /// Declination at the centre, radians.
    pub dec0: f64,
    pub px_per_rad: f32,
}

impl StarChartProjection {
    pub fn project(&self, ra: f64, dec: f64) -> Pos2 {
        pos2(
            self.center.x - (norm_pi(ra - self.ra0) as f32) * self.px_per_rad,
            self.center.y - ((dec - self.dec0) as f32) * self.px_per_rad,
        )
    }

    /// Screen direction of north (straight up).
    pub fn north_direction(&self) -> Vec2 {
        vec2(0.0, -1.0)
    }
}

/// Linear interpolation of colours.
fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// An illustrative sky colour (sRGB components in 0–1) at a point of the
/// sky at altitude `alt_deg`, `sep_deg` from the Sun, when the Sun is at
/// `sun_alt_deg`.
///
/// Not a radiative-transfer model: a blend between day, twilight and night
/// palettes keyed on the Sun's altitude (civil −6°, nautical −12°,
/// astronomical −18°), with a warm glow toward the Sun near the horizon
/// during twilight.
pub fn sky_color(sun_alt_deg: f64, alt_deg: f64, sep_deg: f64) -> [f32; 3] {
    let s = sun_alt_deg as f32;
    let h = (alt_deg as f32).clamp(0.0, 90.0);
    // The horizon is brightest toward the Sun; away from it the sky stays
    // closer to the zenith colour down to the horizon.
    let away = smoothstep(60.0, 170.0, sep_deg as f32);
    let zen_t = smoothstep(0.0, 60.0, h).max(0.65 * away);
    // Zenith and horizon colours of each phase.
    let day = mix([0.62, 0.78, 0.95], [0.22, 0.47, 0.86], zen_t);
    let civil = mix([0.55, 0.52, 0.62], [0.13, 0.22, 0.45], zen_t);
    let nautical = mix([0.12, 0.15, 0.30], [0.04, 0.07, 0.18], zen_t);
    let night = mix([0.03, 0.04, 0.09], [0.01, 0.015, 0.045], zen_t);
    let mut c = if s >= -6.0 {
        mix(civil, day, smoothstep(-2.0, 6.0, s))
    } else if s >= -12.0 {
        mix(nautical, civil, smoothstep(-12.0, -6.0, s))
    } else {
        mix(night, nautical, smoothstep(-18.0, -12.0, s))
    };
    // Twilight glow: strongest with the Sun just below the horizon.
    let glow_strength = smoothstep(-16.0, -3.0, s) * (1.0 - smoothstep(2.0, 10.0, s));
    let glow = glow_strength * (-(sep_deg as f32) / 35.0).exp() * (-h / 12.0).exp();
    c = mix(c, [1.0, 0.55, 0.22], glow * 0.85);
    c
}

/// Opacity factor (0–1) for stars against a sky with the Sun at
/// `sun_alt_deg`: none in daylight, full in astronomical night.
pub fn star_visibility(sun_alt_deg: f64) -> f32 {
    1.0 - smoothstep(-16.0, -4.0, sun_alt_deg as f32)
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

/// Star symbol radius in pixels for a V magnitude.
pub fn star_radius(vmag: f64) -> f32 {
    (3.6 - 0.75 * vmag as f32).clamp(0.9, 4.8)
}

/// An approximate star colour from the B−V index (blue-white to orange).
pub fn star_tint(bv: f64) -> [f32; 3] {
    let t = ((bv as f32 + 0.3) / 2.0).clamp(0.0, 1.0);
    mix([0.75, 0.85, 1.0], [1.0, 0.78, 0.52], t)
}

/// Degrees per radian, for formatting.
pub const DEG_PER_RAD: f64 = 1.0 / DEG;

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::Equatorial;
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
    fn evening_crescent_at_the_equator_faces_the_sun() {
        // Equinox, equator: the Sun just set due west, the Moon 15° east of
        // it on the celestial equator, i.e. 15° straight above it. The lit
        // limb must point straight down, toward the Sun.
        let lat = 0.0;
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
        assert!(close(chi, 1.5 * PI, 1e-9), "χ = {}", chi / DEG); // due west
        let q = parallactic_angle(hour_angle(lst, moon.ra), moon.dec, lat);
        let zeta = zenith_limb_angle(chi, q);
        assert!(close(zeta, PI, 1e-9), "χ − q = {}", zeta / DEG);
        let dir = limb_direction(vec2(0.0, -1.0), zeta);
        assert!((dir - vec2(0.0, 1.0)).length() < 1e-6, "{dir:?}");
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
    fn rotation_is_counter_clockwise_on_screen() {
        let up = vec2(0.0, -1.0);
        assert!((rotate_ccw(up, FRAC_PI_2) - vec2(-1.0, 0.0)).length() < 1e-6);
        assert!((rotate_ccw(up, PI) - vec2(0.0, 1.0)).length() < 1e-6);
        assert!((rotate_ccw(vec2(1.0, 0.0), FRAC_PI_2) - up).length() < 1e-6);
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
        // Mapping to screen: u along the limb direction, v 90° CCW.
        let p = moon_frame_to_screen(pos2(10.0, 10.0), 5.0, vec2(0.0, 1.0), [1.0, 0.0]);
        assert_eq!(p, pos2(10.0, 15.0));
        let q = moon_frame_to_screen(pos2(10.0, 10.0), 5.0, vec2(0.0, 1.0), [0.0, 1.0]);
        assert_eq!(q, pos2(15.0, 10.0));
    }

    #[test]
    fn horizon_projection() {
        let p = HorizonProjection {
            center_az: 270.0 * DEG,
            center_x: 400.0,
            horizon_y: 300.0,
            px_per_rad: 10.0 / DEG as f32,
        };
        // The centre azimuth on the horizon maps to the anchor.
        let a = p.project(0.0, 270.0 * DEG);
        assert!((a - pos2(400.0, 300.0)).length() < 1e-3);
        // Azimuth increases to the right (north of west is right), altitude up.
        let b = p.project(5.0 * DEG, 280.0 * DEG);
        assert!((b - pos2(500.0, 250.0)).length() < 1e-2, "{b:?}");
        // Wrap-around through north.
        let p2 = HorizonProjection {
            center_az: 0.0,
            ..p
        };
        let c = p2.project(0.0, 350.0 * DEG);
        assert!((c.x - 300.0).abs() < 1e-2, "{c:?}");
        // Below the horizon is below the line.
        assert!(p.project(-3.0 * DEG, 270.0 * DEG).y > 300.0);
        // Round trip.
        let (alt, az) = p.unproject(b);
        assert!((alt - 5.0 * DEG).abs() < 1e-6 && (az - 280.0 * DEG).abs() < 1e-6);
    }

    #[test]
    fn all_sky_and_compass_projections() {
        let s = AllSkyProjection {
            center: pos2(100.0, 100.0),
            radius: 90.0,
        };
        assert_eq!(s.project(FRAC_PI_2, 1.0), pos2(100.0, 100.0));
        // North up, east left, south down, west right.
        assert!((s.project(0.0, 0.0) - pos2(100.0, 10.0)).length() < 1e-4);
        assert!((s.project(0.0, FRAC_PI_2) - pos2(10.0, 100.0)).length() < 1e-4);
        assert!((s.project(0.0, PI) - pos2(100.0, 190.0)).length() < 1e-4);
        assert!((s.project(45.0 * DEG, 1.5 * PI) - pos2(145.0, 100.0)).length() < 1e-4);
        assert!((s.zenith_direction(pos2(145.0, 100.0)) - vec2(-1.0, 0.0)).length() < 1e-6);
        // The compass is seen from above: east to the right.
        let c = CompassProjection {
            center: pos2(0.0, 0.0),
            radius: 10.0,
        };
        assert!((c.project(0.0, FRAC_PI_2) - pos2(10.0, 0.0)).length() < 1e-4);
        assert!((c.rim(PI, 1.0) - pos2(0.0, 10.0)).length() < 1e-4);
    }

    #[test]
    fn star_chart_projection() {
        let p = StarChartProjection {
            center: pos2(0.0, 0.0),
            ra0: 0.0,
            dec0: 0.0,
            px_per_rad: 1.0 / DEG as f32,
        };
        // RA increases to the left, Dec up, with wrap-around.
        assert!((p.project(10.0 * DEG, 20.0 * DEG) - pos2(-10.0, -20.0)).length() < 1e-3);
        assert!((p.project(350.0 * DEG, 0.0) - pos2(10.0, 0.0)).length() < 1e-3);
    }

    #[test]
    fn sky_colour_darkens_with_the_sun() {
        let lum = |c: [f32; 3]| c[0] + c[1] + c[2];
        let day = lum(sky_color(20.0, 30.0, 90.0));
        let civil = lum(sky_color(-4.0, 30.0, 90.0));
        let naut = lum(sky_color(-10.0, 30.0, 90.0));
        let night = lum(sky_color(-25.0, 30.0, 90.0));
        assert!(
            day > civil && civil > naut && naut > night,
            "{day} {civil} {naut} {night}"
        );
        // The glow: near the Sun on the horizon is redder than opposite.
        let near = sky_color(-4.0, 2.0, 5.0);
        let far = sky_color(-4.0, 2.0, 170.0);
        assert!(near[0] > far[0]);
        assert_eq!(star_visibility(5.0), 0.0);
        assert_eq!(star_visibility(-20.0), 1.0);
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
        assert!(star_radius(-1.46) > star_radius(3.4));
    }
}
