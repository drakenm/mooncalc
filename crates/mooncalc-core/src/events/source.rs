//! Sources of geocentric apparent Sun and Moon positions for event searches.
//!
//! Event searches evaluate positions many times. [`PositionSource`] hides
//! where they come from: [`DirectSource`] evaluates any [`Ephemeris`] at
//! every call, and [`ApparentTable`] tabulates one over a time span and
//! interpolates, which is what makes world maps fast.

use crate::coords::Equatorial;
use crate::ephemeris::Ephemeris;
use crate::matrix::{Vec3, from_spherical, to_spherical};
use crate::nutation::NutationObliquity;
use crate::time::{Instant, gmst};

/// The two bodies the event searches handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Body {
    Sun,
    Moon,
}

/// Geocentric apparent positions and sidereal time at a UT instant.
///
/// Implementations return the body's position as a Cartesian vector in km,
/// referred to the true equator and equinox of date (the frame of
/// [`Ephemeris::sun_equatorial`]), and the Greenwich apparent sidereal time.
pub trait PositionSource {
    /// Geocentric apparent position of `body` at UT Julian Day `jd_ut`, km.
    fn position(&self, body: Body, jd_ut: f64) -> Vec3;

    /// Greenwich apparent sidereal time at `jd_ut`, radians.
    fn gast(&self, jd_ut: f64) -> f64;

    /// Geocentric apparent equatorial coordinates of `body`.
    fn equatorial(&self, body: Body, jd_ut: f64) -> Equatorial {
        let (ra, dec, distance_km) = to_spherical(&self.position(body, jd_ut));
        Equatorial {
            ra,
            dec,
            distance_km,
        }
    }
}

impl<S: PositionSource + ?Sized> PositionSource for &S {
    fn position(&self, body: Body, jd_ut: f64) -> Vec3 {
        (**self).position(body, jd_ut)
    }
    fn gast(&self, jd_ut: f64) -> f64 {
        (**self).gast(jd_ut)
    }
}

/// Evaluates an [`Ephemeris`] at every call (ΔT from [`crate::deltat`]).
#[derive(Clone, Copy)]
pub struct DirectSource<'a> {
    pub ephemeris: &'a dyn Ephemeris,
}

impl<'a> DirectSource<'a> {
    pub fn new(ephemeris: &'a dyn Ephemeris) -> Self {
        Self { ephemeris }
    }
}

impl PositionSource for DirectSource<'_> {
    fn position(&self, body: Body, jd_ut: f64) -> Vec3 {
        let jd_tt = Instant::from_jd_ut(jd_ut).jd_tt();
        let eq = match body {
            Body::Sun => self.ephemeris.sun_equatorial(jd_tt),
            Body::Moon => self.ephemeris.moon_equatorial(jd_tt),
        };
        from_spherical(eq.ra, eq.dec, eq.distance_km)
    }

    fn gast(&self, jd_ut: f64) -> f64 {
        Instant::from_jd_ut(jd_ut).gast()
    }
}

/// Default node spacing of an [`ApparentTable`]: 30 minutes.
pub const DEFAULT_TABLE_STEP_DAYS: f64 = 1.0 / 48.0;

/// Sun and Moon positions tabulated at equal UT steps and interpolated with
/// four-point (cubic) Lagrange interpolation of the Cartesian vectors.
///
/// Interpolating Cartesian coordinates avoids the 0/2π wrap of right
/// ascension and yields the distance as well. The geocentric vectors vary
/// with periods of two weeks and longer (the Moon's variation, 14.8 d, is
/// the shortest significant one), so at the default 30-minute step the
/// cubic interpolation error is far below 0.001″ (see the tests). Sidereal
/// time is GMST from its closed formula plus the interpolated equation of
/// the equinoxes.
///
/// Evaluation outside the tabulated span extrapolates from the end nodes and
/// quickly loses accuracy; build the table over the whole span you need.
#[derive(Clone, Debug)]
pub struct ApparentTable {
    jd0: f64,
    step: f64,
    start: f64,
    end: f64,
    sun: Vec<Vec3>,
    moon: Vec<Vec3>,
    eqeq: Vec<f64>,
}

impl ApparentTable {
    /// Tabulate `ephemeris` over `[start, end]` (UT) at `step_days`, with
    /// margin nodes so that the whole span interpolates centrally.
    pub fn new(ephemeris: &dyn Ephemeris, start: Instant, end: Instant, step_days: f64) -> Self {
        assert!(step_days > 0.0 && end.jd_ut >= start.jd_ut);
        let jd0 = start.jd_ut - 2.0 * step_days;
        let n = ((end.jd_ut - jd0) / step_days).ceil() as usize + 3;
        let mut sun = Vec::with_capacity(n);
        let mut moon = Vec::with_capacity(n);
        let mut eqeq = Vec::with_capacity(n);
        for k in 0..n {
            let jd_ut = jd0 + k as f64 * step_days;
            let jd_tt = Instant::from_jd_ut(jd_ut).jd_tt();
            let s = ephemeris.sun_equatorial(jd_tt);
            let m = ephemeris.moon_equatorial(jd_tt);
            sun.push(from_spherical(s.ra, s.dec, s.distance_km));
            moon.push(from_spherical(m.ra, m.dec, m.distance_km));
            eqeq.push(NutationObliquity::at(jd_tt).equation_of_equinoxes());
        }
        Self {
            jd0,
            step: step_days,
            start: start.jd_ut,
            end: end.jd_ut,
            sun,
            moon,
            eqeq,
        }
    }

