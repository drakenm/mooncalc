//! Solar and lunar eclipses.
//!
//! Three levels of precision:
//!
//! 1. **Meeus ch. 54** ([`solar_eclipse_meeus`], [`lunar_eclipse_meeus`]):
//!    J. Meeus, *Astronomical Algorithms* 2nd ed., ch. 54 — the eclipse
//!    test on the argument of latitude F, the instant of greatest eclipse
//!    (eq. 54.1 on the mean lunation of eq. 49.1), γ, the radii u, p, ρ, σ,
//!    magnitudes (eqs. 54.2–54.4) and lunar semi-durations. No ephemeris is
//!    needed; Meeus quotes errors of a few minutes in time and a few
//!    thousandths in γ. The search API ([`next_eclipse`],
//!    [`eclipses_between`], [`eclipses_in_years`]) is built on this level.
//! 2. **Ephemeris refinement** ([`refine_solar_eclipse`],
//!    [`refine_lunar_eclipse`]): solar eclipses from Besselian elements
//!    computed directly from any [`Ephemeris`] ([`besselian_elements`],
//!    after the *Explanatory Supplement to the Astronomical Ephemeris*
//!    (1961) ch. 9 and Meeus, *Elements of Solar Eclipses 1951–2200*);
//!    lunar eclipses from the Moon's distance to the axis of the Earth's
//!    shadow with Danjon's enlargement of the shadow by 1/85 (radii
//!    `(1 + 1/85) π₁ + π☉ ± s☉`, `π₁ = 0.998340 π☾`) and the shadow axis
//!    opposite the apparent Sun, which reproduce the NASA Five Millennium
//!    Canon of Lunar Eclipses (Espenak & Meeus 2009) to seconds. NASA's
//!    lunar radii k = 0.272488 (penumbral contacts) and k = 0.272281
//!    (umbral contacts) are used for the Moon.
//! 3. **Local circumstances** of solar eclipses ([`local_solar_eclipse`]):
//!    contacts, maximum, magnitude and obscuration for an observer, from the
//!    topocentric apparent Sun and Moon of the ephemeris. This is equivalent
//!    to evaluating the Besselian elements at the observer, without the
//!    polynomial fit; magnitude and obscuration follow the definitions of
//!    NASA's local-circumstance calculator (Espenak & Jubier).
//!
//! Instants are TT Julian Days throughout (convert with
//! [`Instant::from_jd_tt`] for UT). Lengths in the fundamental plane are in
//! Earth equatorial radii.

use crate::angle::{ARCSEC, DEG, norm_2pi, norm_pi, separation};
use crate::coords::{Equatorial, GeoLocation, Horizontal};
use crate::ephemeris::Ephemeris;
use crate::matrix::from_spherical;
use crate::search;
use crate::time::{Instant, julian_day};
use crate::topocentric::{
    AU_KM, EARTH_EQUATORIAL_RADIUS_KM, EARTH_FLATTENING, ObserverGeocentric,
    SUN_SEMIDIAMETER_1AU_ARCSEC, topocentric_for,
};
use crate::transform::equatorial_to_horizontal;

/// Lunar radius in Earth radii for the penumbral cone and external
/// contacts (NASA / Espenak).
pub const MOON_K_PENUMBRA: f64 = 0.272_488;
/// Lunar radius in Earth radii for the umbral cone and internal contacts
/// (NASA / Espenak; IAU 1982 value).
pub const MOON_K_UMBRA: f64 = 0.272_281;
/// Danjon's enlargement of the Earth's shadow, 1/85 (NASA canon).
const DANJON: f64 = 1.0 + 1.0 / 85.0;
/// Solar radius, km, from the semi-diameter 959.63″ at 1 AU (Meeus ch. 55).
const SUN_RADIUS_KM: f64 = SUN_SEMIDIAMETER_1AU_ARCSEC * ARCSEC * AU_KM;

// ---------------------------------------------------------------------------
// Meeus ch. 54
// ---------------------------------------------------------------------------

/// Type of a solar eclipse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SolarEclipseKind {
    Partial,
    Annular,
    Total,
    /// Annular-total ("hybrid").
    Hybrid,
}

/// Type of a lunar eclipse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LunarEclipseKind {
    Penumbral,
    /// Partial umbral.
    Partial,
    Total,
}

/// A solar eclipse by Meeus ch. 54.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolarEclipse {
    /// Lunation number k (0 at the new moon of 2000 January 6; equal to
    /// NASA's lunation number).
    pub lunation: i64,
    pub kind: SolarEclipseKind,
    /// Whether the shadow axis touches the Earth.
    pub central: bool,
    /// Greatest eclipse (shadow axis nearest the Earth's centre), TT JD.
    pub jd_tt: f64,
    /// γ: least distance of the shadow axis from the Earth's centre, Earth
    /// radii, positive when the axis passes north of the centre.
    pub gamma: f64,
    /// u: radius of the umbral cone in the fundamental plane (negative for
    /// a total eclipse).
    pub umbral_radius: f64,
    /// p = u + 0.5461: radius of the penumbral cone.
    pub penumbral_radius: f64,
    /// Magnitude at greatest eclipse. For non-central eclipses eq. 54.2;
    /// for central ones the Moon/Sun diameter ratio on the axis,
    /// `p − u` over `p + u − 2ω` with Meeus' ω = 0.00464 √(1 − γ²) (the
    /// height of the axis point above the fundamental plane times tan f).
    pub magnitude: f64,
}

/// A lunar eclipse by Meeus ch. 54.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LunarEclipse {
    /// Lunation number of the preceding new moon (k − 0.5; NASA's lunation
    /// number).
    pub lunation: i64,
    pub kind: LunarEclipseKind,
    /// Greatest eclipse, TT JD.
    pub jd_tt: f64,
    /// γ: least distance of the Moon's centre from the shadow axis, Earth
    /// radii, positive when the Moon passes north of the axis.
    pub gamma: f64,
    /// σ: radius of the umbra at the Moon's distance, Earth radii.
    pub umbral_radius: f64,
    /// ρ: radius of the penumbra.
    pub penumbral_radius: f64,
    /// Umbral magnitude (eq. 54.3); negative for penumbral eclipses.
    pub umbral_magnitude: f64,
    /// Penumbral magnitude (eq. 54.4).
    pub penumbral_magnitude: f64,
    /// Semi-duration of the penumbral phase, hours.
    pub semi_duration_penumbral_hours: f64,
    /// Semi-duration of the partial (umbral) phase, hours.
    pub semi_duration_partial_hours: Option<f64>,
    /// Semi-duration of totality, hours.
    pub semi_duration_total_hours: Option<f64>,
}

/// Either kind of eclipse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Eclipse {
    Solar(SolarEclipse),
    Lunar(LunarEclipse),
}

impl Eclipse {
    /// Greatest eclipse, TT JD.
    pub fn jd_tt(&self) -> f64 {
        match self {
            Eclipse::Solar(e) => e.jd_tt,
            Eclipse::Lunar(e) => e.jd_tt,
        }
    }
}

