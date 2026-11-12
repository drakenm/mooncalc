//! Ephemeris for physical observations of the Moon: optical and physical
//! libration, the position angle of the Moon's axis, the selenographic
//! position of the Sun (colongitude, terminator) and the Sun's altitude at
//! a lunar site.
//!
//! Meeus, *Astronomical Algorithms* 2nd ed., ch. 53: eqs. 53.1 (optical
//! libration), 53.2 (physical libration with the ρ, σ, τ series of the IAU
//! rotation model, inclination of the mean lunar equator I = 1.54242°),
//! the position angle P, the Sun's selenographic coordinates via the
//! heliocentric λH, βH, the Sun's altitude above a lunar site, and the
//! topocentric librations. The topocentric case is computed rigorously by
//! feeding the topocentric λ, β, α into the same formulas — the method
//! Meeus describes as an alternative to his differential corrections, which
//! it reproduces (see tests).
//!
//! Inputs come from any [`Ephemeris`], so the results follow the selected
//! backend.

use crate::angle::{DEG, norm_2pi, norm_pi};
use crate::coords::{Ecliptic, GeoLocation};
use crate::ephemeris::Ephemeris;
use crate::meeus::moon::fundamental_arguments;
use crate::nutation::NutationObliquity;
use crate::time::{Instant, centuries_since_j2000};
use crate::topocentric::topocentric_for;
use crate::transform::{ecliptic_to_equatorial, equatorial_to_ecliptic};

/// Inclination of the mean lunar equator to the ecliptic (IAU), Meeus ch. 53.
pub const LUNAR_EQUATOR_INCLINATION: f64 = 1.542_42 * DEG;

/// Lunar librations and the position angle of the axis (all radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Libration {
    /// Optical libration in longitude l′, `(−π, π]`.
    pub optical_lon: f64,
    /// Optical libration in latitude b′.
    pub optical_lat: f64,
    /// Physical libration in longitude l″.
    pub physical_lon: f64,
    /// Physical libration in latitude b″.
    pub physical_lat: f64,
    /// Total libration in longitude l = l′ + l″ (selenographic longitude of
    /// the sub-observer point), `(−π, π]`.
    pub lon: f64,
    /// Total libration in latitude b = b′ + b″.
    pub lat: f64,
    /// Position angle P of the Moon's axis of rotation (north pole),
    /// measured eastward from the north point of the disk, `[0, 2π)`.
    pub position_angle: f64,
}

/// Physical ephemeris of the Moon at one instant (all angles radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoonPhysical {
    pub libration: Libration,
    /// Selenographic longitude of the Sun l₀ (subsolar point), `(−π, π]`.
    pub sun_lon: f64,
    /// Selenographic latitude of the Sun b₀.
    pub sun_lat: f64,
    /// Selenographic colongitude of the Sun c₀ = 90° − l₀, `[0, 2π)`. The
    /// morning terminator lies at selenographic longitude −c₀ (about 0° at
    /// first quarter, 180° at last quarter: c₀ ≈ 270°, 0°, 90°, 180° at new
    /// moon, first quarter, full moon, last quarter).
    pub colongitude: f64,
}

impl MoonPhysical {
    /// Selenographic longitude of the morning terminator (sunrise line at
    /// the lunar equator), `(−π, π]`.
    pub fn morning_terminator_lon(&self) -> f64 {
        norm_pi(-self.colongitude)
    }

    /// Selenographic longitude of the evening terminator (sunset line),
    /// `(−π, π]`.
    pub fn evening_terminator_lon(&self) -> f64 {
        norm_pi(std::f64::consts::PI - self.colongitude)
    }

    /// Altitude of the Sun above the horizon at selenographic longitude
    /// `eta` and latitude `theta` (see [`sun_altitude_on_moon`]).
    pub fn sun_altitude_at(&self, eta: f64, theta: f64) -> f64 {
        sun_altitude_on_moon(eta, theta, self.sun_lon, self.sun_lat)
    }
}

/// Quantities of Meeus ch. 53 that depend only on the instant.
struct Rotation {
    f: f64,
    om: f64,
    dpsi: f64,
    eps: f64,
    rho: f64,
    sigma: f64,
    tau: f64,
}

