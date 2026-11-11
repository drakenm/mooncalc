//! Pure geometry for the sky views: projections, screen directions of the
//! crescent and sky colours. Nothing here draws, so all of it is
//! unit-tested. The frontend-independent angles (parallactic angle, bright
//! limb from the zenith, crescent outline, phase and compass names) are
//! re-exported from [`mooncalc_app::sky::geometry`].
//!
//! # Orientation on screen
//!
//! Every view is drawn **as seen on the sky** (not mirrored): position
//! angles, measured from north through east, turn counter-clockwise on the
//! screen. A direction on screen is a [`Vec2`] with `y` pointing down.

use egui::{Pos2, Vec2, pos2, vec2};
use mooncalc_core::angle::{DEG, norm_2pi, norm_pi};

// The frontend-independent part lives in `mooncalc-app`, shared with the
// Tauri frontend.
pub use mooncalc_app::sky::geometry::{
    CrescentRow, bearing_from_zenith, compass_point, crescent_rows, parallactic_angle, phase_name,
    shadow_length, zenith_limb_angle,
};

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
    use mooncalc_core::transform::hour_angle;
    use std::f64::consts::{FRAC_PI_2, PI};

    fn close(a: f64, b: f64, tol: f64) -> bool {
        norm_pi(a - b).abs() < tol
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
    fn rotation_is_counter_clockwise_on_screen() {
        let up = vec2(0.0, -1.0);
        assert!((rotate_ccw(up, FRAC_PI_2) - vec2(-1.0, 0.0)).length() < 1e-6);
        assert!((rotate_ccw(up, PI) - vec2(0.0, 1.0)).length() < 1e-6);
        assert!((rotate_ccw(vec2(1.0, 0.0), FRAC_PI_2) - up).length() < 1e-6);
    }

    #[test]
    fn crescent_frame_to_screen() {
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
    fn star_sizes() {
        assert!(star_radius(-1.46) > star_radius(3.4));
    }
}