/// Which eclipses a search returns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EclipseFilter {
    #[default]
    Any,
    Solar,
    Lunar,
}

/// Mean lunation instant (TT JD) for k: an integer for new moon, + 0.5 for
/// full moon (Meeus eq. 49.1).
pub fn mean_lunation_jde(k: f64) -> f64 {
    let t = k / 1236.85;
    2_451_550.097_66
        + 29.530_588_861 * k
        + t * t * (0.000_154_37 + t * (-0.000_000_150 + t * 0.000_000_000_73))
}

/// Lunation number of the last mean new moon at or before `jd_tt`.
pub fn lunation_at(jd_tt: f64) -> i64 {
    ((jd_tt - 2_451_550.097_66) / 29.530_588_861).floor() as i64
}

/// Quantities common to solar and lunar eclipses (Meeus ch. 54).
struct Ch54 {
    jd_tt: f64,
    gamma: f64,
    u: f64,
    mp: f64,
}

fn ch54(k: f64) -> Option<Ch54> {
    let t = k / 1236.85;
    let f = (160.710_8
        + 390.670_502_84 * k
        + t * t * (-0.001_611_8 + t * (-0.000_002_27 + t * 0.000_000_011)))
        * DEG;
    if f.sin().abs() > 0.36 {
        return None;
    }
    let solar = k.rem_euclid(1.0) < 0.25;
    let m = (2.553_4 + 29.105_356_70 * k + t * t * (-0.000_001_4 - t * 0.000_000_11)) * DEG;
    let mp = (201.564_3
        + 385.816_935_28 * k
        + t * t * (0.010_758_2 + t * (0.000_012_38 - t * 0.000_000_058)))
        * DEG;
    let om = (124.774_6 - 1.563_755_88 * k + t * t * (0.002_067_2 + t * 0.000_002_15)) * DEG;
    let e = 1.0 - t * (0.002_516 + 0.000_007_4 * t);
    let f1 = f - 0.026_65 * DEG * om.sin();
    let a1 = (299.77 + 0.107_408 * k - 0.009_173 * t * t) * DEG;
    let (c1, c2) = if solar {
        (-0.4075, 0.1721)
    } else {
        (-0.4065, 0.1727)
    };
    let s = f64::sin;
    let jd_tt = mean_lunation_jde(k) + c1 * s(mp) + c2 * e * s(m) + 0.0161 * s(2.0 * mp)
        - 0.0097 * s(2.0 * f1)
        + 0.0073 * e * s(mp - m)
        - 0.0050 * e * s(mp + m)
        - 0.0023 * s(mp - 2.0 * f1)
        + 0.0021 * e * s(2.0 * m)
        + 0.0012 * s(mp + 2.0 * f1)
        + 0.0006 * e * s(2.0 * mp + m)
        - 0.0004 * s(3.0 * mp)
        - 0.0003 * e * s(m + 2.0 * f1)
        + 0.0003 * s(a1)
        - 0.0002 * e * s(m - 2.0 * f1)
        - 0.0002 * e * s(2.0 * mp - m)
        - 0.0002 * s(om);
    let c = f64::cos;
    let p = 0.2070 * e * s(m) + 0.0024 * e * s(2.0 * m) - 0.0392 * s(mp) + 0.0116 * s(2.0 * mp)
        - 0.0073 * e * s(mp + m)
        + 0.0067 * e * s(mp - m)
        + 0.0118 * s(2.0 * f1);
    let q = 5.2207 - 0.0048 * e * c(m) + 0.0020 * e * c(2.0 * m)
        - 0.3299 * c(mp)
        - 0.0060 * e * c(mp + m)
        + 0.0041 * e * c(mp - m);
    let w = c(f1).abs();
    let gamma = (p * c(f1) + q * s(f1)) * (1.0 - 0.0048 * w);
    let u = 0.0059 + 0.0046 * e * c(m) - 0.0182 * c(mp) + 0.0004 * c(2.0 * mp) - 0.0005 * c(m + mp);
    Some(Ch54 {
        jd_tt,
        gamma,
        u,
        mp,
    })
}

/// The solar eclipse at new moon number `lunation`, if any (Meeus ch. 54).
pub fn solar_eclipse_meeus(lunation: i64) -> Option<SolarEclipse> {
    let g = ch54(lunation as f64)?;
    let (gamma, u) = (g.gamma, g.u);
    let ag = gamma.abs();
    let p = u + 0.5461;
    if ag > 1.5433 + u {
        return None;
    }
    let central = ag < 0.9972;
    let kind = if !central {
        if ag < 0.9972 + u.abs() {
            if u < 0.0 {
                SolarEclipseKind::Total
            } else {
                SolarEclipseKind::Annular
            }
        } else {
            SolarEclipseKind::Partial
        }
    } else if u < 0.0 {
        SolarEclipseKind::Total
    } else if u > 0.0047 {
        SolarEclipseKind::Annular
    } else if u < 0.00464 * (1.0 - gamma * gamma).sqrt() {
        SolarEclipseKind::Hybrid
    } else {
        SolarEclipseKind::Annular
    };
    let magnitude = if central {
        let omega = 0.00464 * (1.0 - gamma * gamma).sqrt();
        0.5461 / (0.5461 + 2.0 * (u - omega))
    } else {
        (1.5433 + u - ag) / (0.5461 + 2.0 * u)
    };
    Some(SolarEclipse {
        lunation,
        kind,
        central,
        jd_tt: g.jd_tt,
        gamma,
        umbral_radius: u,
        penumbral_radius: p,
        magnitude,
    })
}

/// The lunar eclipse at the full moon following new moon `lunation`, if
/// any (Meeus ch. 54).
pub fn lunar_eclipse_meeus(lunation: i64) -> Option<LunarEclipse> {
    let g = ch54(lunation as f64 + 0.5)?;
    let (gamma, u) = (g.gamma, g.u);
    let ag = gamma.abs();
    let umbral_magnitude = (1.0128 - u - ag) / 0.5450;
    let penumbral_magnitude = (1.5573 + u - ag) / 0.5450;
    let kind = if umbral_magnitude >= 1.0 {
        LunarEclipseKind::Total
    } else if umbral_magnitude > 0.0 {
        LunarEclipseKind::Partial
    } else if penumbral_magnitude > 0.0 {
        LunarEclipseKind::Penumbral
    } else {
        return None;
    };
    let n = 0.5458 + 0.0400 * g.mp.cos();
    let semi = |r: f64| (r * r - gamma * gamma).sqrt() / n;
    Some(LunarEclipse {
        lunation,
        kind,
        jd_tt: g.jd_tt,
        gamma,
        umbral_radius: 0.7403 - u,
        penumbral_radius: 1.2848 + u,
        umbral_magnitude,
        penumbral_magnitude,
        semi_duration_penumbral_hours: semi(1.5573 + u),
        semi_duration_partial_hours: (kind != LunarEclipseKind::Penumbral)
            .then(|| semi(1.0128 - u)),
        semi_duration_total_hours: (kind == LunarEclipseKind::Total).then(|| semi(0.4678 - u)),
    })
}