impl Rotation {
    fn at(jd_tt: f64, nut: &NutationObliquity) -> Self {
        let t = centuries_since_j2000(jd_tt);
        let [_, d, m, mp, f] = fundamental_arguments(jd_tt);
        // Longitude of the mean ascending node (Meeus eq. 47.7).
        let om = (125.044_547_9
            + t * (-1_934.136_289_1
                + t * (0.002_075_4 + t * (1.0 / 467_441.0 - t / 60_616_000.0))))
            * DEG;
        let e = 1.0 - t * (0.002_516 + 0.000_007_4 * t);
        let k1 = (119.75 + 131.849 * t) * DEG;
        let k2 = (72.56 + 20.186 * t) * DEG;
        let (s, c) = (f64::sin, f64::cos);
        let rho = -0.027_52 * c(mp) - 0.022_45 * s(f) + 0.006_84 * c(mp - 2.0 * f)
            - 0.002_93 * c(2.0 * f)
            - 0.000_85 * c(2.0 * f - 2.0 * d)
            - 0.000_54 * c(mp - 2.0 * d)
            - 0.000_20 * s(mp + f)
            - 0.000_20 * c(mp + 2.0 * f)
            - 0.000_20 * c(mp - f)
            + 0.000_14 * c(mp + 2.0 * f - 2.0 * d);
        let sigma = -0.028_16 * s(mp) + 0.022_44 * c(f)
            - 0.006_82 * s(mp - 2.0 * f)
            - 0.002_79 * s(2.0 * f)
            - 0.000_83 * s(2.0 * f - 2.0 * d)
            + 0.000_69 * s(mp - 2.0 * d)
            + 0.000_40 * c(mp + f)
            - 0.000_25 * s(2.0 * mp)
            - 0.000_23 * s(mp + 2.0 * f)
            + 0.000_20 * c(mp - f)
            + 0.000_19 * s(mp - f)
            + 0.000_13 * s(mp + 2.0 * f - 2.0 * d)
            - 0.000_10 * c(mp - 3.0 * f);
        let tau = 0.025_20 * e * s(m) + 0.004_73 * s(2.0 * mp - 2.0 * f) - 0.004_67 * s(mp)
            + 0.003_96 * s(k1)
            + 0.002_76 * s(2.0 * mp - 2.0 * d)
            + 0.001_96 * s(om)
            - 0.001_83 * c(mp - f)
            + 0.001_15 * s(mp - 2.0 * d)
            - 0.000_96 * s(mp - d)
            + 0.000_46 * s(2.0 * f - 2.0 * d)
            - 0.000_39 * s(mp - f)
            - 0.000_32 * s(mp - m - d)
            + 0.000_27 * s(2.0 * mp - m - 2.0 * d)
            + 0.000_23 * s(k2)
            - 0.000_14 * s(2.0 * d)
            + 0.000_14 * c(2.0 * mp - 2.0 * f)
            - 0.000_12 * s(mp - 2.0 * f)
            - 0.000_12 * s(2.0 * mp)
            + 0.000_11 * s(2.0 * mp - 2.0 * m - 2.0 * d);
        Self {
            f,
            om,
            dpsi: nut.dpsi,
            eps: nut.eps,
            rho: rho * DEG,
            sigma: sigma * DEG,
            tau: tau * DEG,
        }
    }

    /// Optical and physical librations for a direction given by apparent
    /// ecliptic longitude `lon` (true equinox of date) and latitude `lat`
    /// (Meeus eqs. 53.1, 53.2). Returns `(l′, b′, l″, b″)`.
    fn librations(&self, lon: f64, lat: f64) -> (f64, f64, f64, f64) {
        let (si, ci) = LUNAR_EQUATOR_INCLINATION.sin_cos();
        let w = lon - self.dpsi - self.om;
        let (sw, cw) = w.sin_cos();
        let (sb, cb) = lat.sin_cos();
        let a = (sw * cb * ci - sb * si).atan2(cw * cb);
        let lp = norm_pi(a - self.f);
        let bp = (-sw * cb * si - sb * ci).clamp(-1.0, 1.0).asin();
        let (sa, ca) = a.sin_cos();
        let lpp = -self.tau + (self.rho * ca + self.sigma * sa) * bp.tan();
        let bpp = self.sigma * ca - self.rho * sa;
        (lp, bp, lpp, bpp)
    }