    /// Tabulate at the default 30-minute step.
    pub fn with_default_step(ephemeris: &dyn Ephemeris, start: Instant, end: Instant) -> Self {
        Self::new(ephemeris, start, end, DEFAULT_TABLE_STEP_DAYS)
    }

    /// The span requested at construction.
    pub fn span(&self) -> (Instant, Instant) {
        (
            Instant::from_jd_ut(self.start),
            Instant::from_jd_ut(self.end),
        )
    }

    /// Node spacing, days.
    pub fn step_days(&self) -> f64 {
        self.step
    }

    /// First node index of the four-point stencil and the Lagrange weights.
    #[inline]
    fn stencil(&self, jd_ut: f64) -> (usize, [f64; 4]) {
        let x = (jd_ut - self.jd0) / self.step;
        let last = self.sun.len() - 3;
        let i = (x.floor() as isize).clamp(1, last as isize) as usize;
        let u = x - i as f64;
        let (um1, up1, um2) = (u - 1.0, u + 1.0, u - 2.0);
        (
            i - 1,
            [
                -u * um1 * um2 / 6.0,
                up1 * um1 * um2 / 2.0,
                -up1 * u * um2 / 2.0,
                up1 * u * um1 / 6.0,
            ],
        )
    }
}

#[inline]
fn lagrange3(v: &[Vec3], i: usize, w: &[f64; 4]) -> Vec3 {
    let mut out = [0.0; 3];
    for (k, wk) in w.iter().enumerate() {
        let p = &v[i + k];
        out[0] += wk * p[0];
        out[1] += wk * p[1];
        out[2] += wk * p[2];
    }
    out
}

impl PositionSource for ApparentTable {
    #[inline]
    fn position(&self, body: Body, jd_ut: f64) -> Vec3 {
        let (i, w) = self.stencil(jd_ut);
        match body {
            Body::Sun => lagrange3(&self.sun, i, &w),
            Body::Moon => lagrange3(&self.moon, i, &w),
        }
    }

    #[inline]
    fn gast(&self, jd_ut: f64) -> f64 {
        let (i, w) = self.stencil(jd_ut);
        let e = &self.eqeq[i..i + 4];
        gmst(jd_ut) + w[0] * e[0] + w[1] * e[1] + w[2] * e[2] + w[3] * e[3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::angle::{ARCSEC, norm_pi};
    use crate::meeus::MeeusEphemeris;

    fn angle(a: &Vec3, b: &Vec3) -> f64 {
        let (ra1, d1, _) = to_spherical(a);
        let (ra2, d2, _) = to_spherical(b);
        crate::angle::separation(ra1, d1, ra2, d2)
    }

    #[test]
    fn table_matches_direct_evaluation() {
        let eph = MeeusEphemeris::new();
        let direct = DirectSource::new(&eph);
        let start = Instant::from_calendar(2024, 3, 9, 0, 0, 0.0);
        let end = start.add_days(4.0);
        let table = ApparentTable::with_default_step(&eph, start, end);
        let mut worst = [0.0_f64; 3];
        for k in 0..=400 {
            // Off-node instants across the whole span.
            let jd = start.jd_ut + k as f64 * 0.01 + 0.003;
            for (n, body) in [Body::Sun, Body::Moon].into_iter().enumerate() {
                let a = table.position(body, jd);
                let b = direct.position(body, jd);
                worst[n] = worst[n].max(angle(&a, &b));
                let ra = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
                let rb = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
                assert!((ra - rb).abs() < 1e-3, "distance {} km", ra - rb);
            }
            worst[2] = worst[2].max(norm_pi(table.gast(jd) - direct.gast(jd)).abs());
        }
        println!(
            "table vs direct: Sun {:.2e}″, Moon {:.2e}″, GAST {:.2e}″",
            worst[0] / ARCSEC,
            worst[1] / ARCSEC,
            worst[2] / ARCSEC
        );
        assert!(worst[0] < 1e-3 * ARCSEC, "Sun {}″", worst[0] / ARCSEC);
        assert!(worst[1] < 1e-3 * ARCSEC, "Moon {}″", worst[1] / ARCSEC);
        assert!(worst[2] < 1e-4 * ARCSEC, "GAST {}″", worst[2] / ARCSEC);
        // Exactly at a node the table reproduces the ephemeris.
        let jd = table.jd0 + 10.0 * table.step;
        assert!(
            angle(
                &table.position(Body::Moon, jd),
                &direct.position(Body::Moon, jd)
            ) < 1e-9
        );
        assert_eq!(table.span(), (start, end));
    }

    #[test]
    fn equatorial_from_position() {
        let eph = MeeusEphemeris::new();
        let direct = DirectSource::new(&eph);
        let jd_ut = 2_451_545.0;
        let eq = direct.equatorial(Body::Sun, jd_ut);
        let e = eph.sun_equatorial(Instant::from_jd_ut(jd_ut).jd_tt());
        assert!((eq.ra - e.ra).abs() < 1e-12 && (eq.dec - e.dec).abs() < 1e-12);
        assert!((eq.distance_km - e.distance_km).abs() < 1e-3);
    }
}