/// The eclipse (if any) at half-lunation `k2 / 2`: even → solar, odd → lunar.
fn eclipse_at_half(k2: i64, filter: EclipseFilter) -> Option<Eclipse> {
    let solar = k2.rem_euclid(2) == 0;
    match (solar, filter) {
        (true, EclipseFilter::Any | EclipseFilter::Solar) => {
            solar_eclipse_meeus(k2.div_euclid(2)).map(Eclipse::Solar)
        }
        (false, EclipseFilter::Any | EclipseFilter::Lunar) => {
            lunar_eclipse_meeus(k2.div_euclid(2)).map(Eclipse::Lunar)
        }
        _ => None,
    }
}

/// The first eclipse with greatest eclipse strictly after `jd_tt`.
pub fn next_eclipse(jd_tt: f64, filter: EclipseFilter) -> Eclipse {
    let mut k2 = 2 * lunation_at(jd_tt) - 2;
    loop {
        if let Some(e) = eclipse_at_half(k2, filter).filter(|e| e.jd_tt() > jd_tt) {
            return e;
        }
        k2 += 1;
    }
}

/// The last eclipse with greatest eclipse strictly before `jd_tt`.
pub fn previous_eclipse(jd_tt: f64, filter: EclipseFilter) -> Eclipse {
    let mut k2 = 2 * lunation_at(jd_tt) + 3;
    loop {
        if let Some(e) = eclipse_at_half(k2, filter).filter(|e| e.jd_tt() < jd_tt) {
            return e;
        }
        k2 -= 1;
    }
}

/// All eclipses with `jd_from <= jd_tt < jd_to`, chronological.
pub fn eclipses_between(jd_from: f64, jd_to: f64, filter: EclipseFilter) -> Vec<Eclipse> {
    let mut out = Vec::new();
    let mut k2 = 2 * lunation_at(jd_from) - 2;
    let end = 2 * lunation_at(jd_to) + 3;
    while k2 <= end {
        if let Some(e) = eclipse_at_half(k2, filter)
            && e.jd_tt() >= jd_from
            && e.jd_tt() < jd_to
        {
            out.push(e);
        }
        k2 += 1;
    }
    out
}

/// All eclipses in the calendar years `first..=last` (TT dates).
pub fn eclipses_in_years(first: i32, last: i32, filter: EclipseFilter) -> Vec<Eclipse> {
    eclipses_between(
        julian_day(first, 1, 1.0),
        julian_day(last + 1, 1, 1.0),
        filter,
    )
}

// ---------------------------------------------------------------------------
// Besselian elements and refined solar eclipses
// ---------------------------------------------------------------------------

/// Besselian elements of a solar eclipse at one instant, computed from the
/// apparent geocentric Sun and Moon. Lengths in Earth equatorial radii,
/// angles in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BesselianElements {
    /// TT Julian Day.
    pub jd_tt: f64,
    /// Coordinates of the shadow axis in the fundamental plane (x towards
    /// the east, y towards the north celestial pole).
    pub x: f64,
    pub y: f64,
    /// Declination of the shadow axis direction (towards the Sun).
    pub d: f64,
    /// Greenwich hour angle of the shadow axis direction, `[0, 2π)`.
    pub mu: f64,
    /// Radius of the penumbral cone in the fundamental plane.
    pub l1: f64,
    /// Radius of the umbral cone (negative: total).
    pub l2: f64,
    /// Tangents of the cone half-angles.
    pub tan_f1: f64,
    pub tan_f2: f64,
}

/// Besselian elements at a TT Julian Day from `eph` (Explanatory
/// Supplement 1961 ch. 9: the shadow axis is the Moon→Sun direction; the
/// cones are tangent to the Sun (radius 696 000 km) and to the Moon with
/// NASA's radii [`MOON_K_PENUMBRA`], [`MOON_K_UMBRA`]).
pub fn besselian_elements(eph: &dyn Ephemeris, jd_tt: f64) -> BesselianElements {
    let a = EARTH_EQUATORIAL_RADIUS_KM;
    let sun = eph.sun_equatorial(jd_tt);
    let moon = eph.moon_equatorial(jd_tt);
    let s = from_spherical(sun.ra, sun.dec, sun.distance_km / a);
    let m = from_spherical(moon.ra, moon.dec, moon.distance_km / a);
    let g = [s[0] - m[0], s[1] - m[1], s[2] - m[2]];
    let gl = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
    let ra_axis = g[1].atan2(g[0]);
    let d = (g[2] / gl).asin();
    let r = moon.distance_km / a;
    let (sd, cd) = d.sin_cos();
    let (sdm, cdm) = moon.dec.sin_cos();
    let (sh, ch) = (moon.ra - ra_axis).sin_cos();
    let x = r * cdm * sh;
    let y = r * (sdm * cd - cdm * sd * ch);
    let z = r * (sdm * sd + cdm * cd * ch);
    let rs = SUN_RADIUS_KM / a;
    let f1 = ((rs + MOON_K_PENUMBRA) / gl).asin();
    let f2 = ((rs - MOON_K_UMBRA) / gl).asin();
    let (tan_f1, tan_f2) = (f1.tan(), f2.tan());
    let mu = norm_2pi(Instant::from_jd_tt(jd_tt).gast() - ra_axis);
    BesselianElements {
        jd_tt,
        x,
        y,
        d,
        mu,
        l1: z * tan_f1 + MOON_K_PENUMBRA / f1.cos(),
        l2: z * tan_f2 - MOON_K_UMBRA / f2.cos(),
        tan_f1,
        tan_f2,
    }
}

/// Fundamental-plane coordinates (ξ, η, ζ) of a geodetic point for
/// Besselian elements `b` (Explanatory Supplement 1961 §9.3).
fn observer_fundamental(b: &BesselianElements, lat: f64, lon_east: f64) -> (f64, f64, f64) {
    let o = ObserverGeocentric::new(lat, 0.0);
    let h = b.mu + lon_east;
    let (sh, chh) = h.sin_cos();
    let (sd, cd) = b.d.sin_cos();
    (
        o.rho_cos_phi * sh,
        o.rho_sin_phi * cd - o.rho_cos_phi * chh * sd,
        o.rho_sin_phi * sd + o.rho_cos_phi * chh * cd,
    )
}

/// A solar eclipse refined against an ephemeris.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolarEclipseDetails {
    pub kind: SolarEclipseKind,
    pub central: bool,
    /// Greatest eclipse: least distance of the shadow axis from the
    /// Earth's centre, TT JD.
    pub jd_tt: f64,
    /// γ, Earth radii, signed like y.
    pub gamma: f64,
    /// Magnitude at the point of greatest eclipse: the Moon/Sun diameter
    /// ratio for a central eclipse, the fraction of the Sun's diameter
    /// covered otherwise (NASA catalogue convention).
    pub magnitude: f64,
    /// Geodetic latitude of the point of greatest eclipse, degrees.
    pub lat_deg: f64,
    /// East longitude of the point of greatest eclipse, degrees `(−180, 180]`.
    pub lon_deg: f64,
    /// Sun's altitude at the point of greatest eclipse (airless), radians.
    pub sun_alt: f64,
    /// Besselian elements at greatest eclipse.
    pub elements: BesselianElements,
}