    /// Full libration and position angle for a Moon seen at apparent
    /// ecliptic (lon, lat) and apparent right ascension `ra`.
    fn libration(&self, lon: f64, lat: f64, ra: f64) -> Libration {
        let (lp, bp, lpp, bpp) = self.librations(lon, lat);
        let b = bp + bpp;
        let i = LUNAR_EQUATOR_INCLINATION;
        let v = self.om + self.dpsi + self.sigma / i.sin();
        let (sv, cv) = v.sin_cos();
        let (sir, cir) = (i + self.rho).sin_cos();
        let (se, ce) = self.eps.sin_cos();
        let x = sir * sv;
        let y = sir * cv * ce - cir * se;
        let omega = x.atan2(y);
        let sp = (x.hypot(y) * (ra - omega).cos() / b.cos()).clamp(-1.0, 1.0);
        Libration {
            optical_lon: lp,
            optical_lat: bp,
            physical_lon: lpp,
            physical_lat: bpp,
            lon: norm_pi(lp + lpp),
            lat: b,
            position_angle: norm_2pi(sp.asin()),
        }
    }

    /// Selenographic (l₀, b₀) of the Sun (Meeus ch. 53): the librations
    /// formulas applied to the heliocentric direction of the Moon
    /// λH = λ☉ + 180° + (Δ/R) cos β sin(λ☉ − λ), βH = (Δ/R) β.
    fn sun_selenographic(&self, moon: &Ecliptic, sun: &Ecliptic) -> (f64, f64) {
        let ratio = moon.distance_km / sun.distance_km;
        let lon_h =
            sun.lon + std::f64::consts::PI + ratio * moon.lat.cos() * (sun.lon - moon.lon).sin();
        let lat_h = ratio * moon.lat;
        let (lp, bp, lpp, bpp) = self.librations(lon_h, lat_h);
        (norm_pi(lp + lpp), bp + bpp)
    }
}

fn assemble(rot: &Rotation, libration: Libration, moon: &Ecliptic, sun: &Ecliptic) -> MoonPhysical {
    let (sun_lon, sun_lat) = rot.sun_selenographic(moon, sun);
    MoonPhysical {
        libration,
        sun_lon,
        sun_lat,
        colongitude: norm_2pi(90.0 * DEG - sun_lon),
    }
}

/// Geocentric physical ephemeris of the Moon at a TT Julian Day.
pub fn physical_ephemeris(eph: &dyn Ephemeris, jd_tt: f64) -> MoonPhysical {
    let nut = NutationObliquity::at(jd_tt);
    let rot = Rotation::at(jd_tt, &nut);
    let moon = eph.moon_ecliptic(jd_tt);
    let sun = eph.sun_ecliptic(jd_tt);
    let ra = ecliptic_to_equatorial(&moon, nut.eps).ra;
    assemble(&rot, rot.libration(moon.lon, moon.lat, ra), &moon, &sun)
}

/// Topocentric physical ephemeris for an observer: librations and position
/// angle from the topocentric Moon (which can differ from the geocentric
/// values by up to ~1°). The Sun's selenographic position does not depend
/// on the observer and is geocentric.
pub fn topocentric_physical_ephemeris(
    eph: &dyn Ephemeris,
    instant: Instant,
    loc: &GeoLocation,
) -> MoonPhysical {
    let jd_tt = instant.jd_tt();
    let nut = NutationObliquity::at(jd_tt);
    let rot = Rotation::at(jd_tt, &nut);
    let moon = eph.moon_ecliptic(jd_tt);
    let sun = eph.sun_ecliptic(jd_tt);
    let lst = norm_2pi(instant.gast_with(&nut) + loc.lon_deg * DEG);
    let topo_eq = topocentric_for(&ecliptic_to_equatorial(&moon, nut.eps), loc, lst);
    let topo = equatorial_to_ecliptic(&topo_eq, nut.eps);
    assemble(
        &rot,
        rot.libration(topo.lon, topo.lat, topo_eq.ra),
        &moon,
        &sun,
    )
}

