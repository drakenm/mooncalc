//! The Moon close-up geometry: orthographic projection of the lunar sphere
//! as seen by the observer and the illuminated hemisphere.
//!
//! Geometry (Meeus, *Astronomical Algorithms* 2nd ed., ch. 53): the centre
//! of the disc is the sub-observer point, at selenographic longitude l and
//! latitude b (the total librations). A point at selenographic longitude η
//! (east-positive, towards Mare Crisium) and latitude θ projects to
//!
//! ```text
//! x = cos θ sin(η − l)                          (towards lunar east)
//! y = sin θ cos b − cos θ sin b cos(η − l)      (towards lunar north)
//! z = sin θ sin b + cos θ cos b cos(η − l)      (towards the observer)
//! ```
//!
//! and is visible when z > 0. The lunar north pole lies at position angle
//! P on the sky (measured from celestial north through east), and the sky
//! is drawn as seen with the naked eye, north up and east to the left, so
//! the lunar frame is rotated anticlockwise by P (or by P − q to put the
//! zenith up, q being the parallactic angle, Meeus eq. 14.1). A point is
//! sunlit when its direction has a positive dot product with the
//! direction of the Sun, at selenographic longitude l₀ and latitude b₀.

/// What the disc shows: the viewing geometry, all angles in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// Selenographic longitude of the sub-observer point (libration in
    /// longitude).
    pub l: f64,
    /// Selenographic latitude of the sub-observer point.
    pub b: f64,
    /// Selenographic longitude of the Sun l₀.
    pub sun_lon: f64,
    /// Selenographic latitude of the Sun b₀.
    pub sun_lat: f64,
    /// Anticlockwise screen angle of the lunar north pole from "up": P, or
    /// P − q in the zenith frame.
    pub rotation: f64,
}

impl View {
    /// Project selenographic (η, θ) onto the disc: (right, up, towards the
    /// observer), unit radius.
    pub fn project(&self, eta: f64, theta: f64) -> (f64, f64, f64) {
        let (st, ct) = theta.sin_cos();
        let (sd, cd) = (eta - self.l).sin_cos();
        let (sb, cb) = self.b.sin_cos();
        let x = ct * sd;
        let y = st * cb - ct * sb * cd;
        let z = st * sb + ct * cb * cd;
        let (sr, cr) = self.rotation.sin_cos();
        (x * cr - y * sr, x * sr + y * cr, z)
    }

    /// The selenographic (η, θ) under disc point (`u` right, `v` up), or
    /// `None` outside the disc.
    pub fn unproject(&self, u: f64, v: f64) -> Option<(f64, f64)> {
        let (sr, cr) = self.rotation.sin_cos();
        let x = u * cr + v * sr;
        let y = -u * sr + v * cr;
        let z2 = 1.0 - x * x - y * y;
        if z2 < 0.0 {
            return None;
        }
        let z = z2.sqrt();
        let (sb, cb) = self.b.sin_cos();
        let theta = (y * cb + z * sb).clamp(-1.0, 1.0).asin();
        let eta = self.l + x.atan2(z * cb - y * sb);
        Some((eta, theta))
    }

    /// Cosine of the Sun's zenith angle at (η, θ): positive in sunlight.
    pub fn sun_dot(&self, eta: f64, theta: f64) -> f64 {
        let (st, ct) = theta.sin_cos();
        let (sb0, cb0) = self.sun_lat.sin_cos();
        st * sb0 + ct * cb0 * (eta - self.sun_lon).cos()
    }

    /// Screen angle (anticlockwise from up) of the bright limb: the
    /// direction of the Sun projected onto the disc.
    pub fn bright_limb_screen_angle(&self) -> f64 {
        let (u, v, _) = self.project(self.sun_lon, self.sun_lat);
        (-u).atan2(v)
    }

    /// Illuminated fraction of the disc implied by the geometry.
    pub fn illuminated_fraction(&self) -> f64 {
        let (_, _, z) = self.project(self.sun_lon, self.sun_lat);
        0.5 * (1.0 + z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::angle::{DEG, norm_pi};
    use mooncalc_core::libration::physical_ephemeris;
    use mooncalc_core::tables::{TableOptions, ephemeris_row};
    use mooncalc_core::{Instant, MeeusEphemeris};

    #[test]
    fn project_and_unproject_are_inverse() {
        let view = View {
            l: 5.0 * DEG,
            b: -6.0 * DEG,
            sun_lon: 0.3,
            sun_lat: 0.02,
            rotation: 0.7,
        };
        for (eta, theta) in [(0.0, 0.0), (0.5, 0.3), (-1.2, -0.8), (1.4, 1.1)] {
            let (u, v, z) = view.project(eta, theta);
            assert!(z > 0.0);
            let (e2, t2) = view.unproject(u, v).unwrap();
            assert!(norm_pi(e2 - eta).abs() < 1e-12 && (t2 - theta).abs() < 1e-12);
        }
        // The sub-observer point is the centre of the disc.
        let (u, v, z) = view.project(view.l, view.b);
        assert!(u.abs() < 1e-12 && v.abs() < 1e-12 && (z - 1.0).abs() < 1e-12);
        assert!(view.unproject(0.8, 0.8).is_none());
    }

    /// The rendered geometry must reproduce Meeus' bright-limb angle χ
    /// (eq. 48.5) and illuminated fraction k (eq. 48.1), computed
    /// independently from the Sun's and Moon's equatorial positions.
    fn check(instant: Instant) -> (f64, f64) {
        let eph = MeeusEphemeris::new();
        let row = ephemeris_row(&eph, instant, &TableOptions::default());
        let p = physical_ephemeris(&eph, row.jd_tt);
        let view = View {
            l: p.libration.lon,
            b: p.libration.lat,
            sun_lon: p.sun_lon,
            sun_lat: p.sun_lat,
            rotation: p.libration.position_angle,
        };
        let chi = row.illumination.bright_limb_angle;
        let screen = view.bright_limb_screen_angle();
        assert!(
            norm_pi(screen - chi).abs() < 1.0 * DEG,
            "χ = {:.2}°, rendered {:.2}°",
            chi / DEG,
            screen / DEG
        );
        assert!((view.illuminated_fraction() - row.illumination.fraction).abs() < 0.005);
        (chi, screen)
    }

    #[test]
    fn rendered_bright_limb_matches_meeus() {
        // 1992 April 12 (Meeus 48.a, 53.a), the evening crescent of
        // 2024 March 11, and a waning crescent.
        check(Instant::from_jd_tt(2_448_724.5));
        let (_, crescent) = check(Instant::from_calendar(2024, 3, 11, 16, 0, 0.0));
        // A waxing crescent is lit on its west side: position angle near
        // 270°, i.e. on the right of a north-up, east-left display.
        let right = -crescent.sin(); // screen x of the bright-limb direction
        assert!(right > 0.9, "bright limb should face right, got {right}");
        check(Instant::from_calendar(2024, 4, 5, 3, 0, 0.0));
    }
}