/// Refine a ch. 54 solar eclipse against `eph`: the minimum of x² + y²
/// within ±0.1 day, then γ, type, magnitude and the point of greatest
/// eclipse from the Besselian elements there.
pub fn refine_solar_eclipse(eph: &dyn Ephemeris, approx: &SolarEclipse) -> SolarEclipseDetails {
    let r2 = |jd: f64| {
        let b = besselian_elements(eph, jd);
        b.x * b.x + b.y * b.y
    };
    let (jd_tt, _) = search::minimize(r2, approx.jd_tt - 0.1, approx.jd_tt + 0.1, 1e-7);
    let b = besselian_elements(eph, jd_tt);
    let m = b.x.hypot(b.y);
    let gamma = m.copysign(b.y);
    // Earth's outline in the fundamental plane: y is scaled by ρ₁.
    let e2 = EARTH_FLATTENING * (2.0 - EARTH_FLATTENING);
    let (sd, cd) = b.d.sin_cos();
    let rho1 = (1.0 - e2 * cd * cd).sqrt();
    let y1 = b.y / rho1;
    let zeta1_sq = 1.0 - b.x * b.x - y1 * y1;
    let central = zeta1_sq > 0.0;
    let (xi, eta1, zeta1) = if central {
        (b.x, y1, zeta1_sq.sqrt())
    } else {
        let m1 = b.x.hypot(y1);
        (b.x / m1, y1 / m1, 0.0)
    };
    let sd1 = sd / rho1;
    let cd1 = (1.0 - e2).sqrt() * cd / rho1;
    let sin_phi1 = eta1 * cd1 + zeta1 * sd1;
    let cos_phi1_cos_th = zeta1 * cd1 - eta1 * sd1;
    let theta = xi.atan2(cos_phi1_cos_th);
    let cos_phi1 = xi.hypot(cos_phi1_cos_th);
    let lat = sin_phi1.atan2((1.0 - e2).sqrt() * cos_phi1);
    let lon = norm_pi(theta - b.mu);
    let (oxi, oeta, ozeta) = observer_fundamental(&b, lat, lon);
    let l1 = b.l1 - ozeta * b.tan_f1;
    let l2 = b.l2 - ozeta * b.tan_f2;
    let dist = (b.x - oxi).hypot(b.y - oeta);
    let inside = dist < l2.abs();
    let magnitude = if inside {
        (l1 - l2) / (l1 + l2)
    } else {
        (l1 - dist) / (l1 + l2)
    };
    let kind = if central {
        if l2 < 0.0 {
            if b.l2 > 0.0 {
                SolarEclipseKind::Hybrid
            } else {
                SolarEclipseKind::Total
            }
        } else {
            SolarEclipseKind::Annular
        }
    } else if inside {
        if l2 < 0.0 {
            SolarEclipseKind::Total
        } else {
            SolarEclipseKind::Annular
        }
    } else {
        SolarEclipseKind::Partial
    };
    SolarEclipseDetails {
        kind,
        central,
        jd_tt,
        gamma,
        magnitude,
        lat_deg: lat / DEG,
        lon_deg: lon / DEG,
        sun_alt: ozeta.clamp(-1.0, 1.0).asin(),
        elements: b,
    }
}

// ---------------------------------------------------------------------------
// Refined lunar eclipses
// ---------------------------------------------------------------------------

/// A lunar eclipse refined against an ephemeris. Contacts are TT JDs
/// (`None` when the phase does not occur).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LunarEclipseDetails {
    pub kind: LunarEclipseKind,
    /// Greatest eclipse (Moon's centre nearest the shadow axis), TT JD.
    pub jd_tt: f64,
    /// γ, Earth radii, positive when the Moon passes north of the axis.
    pub gamma: f64,
    pub umbral_magnitude: f64,
    pub penumbral_magnitude: f64,
    /// Penumbral contacts P1 and P4.
    pub p1: Option<f64>,
    pub p4: Option<f64>,
    /// Umbral contacts U1 (partial begins), U4 (partial ends).
    pub u1: Option<f64>,
    pub u4: Option<f64>,
    /// Internal umbral contacts U2 (totality begins), U3 (ends).
    pub u2: Option<f64>,
    pub u3: Option<f64>,
    /// Geographic latitude where the Moon is in the zenith at greatest
    /// eclipse, degrees.
    pub zenith_lat_deg: f64,
    /// East longitude of that point, degrees.
    pub zenith_lon_deg: f64,
}

impl LunarEclipseDetails {
    fn span_min(a: Option<f64>, b: Option<f64>) -> Option<f64> {
        Some((b? - a?) * 1440.0)
    }
    /// Duration of the penumbral phase, minutes.
    pub fn penumbral_duration_min(&self) -> Option<f64> {
        Self::span_min(self.p1, self.p4)
    }
    /// Duration of the partial (umbral) phase, minutes.
    pub fn partial_duration_min(&self) -> Option<f64> {
        Self::span_min(self.u1, self.u4)
    }
    /// Duration of totality, minutes.
    pub fn total_duration_min(&self) -> Option<f64> {
        Self::span_min(self.u2, self.u3)
    }
}

/// Shadow geometry at one instant: distance of the Moon from the shadow
/// axis and the radii (radians), plus the Moon's distance.
struct Shadow {
    dist: f64,
    north: bool,
    r_umbra: f64,
    r_penumbra: f64,
    sd_moon_pen: f64,
    sd_moon_umb: f64,
    moon: Equatorial,
}

fn shadow(eph: &dyn Ephemeris, jd_tt: f64) -> Shadow {
    let sun_ecl = eph.sun_ecliptic(jd_tt);
    let moon_ecl = eph.moon_ecliptic(jd_tt);
    // The shadow axis is taken opposite the Sun's apparent direction, as in
    // the NASA canon (checked: removing the 20.5″ of aberration instead
    // shifts NASA's instants of greatest eclipse by ~40 s).
    let axis_lon = sun_ecl.lon + std::f64::consts::PI;
    let axis_lat = -sun_ecl.lat;
    let dist = separation(axis_lon, axis_lat, moon_ecl.lon, moon_ecl.lat);
    let a = EARTH_EQUATORIAL_RADIUS_KM;
    let pi_moon = (a / moon_ecl.distance_km).asin();
    let pi_sun = (a / sun_ecl.distance_km).asin();
    let s_sun = (SUN_RADIUS_KM / sun_ecl.distance_km).asin();
    let pi1 = 0.998_340 * pi_moon;
    Shadow {
        dist,
        north: moon_ecl.lat > axis_lat,
        r_umbra: DANJON * pi1 + pi_sun - s_sun,
        r_penumbra: DANJON * pi1 + pi_sun + s_sun,
        sd_moon_pen: (MOON_K_PENUMBRA * a / moon_ecl.distance_km).asin(),
        sd_moon_umb: (MOON_K_UMBRA * a / moon_ecl.distance_km).asin(),
        moon: eph.moon_equatorial(jd_tt),
    }
}