/// Altitude of the Sun above the lunar horizon at selenographic longitude
/// `eta` (east-positive, i.e. towards Mare Crisium) and latitude `theta`,
/// given the Sun's selenographic `sun_lon` (l₀) and `sun_lat` (b₀):
/// `sin h = sin b₀ sin θ + cos b₀ cos θ sin(c₀ + η)` (Meeus ch. 53).
pub fn sun_altitude_on_moon(eta: f64, theta: f64, sun_lon: f64, sun_lat: f64) -> f64 {
    let c0 = 90.0 * DEG - sun_lon;
    let (sb, cb) = sun_lat.sin_cos();
    let (st, ct) = theta.sin_cos();
    (sb * st + cb * ct * (c0 + eta).sin())
        .clamp(-1.0, 1.0)
        .asin()
}

/// The Sun's altitude at a lunar site rises by about 12.19075° per day
/// times cos θ (Meeus ch. 53).
const LUNAR_SUN_RATE_DEG_PER_DAY: f64 = 12.190_75;

fn lunar_horizon_crossing(eph: &dyn Ephemeris, eta: f64, theta: f64, jd_tt: f64, sign: f64) -> f64 {
    let mut jd = jd_tt;
    for _ in 0..12 {
        let p = physical_ephemeris(eph, jd);
        let h = sun_altitude_on_moon(eta, theta, p.sun_lon, p.sun_lat);
        let step = h / DEG / (LUNAR_SUN_RATE_DEG_PER_DAY * theta.cos());
        jd += sign * step;
        if step.abs() < 1e-6 {
            break;
        }
    }
    jd
}

/// Sunrise at a lunar site (selenographic `eta`, `theta`) nearest `jd_tt`,
/// TT Julian Day, by the iteration of Meeus ch. 53 (example 53.b).
pub fn lunar_sunrise(eph: &dyn Ephemeris, eta: f64, theta: f64, jd_tt: f64) -> f64 {
    lunar_horizon_crossing(eph, eta, theta, jd_tt, -1.0)
}

/// Sunset at a lunar site nearest `jd_tt`, TT Julian Day.
pub fn lunar_sunset(eph: &dyn Ephemeris, eta: f64, theta: f64, jd_tt: f64) -> f64 {
    lunar_horizon_crossing(eph, eta, theta, jd_tt, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeus::MeeusEphemeris;
    use crate::topocentric::horizontal_parallax;
    use crate::transform::hour_angle;

    #[test]
    fn meeus_53a() {
        // 1992 April 12, 0h TD: l′ = −1.206°, b′ = +4.194°, l″ = −0.025°,
        // b″ = +0.006°, l = −1.23°, b = +4.20°, P = 15.08°, l₀ = 67.90°,
        // b₀ = +1.46°.
        let p = physical_ephemeris(&MeeusEphemeris::new(), 2_448_724.5);
        let l = p.libration;
        assert!(
            (l.optical_lon / DEG + 1.206).abs() < 0.001,
            "{}",
            l.optical_lon / DEG
        );
        assert!(
            (l.optical_lat / DEG - 4.194).abs() < 0.001,
            "{}",
            l.optical_lat / DEG
        );
        assert!(
            (l.physical_lon / DEG + 0.025).abs() < 0.001,
            "{}",
            l.physical_lon / DEG
        );
        assert!(
            (l.physical_lat / DEG - 0.006).abs() < 0.001,
            "{}",
            l.physical_lat / DEG
        );
        assert!((l.lon / DEG + 1.23).abs() < 0.005);
        assert!((l.lat / DEG - 4.20).abs() < 0.005);
        assert!(
            (l.position_angle / DEG - 15.08).abs() < 0.005,
            "{}",
            l.position_angle / DEG
        );
        assert!(
            (p.sun_lon / DEG - 67.90).abs() < 0.01,
            "{}",
            p.sun_lon / DEG
        );
        assert!(
            (p.sun_lat / DEG - 1.46).abs() < 0.005,
            "{}",
            p.sun_lat / DEG
        );
        assert!((p.colongitude / DEG - 22.10).abs() < 0.01);
    }

    #[test]
    fn meeus_53b() {
        // Copernicus (η = −20°, θ = +9.7°) on 1992 April 12, 0h TD: the Sun
        // is 2.318° above the horizon; sunrise there on 1992 April 11.8069 TD.
        let eph = MeeusEphemeris::new();
        let (eta, theta) = (-20.0 * DEG, 9.7 * DEG);
        let p = physical_ephemeris(&eph, 2_448_724.5);
        assert!(
            (p.sun_altitude_at(eta, theta) / DEG - 2.318).abs() < 0.005,
            "{}",
            p.sun_altitude_at(eta, theta) / DEG
        );
        let sr = lunar_sunrise(&eph, eta, theta, crate::time::julian_day(1992, 4, 15.0));
        assert!(
            (sr - crate::time::julian_day(1992, 4, 11.8069)).abs() < 0.001,
            "{sr}"
        );
        let p = physical_ephemeris(&eph, sr);
        assert!(p.sun_altitude_at(eta, theta).abs() < 1e-4);
        let ss = lunar_sunset(&eph, eta, theta, sr + 15.0);
        assert!(ss > sr + 14.0 && ss < sr + 16.0);
        // The site lies near the morning terminator at sunrise (offset in
        // longitude by ≈ tan b₀ tan θ at latitude θ).
        assert!((p.morning_terminator_lon() - eta).abs() < 0.5 * DEG);
    }

    #[test]
    fn sub_earth_point_sees_sun_at_90_minus_phase_angle() {
        // Geometric identity: the Sun's altitude at the sub-Earth point
        // (l, b) is 90° minus the geocentric phase angle. It ties l₀/b₀ to
        // l/b and to ch. 48 (illumination) over a whole lunation; at first
        // quarter the terminator passes through the sub-Earth point.
        let eph = MeeusEphemeris::new();
        for i in 0..30 {
            let jd = 2_460_310.5 + i as f64;
            let p = physical_ephemeris(&eph, jd);
            let ill = crate::illumination::Illumination::compute(
                &eph.sun_equatorial(jd),
                &eph.moon_equatorial(jd),
            );
            let h = p.sun_altitude_at(p.libration.lon, p.libration.lat);
            let expected = 90.0 * DEG - ill.phase_angle;
            assert!(
                (h - expected).abs() < 0.05 * DEG,
                "{jd}: {} vs {}",
                h / DEG,
                expected / DEG
            );
        }
    }

    #[test]
    fn topocentric_matches_meeus_corrections() {
        // Meeus ch. 53 topocentric corrections:
        // Q = parallactic angle, z = geocentric zenith distance,
        // π′ = π (sin z + 0.0084 sin 2z), Δl = −π′ sin(Q − P) / cos b,
        // Δb = π′ cos(Q − P).
        let eph = MeeusEphemeris::new();
        let loc = GeoLocation::new(51.5, -0.1, 0.0);
        for h in [0u32, 5, 11, 17, 22] {
            let i = Instant::from_calendar(2024, 3, 20, h, 0, 0.0);
            let jd = i.jd_tt();
            let geo = physical_ephemeris(&eph, jd);
            let topo = topocentric_physical_ephemeris(&eph, i, &loc);
            let moon = eph.moon_equatorial(jd);
            let phi = loc.lat_deg * DEG;
            let ha = hour_angle(i.local_apparent_sidereal_time(loc.lon_deg * DEG), moon.ra);
            let (sd, cd) = moon.dec.sin_cos();
            let q = (phi.cos() * ha.sin()).atan2(cd * phi.sin() - sd * phi.cos() * ha.cos());
            let z = (sd * phi.sin() + cd * phi.cos() * ha.cos()).acos();
            let pp = horizontal_parallax(moon.distance_km) * (z.sin() + 0.0084 * (2.0 * z).sin());
            let (p, b) = (geo.libration.position_angle, geo.libration.lat);
            let dl = -pp * (q - p).sin() / b.cos();
            let db = pp * (q - p).cos();
            let got_dl = norm_pi(topo.libration.lon - geo.libration.lon);
            let got_db = topo.libration.lat - geo.libration.lat;
            assert!(
                (got_dl - dl).abs() < 0.01 * DEG,
                "{h}h: {} vs {}",
                got_dl / DEG,
                dl / DEG
            );
            assert!(
                (got_db - db).abs() < 0.01 * DEG,
                "{h}h: {} vs {}",
                got_db / DEG,
                db / DEG
            );
            assert_eq!(topo.sun_lon, geo.sun_lon);
        }
    }
}