/// Refine a ch. 54 lunar eclipse against `eph`.
pub fn refine_lunar_eclipse(eph: &dyn Ephemeris, approx: &LunarEclipse) -> LunarEclipseDetails {
    let (jd_tt, _) = search::minimize(
        |jd| shadow(eph, jd).dist,
        approx.jd_tt - 0.1,
        approx.jd_tt + 0.1,
        1e-7,
    );
    let s = shadow(eph, jd_tt);
    let umbral_magnitude = (s.r_umbra + s.sd_moon_umb - s.dist) / (2.0 * s.sd_moon_umb);
    let penumbral_magnitude = (s.r_penumbra + s.sd_moon_pen - s.dist) / (2.0 * s.sd_moon_pen);
    let gamma = (s.dist.sin() * s.moon.distance_km / EARTH_EQUATORIAL_RADIUS_KM)
        .copysign(if s.north { 1.0 } else { -1.0 });
    let contacts = |f: &dyn Fn(&Shadow) -> f64| {
        let g = |jd: f64| f(&shadow(eph, jd));
        (
            search::find_root(g, jd_tt - 0.3, jd_tt, 1e-7),
            search::find_root(g, jd_tt, jd_tt + 0.3, 1e-7),
        )
    };
    let (p1, p4) = contacts(&|s| s.dist - (s.r_penumbra + s.sd_moon_pen));
    let (u1, u4) = contacts(&|s| s.dist - (s.r_umbra + s.sd_moon_umb));
    let (u2, u3) = contacts(&|s| s.dist - (s.r_umbra - s.sd_moon_umb));
    let kind = if u2.is_some() {
        LunarEclipseKind::Total
    } else if u1.is_some() {
        LunarEclipseKind::Partial
    } else {
        LunarEclipseKind::Penumbral
    };
    let gast = Instant::from_jd_tt(jd_tt).gast();
    LunarEclipseDetails {
        kind,
        jd_tt,
        gamma,
        umbral_magnitude,
        penumbral_magnitude,
        p1,
        p4,
        u1,
        u4,
        u2,
        u3,
        zenith_lat_deg: s.moon.dec / DEG,
        zenith_lon_deg: norm_pi(s.moon.ra - gast) / DEG,
    }
}

// ---------------------------------------------------------------------------
// Local circumstances of solar eclipses
// ---------------------------------------------------------------------------

/// Local type of a solar eclipse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LocalEclipseKind {
    Partial,
    Annular,
    Total,
}

/// A contact or maximum: instant and the Sun's airless position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EclipseContact {
    /// TT Julian Day.
    pub jd_tt: f64,
    /// Sun's topocentric altitude/azimuth (no refraction).
    pub sun: Horizontal,
}

/// Local circumstances of a solar eclipse for one observer. Contacts are
/// computed whether or not the Sun is above the horizon; check `sun.alt`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalSolarEclipse {
    pub kind: LocalEclipseKind,
    /// First contact (partial phase begins).
    pub c1: EclipseContact,
    /// Second contact (total/annular phase begins).
    pub c2: Option<EclipseContact>,
    /// Maximum eclipse (least topocentric separation of the centres).
    pub max: EclipseContact,
    pub c3: Option<EclipseContact>,
    /// Fourth contact (partial phase ends).
    pub c4: EclipseContact,
    /// Magnitude at maximum: fraction of the Sun's diameter covered,
    /// `(s☉ + s☾ − Δ) / 2s☉`.
    pub magnitude: f64,
    /// Moon/Sun apparent diameter ratio at maximum.
    pub moon_sun_ratio: f64,
    /// Fraction of the Sun's disc area covered at maximum.
    pub obscuration: f64,
}

struct LocalState {
    sep: f64,
    sd_sun: f64,
    sd_moon_pen: f64,
    sd_moon_umb: f64,
    sun: Equatorial,
    lst: f64,
}

fn local_state(eph: &dyn Ephemeris, loc: &GeoLocation, jd_tt: f64) -> LocalState {
    let lst = norm_2pi(Instant::from_jd_tt(jd_tt).gast() + loc.lon_deg * DEG);
    let sun = topocentric_for(&eph.sun_equatorial(jd_tt), loc, lst);
    let moon = topocentric_for(&eph.moon_equatorial(jd_tt), loc, lst);
    let a = EARTH_EQUATORIAL_RADIUS_KM;
    LocalState {
        sep: separation(sun.ra, sun.dec, moon.ra, moon.dec),
        sd_sun: (SUN_RADIUS_KM / sun.distance_km).asin(),
        sd_moon_pen: (MOON_K_PENUMBRA * a / moon.distance_km).asin(),
        sd_moon_umb: (MOON_K_UMBRA * a / moon.distance_km).asin(),
        sun,
        lst,
    }
}

/// Fraction of the solar disc (radius `rs`) covered by the lunar disc
/// (radius `rm`) at centre separation `d`.
fn obscuration(rs: f64, rm: f64, d: f64) -> f64 {
    if d >= rs + rm {
        return 0.0;
    }
    if d <= (rm - rs).abs() {
        return if rm >= rs { 1.0 } else { (rm / rs).powi(2) };
    }
    let a1 = ((d * d + rs * rs - rm * rm) / (2.0 * d * rs))
        .clamp(-1.0, 1.0)
        .acos();
    let a2 = ((d * d + rm * rm - rs * rs) / (2.0 * d * rm))
        .clamp(-1.0, 1.0)
        .acos();
    let area = rs * rs * (a1 - a1.sin() * a1.cos()) + rm * rm * (a2 - a2.sin() * a2.cos());
    area / (std::f64::consts::PI * rs * rs)
}

/// Local circumstances of the solar eclipse near `jd_tt` (e.g. a
/// [`SolarEclipse::jd_tt`]) for an observer, or `None` if no phase of it
/// reaches the observer (above or below the horizon). Searches ±0.3 day.
pub fn local_solar_eclipse(
    eph: &dyn Ephemeris,
    jd_tt: f64,
    loc: &GeoLocation,
) -> Option<LocalSolarEclipse> {
    // Coarse scan for the minimum separation, then golden-section.
    let step = 5.0 / 1440.0;
    let n = (0.6 / step) as i32;
    let (mut best, mut best_sep) = (jd_tt, f64::INFINITY);
    for i in 0..=n {
        let t = jd_tt - 0.3 + step * i as f64;
        let s = local_state(eph, loc, t).sep;
        if s < best_sep {
            best = t;
            best_sep = s;
        }
    }
    let (t_max, _) = search::minimize(
        |t| local_state(eph, loc, t).sep,
        best - step,
        best + step,
        1e-7,
    );
    let st = local_state(eph, loc, t_max);
    let outer = |s: &LocalState| s.sep - (s.sd_sun + s.sd_moon_pen);
    if outer(&st) >= 0.0 {
        return None;
    }
    let lat = loc.lat_deg * DEG;
    let contact = |t: f64| {
        let s = local_state(eph, loc, t);
        EclipseContact {
            jd_tt: t,
            sun: equatorial_to_horizontal(&s.sun, lat, s.lst),
        }
    };
    let root = |f: &dyn Fn(&LocalState) -> f64, a: f64, b: f64| {
        search::find_root(|t| f(&local_state(eph, loc, t)), a, b, 1e-8)
    };
    let c1 = root(&outer, t_max - 0.3, t_max)?;
    let c4 = root(&outer, t_max, t_max + 0.3)?;
    let inner = |s: &LocalState| s.sep - (s.sd_sun - s.sd_moon_umb).abs();
    let central = inner(&st) < 0.0;
    let (c2, c3) = if central {
        (
            root(&inner, c1, t_max).map(contact),
            root(&inner, t_max, c4).map(contact),
        )
    } else {
        (None, None)
    };
    let kind = match (central, st.sd_moon_umb > st.sd_sun) {
        (false, _) => LocalEclipseKind::Partial,
        (true, true) => LocalEclipseKind::Total,
        (true, false) => LocalEclipseKind::Annular,
    };
    Some(LocalSolarEclipse {
        kind,
        c1: contact(c1),
        c2,
        max: contact(t_max),
        c3,
        c4: contact(c4),
        magnitude: (st.sd_sun + st.sd_moon_pen - st.sep) / (2.0 * st.sd_sun),
        moon_sun_ratio: st.sd_moon_umb / st.sd_sun,
        obscuration: obscuration(st.sd_sun, st.sd_moon_umb, st.sep),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeus::MeeusEphemeris;

    #[test]
    fn meeus_54a() {
        // 1993 May 21: partial solar eclipse, k = −82, JDE = 2449129.0978,
        // γ = 1.1348, u = 0.0097, p = 0.5558, magnitude 0.740.
        assert_eq!(lunation_at(julian_day(1993, 5, 25.0)), -82);
        let e = solar_eclipse_meeus(-82).unwrap();
        assert_eq!(e.kind, SolarEclipseKind::Partial);
        assert!(!e.central);
        assert!((e.jd_tt - 2_449_129.097_8).abs() < 1e-4, "{}", e.jd_tt);
        assert!((e.gamma - 1.1348).abs() < 1e-4);
        assert!((e.umbral_radius - 0.0097).abs() < 1e-4);
        assert!((e.penumbral_radius - 0.5558).abs() < 1e-4);
        assert!((e.magnitude - 0.740).abs() < 5e-4);
    }

    #[test]
    fn meeus_54b() {
        // 2009 July 22: total, central, k = 118, JDE = 2455034.6088,
        // γ = 0.0695, u = −0.0157, p = 0.5304.
        let e = solar_eclipse_meeus(118).unwrap();
        assert_eq!(e.kind, SolarEclipseKind::Total);
        assert!(e.central);
        assert!((e.jd_tt - 2_455_034.608_8).abs() < 1e-4, "{}", e.jd_tt);
        assert!((e.gamma - 0.0695).abs() < 1e-4);
        assert!((e.umbral_radius + 0.0157).abs() < 1e-4);
        assert!((e.penumbral_radius - 0.5304).abs() < 1e-4);
        // Neighbouring new moons have no eclipse.
        assert!(solar_eclipse_meeus(117).is_none() || solar_eclipse_meeus(119).is_none());
    }

    #[test]
    fn meeus_54c_54d_lunar() {
        // 54.c, 1973 June 15: penumbral, k = −328.5, JDE = 2441849.3687,
        // γ = −1.3249, ρ = 1.3045, magnitude 0.4625, semi-duration 101 min.
        let e = lunar_eclipse_meeus(-329).unwrap();
        assert_eq!(e.kind, LunarEclipseKind::Penumbral);
        assert!((e.jd_tt - 2_441_849.368_7).abs() < 1e-4, "{}", e.jd_tt);
        assert!((e.gamma + 1.3249).abs() < 1e-4);
        assert!((e.penumbral_radius - 1.3045).abs() < 1e-4);
        assert!((e.penumbral_magnitude - 0.4625).abs() < 1e-4);
        assert!((e.semi_duration_penumbral_hours * 60.0 - 101.0).abs() < 0.5);
        assert!(e.semi_duration_partial_hours.is_none());
        // 54.d, 1997 September 16: total, k = −28.5, JDE = 2450708.2835,
        // γ = −0.3791, σ = 0.7534, ρ = 1.2717, magnitude 1.1868,
        // semi-durations 30, 98, 153 min.
        let e = lunar_eclipse_meeus(-29).unwrap();
        assert_eq!(e.kind, LunarEclipseKind::Total);
        assert!((e.jd_tt - 2_450_708.283_5).abs() < 1e-4, "{}", e.jd_tt);
        assert!((e.gamma + 0.3791).abs() < 1e-4);
        assert!((e.umbral_radius - 0.7534).abs() < 1e-4);
        assert!((e.penumbral_radius - 1.2717).abs() < 1e-4);
        assert!((e.umbral_magnitude - 1.1868).abs() < 1e-4);
        assert!((e.semi_duration_total_hours.unwrap() * 60.0 - 30.0).abs() < 0.5);
        assert!((e.semi_duration_partial_hours.unwrap() * 60.0 - 98.0).abs() < 0.5);
        assert!((e.semi_duration_penumbral_hours * 60.0 - 153.0).abs() < 0.5);
    }

    /// Parse "HH:MM:SS" on "YYYY-MM-DD" into a JD.
    fn jd_of(date: &str, time: &str) -> f64 {
        let d: Vec<f64> = date.split('-').map(|x| x.parse().unwrap()).collect();
        let t: Vec<f64> = time.split(':').map(|x| x.parse().unwrap()).collect();
        julian_day(
            d[0] as i32,
            d[1] as u32,
            d[2] + (t[0] + t[1] / 60.0 + t[2] / 3600.0) / 24.0,
        )
    }

    /// Parse "25N" / "104W" into signed degrees.
    fn deg_of(s: &str) -> f64 {
        let (num, hemi) = s.split_at(s.len() - 1);
        let v: f64 = num.parse().unwrap();
        if hemi == "S" || hemi == "W" { -v } else { v }
    }

    const NASA_SOLAR: &str =
        include_str!("../../../tests/fixtures/eclipses/nasa_solar_eclipses.csv");
    const NASA_LUNAR: &str =
        include_str!("../../../tests/fixtures/eclipses/nasa_lunar_eclipses.csv");

    #[test]
    fn nasa_solar_catalog() {
        // NASA Five Millennium Catalog of Solar Eclipses (Espenak & Meeus;
        // "Eclipse Predictions by Fred Espenak (NASA's GSFC)"); see
        // docs/validation.md. Measured worst cases over the 13 rows:
        // Meeus ch. 54 — 0.8 min, γ 0.0012, magnitude 0.003; refined with
        // the Meeus ephemeris — 13 s, γ 0.0008, magnitude 0.0014.
        let eph = MeeusEphemeris::new();
        for line in NASA_SOLAR.lines().skip(1) {
            let c: Vec<&str> = line.split(',').collect();
            let (lunation, kind) = (c[4].parse::<i64>().unwrap(), &c[6][..1]);
            let jd = jd_of(c[1], c[2]);
            let (gamma, mag): (f64, f64) = (c[8].parse().unwrap(), c[9].parse().unwrap());
            let expected = match kind {
                "P" => SolarEclipseKind::Partial,
                "A" => SolarEclipseKind::Annular,
                "T" => SolarEclipseKind::Total,
                _ => SolarEclipseKind::Hybrid,
            };
            let e = solar_eclipse_meeus(lunation).unwrap_or_else(|| panic!("{line}"));
            assert_eq!(e.kind, expected, "{line}");
            assert!((e.jd_tt - jd).abs() * 1440.0 < 1.5, "{line}: {}", e.jd_tt);
            assert!((e.gamma - gamma).abs() < 0.003, "{line}: {}", e.gamma);
            assert!((e.magnitude - mag).abs() < 0.005, "{line}: {}", e.magnitude);
            let r = refine_solar_eclipse(&eph, &e);
            assert_eq!(r.kind, expected, "{line}");
            assert!(
                (r.jd_tt - jd).abs() * 86_400.0 < 20.0,
                "{line}: {}",
                r.jd_tt
            );
            assert!((r.gamma - gamma).abs() < 0.001, "{line}: {}", r.gamma);
            assert!((r.magnitude - mag).abs() < 0.002, "{line}: {}", r.magnitude);
            // The catalogue rounds the point of greatest eclipse to 1°.
            assert!(
                (r.lat_deg - deg_of(c[10])).abs() <= 1.0,
                "{line}: {}",
                r.lat_deg
            );
            let dlon = norm_pi((r.lon_deg - deg_of(c[11])) * DEG) / DEG;
            assert!(dlon.abs() <= 1.0, "{line}: {}", r.lon_deg);
            let alt: f64 = c[12].parse().unwrap();
            assert!(
                (r.sun_alt / DEG - alt).abs() <= 1.0,
                "{line}: {}",
                r.sun_alt / DEG
            );
        }
        // 2024 April 8: NASA's point of greatest eclipse is 25.28651° N,
        // 104.13800° W (Google map page).
        let r = refine_solar_eclipse(&eph, &solar_eclipse_meeus(300).unwrap());
        assert!((r.lat_deg - 25.286_51).abs() < 0.05, "{}", r.lat_deg);
        assert!((r.lon_deg + 104.138).abs() < 0.05, "{}", r.lon_deg);
    }

    #[test]
    fn nasa_lunar_catalog() {
        // NASA Five Millennium Catalog of Lunar Eclipses (Espenak & Meeus).
        // Measured worst cases over the 11 rows: Meeus ch. 54 — 0.7 min,
        // γ 0.0062, magnitudes 0.012; refined — 7.4 s, γ 0.0004, magnitudes
        // 0.0015, phase durations 0.15 min.
        let eph = MeeusEphemeris::new();
        for line in NASA_LUNAR.lines().skip(1) {
            let c: Vec<&str> = line.split(',').collect();
            let (lunation, kind) = (c[4].parse::<i64>().unwrap(), &c[6][..1]);
            let jd = jd_of(c[1], c[2]);
            let f = |i: usize| c[i].parse::<f64>().ok();
            let (gamma, pen, um) = (f(8).unwrap(), f(9).unwrap(), f(10).unwrap());
            let expected = match kind {
                "N" => LunarEclipseKind::Penumbral,
                "P" => LunarEclipseKind::Partial,
                _ => LunarEclipseKind::Total,
            };
            let e = lunar_eclipse_meeus(lunation).unwrap_or_else(|| panic!("{line}"));
            assert_eq!(e.kind, expected, "{line}");
            assert!((e.jd_tt - jd).abs() * 1440.0 < 1.0, "{line}: {}", e.jd_tt);
            assert!((e.gamma - gamma).abs() < 0.008, "{line}: {}", e.gamma);
            assert!(
                (e.umbral_magnitude - um).abs() < 0.015,
                "{line}: {}",
                e.umbral_magnitude
            );
            assert!(
                (e.penumbral_magnitude - pen).abs() < 0.015,
                "{line}: {}",
                e.penumbral_magnitude
            );
            let r = refine_lunar_eclipse(&eph, &e);
            assert_eq!(r.kind, expected, "{line}");
            assert!(
                (r.jd_tt - jd).abs() * 86_400.0 < 10.0,
                "{line}: {}",
                r.jd_tt
            );
            assert!((r.gamma - gamma).abs() < 0.001, "{line}: {}", r.gamma);
            assert!(
                (r.umbral_magnitude - um).abs() < 0.003,
                "{line}: {}",
                r.umbral_magnitude
            );
            assert!(
                (r.penumbral_magnitude - pen).abs() < 0.003,
                "{line}: {}",
                r.penumbral_magnitude
            );
            for (got, i) in [
                (r.penumbral_duration_min(), 11),
                (r.partial_duration_min(), 12),
                (r.total_duration_min(), 13),
            ] {
                match f(i) {
                    Some(d) => assert!((got.unwrap() - d).abs() < 0.25, "{line}: {got:?} vs {d}"),
                    None => assert!(got.is_none(), "{line}"),
                }
            }
            assert!((r.zenith_lat_deg - deg_of(c[14])).abs() <= 1.0, "{line}");
            let dlon = norm_pi((r.zenith_lon_deg - deg_of(c[15])) * DEG) / DEG;
            assert!(dlon.abs() <= 1.5, "{line}: {}", r.zenith_lon_deg);
        }
    }

    #[test]
    fn besselian_elements_2024_apr_08() {
        // NASA Besselian elements, total solar eclipse of 2024 April 8
        // (F. Espenak), at t0 = 18:00 TDT: x = −0.318157, y = 0.219747,
        // d = 7.58620°, l1 = 0.535813, l2 = −0.010274, tan f1 = 0.0046683,
        // tan f2 = 0.0046450, μ = 89.59122° (with ΔT = 70.6 s).
        let b = besselian_elements(&MeeusEphemeris::new(), julian_day(2024, 4, 8.75));
        assert!((b.x + 0.318_157).abs() < 1e-3, "{}", b.x);
        assert!((b.y - 0.219_747).abs() < 1e-3, "{}", b.y);
        assert!((b.d / DEG - 7.586_20).abs() < 2e-4, "{}", b.d / DEG);
        assert!((b.l1 - 0.535_813).abs() < 2e-5, "{}", b.l1);
        assert!((b.l2 + 0.010_274).abs() < 2e-5, "{}", b.l2);
        assert!((b.tan_f1 - 0.004_668_3).abs() < 1e-7);
        assert!((b.tan_f2 - 0.004_645_0).abs() < 1e-7);
        // NASA tabulates μ as an ephemeris hour angle (UT = TT) and applies
        // ΔT when evaluating (SEcirc.js: H = μ − λ − ΔT/13713.44); ours is the
        // true Greenwich hour angle, i.e. NASA's μ minus 1.0027379 × 15″ × ΔT.
        let dt = Instant::from_jd_tt(b.jd_tt).delta_t_seconds();
        let expected = 89.591_22 - dt * 1.002_737_9 * 15.0 / 3600.0;
        assert!(
            (b.mu / DEG - expected).abs() < 0.002,
            "{} vs {expected}",
            b.mu / DEG
        );
    }

    #[test]
    fn local_circumstances_2024_apr_08() {
        // Reference: NASA's local-circumstance calculator (SEcirc.js, Espenak
        // & Jubier) evaluated with NASA's Besselian elements for 2024 April 8.
        // (lat, lon, kind, C1, C2, max, C3, C4 as TT JD − 2460409, magnitude,
        // Moon/Sun ratio, Sun altitude at max in degrees).
        type Case = (f64, f64, LocalEclipseKind, [Option<f64>; 5], f64, f64, f64);
        let cases: [Case; 4] = [
            (
                32.7767,
                -96.7970,
                LocalEclipseKind::Total,
                [
                    Some(0.225_311),
                    Some(0.279_073),
                    Some(0.280_406),
                    Some(0.281_738),
                    Some(0.335_989),
                ],
                1.0146,
                1.0558,
                64.62,
            ),
            (
                39.7684,
                -86.1581,
                LocalEclipseKind::Total,
                [
                    Some(0.244_240),
                    Some(0.296_674),
                    Some(0.298_000),
                    Some(0.299_323),
                    Some(0.350_245),
                ],
                1.0183,
                1.0538,
                53.00,
            ),
            (
                40.7128,
                -74.0060,
                LocalEclipseKind::Partial,
                [
                    Some(0.258_155),
                    None,
                    Some(0.310_231),
                    None,
                    Some(0.359_408),
                ],
                0.9107,
                1.0516,
                43.36,
            ),
            (
                25.28651,
                -104.138,
                LocalEclipseKind::Total,
                [
                    Some(0.208_067),
                    Some(0.261_286),
                    Some(0.262_835),
                    Some(0.264_388),
                    Some(0.320_076),
                ],
                1.0283,
                1.0566,
                69.80,
            ),
        ];
        let eph = MeeusEphemeris::new();
        let ecl = solar_eclipse_meeus(300).unwrap();
        for (lat, lon, kind, times, mag, ratio, alt) in cases {
            let loc = GeoLocation::new(lat, lon, 0.0);
            let l = local_solar_eclipse(&eph, ecl.jd_tt, &loc).unwrap();
            assert_eq!(l.kind, kind, "{lat}");
            let got = [Some(l.c1), l.c2, Some(l.max), l.c3, Some(l.c4)];
            for (g, t) in got.iter().zip(times) {
                match t {
                    Some(t) => {
                        let ds = (g.unwrap().jd_tt - 2_460_409.0 - t) * 86_400.0;
                        assert!(ds.abs() < 15.0, "{lat}: {ds} s");
                    }
                    None => assert!(g.is_none()),
                }
            }
            assert!((l.magnitude - mag).abs() < 0.002, "{lat}: {}", l.magnitude);
            assert!(
                (l.moon_sun_ratio - ratio).abs() < 0.001,
                "{lat}: {}",
                l.moon_sun_ratio
            );
            assert!(
                (l.max.sun.alt / DEG - alt).abs() < 0.1,
                "{lat}: {}",
                l.max.sun.alt / DEG
            );
            if kind == LocalEclipseKind::Total {
                assert_eq!(l.obscuration, 1.0);
            } else {
                assert!(l.obscuration > 0.85 && l.obscuration < l.magnitude);
            }
        }
        // Far from the path: Cape Town sees nothing.
        assert!(
            local_solar_eclipse(&eph, ecl.jd_tt, &GeoLocation::new(-33.9, 18.4, 0.0)).is_none()
        );
    }

    #[test]
    fn obscuration_limits() {
        assert_eq!(obscuration(1.0, 1.0, 2.5), 0.0);
        assert_eq!(obscuration(1.0, 1.1, 0.05), 1.0);
        assert!((obscuration(1.0, 0.9, 0.05) - 0.81).abs() < 1e-12);
        // Equal discs offset by one radius: 2(π/3 − √3/4)/π ≈ 0.391.
        assert!((obscuration(1.0, 1.0, 1.0) - 0.391_002_218).abs() < 1e-6);
    }

    #[test]
    fn search_api() {
        // 2024: two solar (Apr 8 total, Oct 2 annular) and two lunar
        // (Mar 25 penumbral, Sep 18 partial) eclipses.
        let all = eclipses_in_years(2024, 2024, EclipseFilter::Any);
        assert_eq!(all.len(), 4);
        assert!(all.windows(2).all(|w| w[1].jd_tt() > w[0].jd_tt()));
        let solar = eclipses_in_years(2024, 2024, EclipseFilter::Solar);
        assert_eq!(solar.len(), 2);
        let jd = julian_day(2024, 1, 1.0);
        match next_eclipse(jd, EclipseFilter::Any) {
            Eclipse::Lunar(e) => {
                assert_eq!(e.kind, LunarEclipseKind::Penumbral);
                assert!((e.jd_tt - julian_day(2024, 3, 25.3)).abs() < 0.1);
            }
            other => panic!("{other:?}"),
        }
        match next_eclipse(jd, EclipseFilter::Solar) {
            Eclipse::Solar(e) => assert_eq!(e.lunation, 300),
            other => panic!("{other:?}"),
        }
        match previous_eclipse(jd, EclipseFilter::Lunar) {
            Eclipse::Lunar(e) => {
                assert_eq!(e.kind, LunarEclipseKind::Partial);
                assert!((e.jd_tt - julian_day(2023, 10, 28.84)).abs() < 0.1);
            }
            other => panic!("{other:?}"),
        }
        // NASA lists 224 solar and 228 lunar eclipses in 2001–2100. Meeus'
        // limits miss two extreme-margin penumbral eclipses (2027 July 18 and
        // 2096 June 6, |γ| ≈ 1.575, penumbral magnitude ≈ 0.001–0.01).
        assert_eq!(
            eclipses_in_years(2001, 2100, EclipseFilter::Solar).len(),
            224
        );
        assert_eq!(
            eclipses_in_years(2001, 2100, EclipseFilter::Lunar).len(),
            226
        );
    }
}
