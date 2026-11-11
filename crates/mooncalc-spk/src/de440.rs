//! The JPL DE440 apparent-place [`Ephemeris`] backend.
//!
//! [`De440Ephemeris`] wraps an [`Spk`] holding a JPL planetary ephemeris
//! (DE440, DE440s, or any DE file with the Sun, the Earth, the Moon and the
//! Earth–Moon barycentre) and turns its geometric barycentric vectors into
//! **geocentric apparent** places referred to the true equator and equinox
//! of date, as the [`Ephemeris`] trait requires.
//!
//! # Pipeline
//!
//! For a TT Julian Day `jd_tt`, following the reduction of Kaplan, *The IAU
//! Resolutions on Astronomical Reference Systems, Time Scales, and Earth
//! Rotation Models* (USNO Circular 179, 2005), ch. 7, and Urban & Seidelmann
//! (eds.), *Explanatory Supplement to the Astronomical Almanac* (3rd ed.,
//! 2013), §7.2:
//!
//! 1. **Time.** `et = time::jd_tt_to_et(jd_tt)` (TT → TDB seconds past
//!    J2000).
//! 2. **Observer.** The barycentric position `E` and velocity `Ė` of the
//!    geocentre (399 relative to 0) at `et`, ICRF.
//! 3. **Light time.** For the Sun (10) or the Moon (301) iterate
//!    `τ = |B(et − τ) − E| / c` from `τ = 0` until it changes by less than
//!    0.1 µs (three evaluations; the residual moves the Moon by < 3 mm). The
//!    astrometric vector is `p = B(et − τ) − E`; its length is the returned
//!    distance (Horizons' light-time-corrected range "delta").
//! 4. **Gravitational deflection** is omitted. For the Sun itself it is
//!    zero, and for the Moon (source and observer almost collinear with the
//!    Sun, 0.0026 au apart) the Sun's deflection is below 0.01 mas
//!    (*Explanatory Supplement* 3rd ed., eq. 7.65; Kaplan 2005, eq. 7.5).
//! 5. **Aberration.** The relativistic stellar aberration from the
//!    geocentre's barycentric velocity, `V = Ė / c`, `β⁻¹ = √(1 − V²)`:
//!    `u' = [β⁻¹ u + (1 + u·V / (1 + β⁻¹)) V] / (1 + u·V)`
//!    (*Explanatory Supplement* 3rd ed., eq. 7.40; Kaplan 2005, eq. 7.40).
//!    It applies to the Moon as well as the Sun: with barycentric light-time
//!    vectors, the classical "planetary aberration" of a geocentric body is
//!    exactly light time plus this term.
//! 6. **ICRF → true equator and equinox of date**, `r = N · P · B · u'`
//!    with the model chosen by [`FrameModel`] (IERS Conventions 2010,
//!    ch. 5).
//! 7. **Ecliptic of date.** `R1(ε) · r` with the true obliquity
//!    `ε = ε_A + Δε` of the same model.
//!
//! # Precession–nutation model and sidereal time
//!
//! [`FrameModel::Iau2006`] (the default) uses the frame bias, IAU 2006
//! precession (Capitaine et al. 2003) and the IAU 1980 nutation series of
//! `mooncalc-core` (Meeus Table 22.A, 63 terms), with the IAU 2006 mean
//! obliquity `ε_A` in the nutation matrix and the ecliptic. The IAU 1980
//! series is the only nutation the core has; combined with IAU 2006
//! precession it reproduces the IAU 2006/2000A celestial pole to the IAU
//! 1980 series' own error (a few mas, plus 0.3 mas from the truncation), so
//! declinations match JPL Horizons' EOP-corrected pole within about
//! 0.01″. The mismatch between the two theories' ecliptics (ε₀ = 84381.406″
//! versus 84381.448″) enters the nutation matrix only at second order
//! (Δψ · 0.042″ ≈ 4 µas).
//!
//! Right ascensions are then counted from the IAU 2006 equinox. The core's
//! Greenwich apparent sidereal time ([`mooncalc_core::Instant::gast`]) is
//! GMST 1982 plus the IAU 1980 equation of the equinoxes, which counts from
//! the IAU 1976 equinox. That equinox drifts from the IAU 2006 one by the
//! precession-rate correction, so hour angles formed from the core's GAST
//! and this backend's right ascensions are off by about
//! `−0.015″ + 0.276″ T` (`T` in Julian centuries from J2000; at most
//! 0.42″ over 1849–2150, see [`sidereal_time_offset`]). That is 1/9000 of
//! the smallest crescent-criterion tolerance, but a caller that needs exact
//! hour angles can add [`sidereal_time_offset`] to the core's GAST.
//!
//! [`FrameModel::Iau1976`] is the classic consistent pair instead: IAU 1976
//! precession and IAU 1980 nutation with the Laskar obliquity, no frame
//! bias, exactly the core's [`j2000_to_true_of_date_matrix`] used by the
//! Meeus backend. Its right ascensions are consistent with the core's GAST,
//! but its pole carries the IAU 1976 precession-rate error (0.12″ per
//! century in declination, plus the 17 mas frame bias).
//!
//! # Range and errors
//!
//! [`Ephemeris::valid_range`] is the common coverage of the Sun, Moon,
//! Earth and Earth–Moon barycentre segments, shortened at the start by the
//! Sun's light time. The trait methods cannot fail; outside the range they
//! return NaN coordinates. The fallible [`De440Ephemeris::apparent`] and
//! [`De440Ephemeris::sun_moon`] report the [`SpkError`] instead.
//!
//! # Cost
//!
//! One call evaluates the Earth's state, the nutation series and the
//! precession polynomials, plus three body positions for the light-time
//! iteration. The Earth state and rotation matrix of the most recent epoch
//! are cached, so `sun_equatorial` followed by `moon_equatorial` (or the
//! ecliptic variants) at the same `jd_tt` pays for them once.
//! [`De440Ephemeris::sun_moon`] makes that explicit.

use std::sync::Mutex;

use mooncalc_core::angle::ARCSEC;
use mooncalc_core::matrix::{self, Mat3, Vec3, mul, rot_x, rot_z};
use mooncalc_core::nutation::{self, NutationObliquity};
use mooncalc_core::precession::{
    frame_bias_matrix, j2000_to_true_of_date_matrix, precession_matrix_iau2006,
};
use mooncalc_core::time::{DAYS_PER_CENTURY, J2000};
use mooncalc_core::{Ecliptic, Ephemeris, Equatorial};

use crate::error::SpkError;
use crate::naif;
use crate::spk::Spk;
use crate::time;

/// Speed of light, km/s (IAU 1976, defining constant).
pub const C_KM_S: f64 = 299_792.458;

/// The bodies whose segments [`De440Ephemeris`] needs.
const REQUIRED_BODIES: [i32; 4] = [
    naif::SUN,
    naif::MOON,
    naif::EARTH,
    naif::EARTH_MOON_BARYCENTER,
];

/// Margin at the start of the coverage for the Sun's light time (≤ 510 s).
const LIGHT_TIME_MARGIN_S: f64 = 600.0;

/// Light-time iteration stops when `τ` changes by less than this (s).
const LIGHT_TIME_TOL_S: f64 = 1e-7;

/// How the ICRF is rotated to the true equator and equinox of date. See the
/// [module documentation](self) for the trade-off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FrameModel {
    /// Frame bias, IAU 2006 precession, IAU 1980 nutation and the IAU 2006
    /// obliquity: the most accurate pole and equinox.
    #[default]
    Iau2006,
    /// IAU 1976 precession, IAU 1980 nutation and the Laskar obliquity, no
    /// frame bias: the Meeus backend's model, consistent with the core's
    /// GMST 1982 sidereal time.
    Iau1976,
}

/// A body the backend can place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Body {
    Sun,
    Moon,
}

impl Body {
    /// NAIF ID of the body.
    pub const fn naif_id(self) -> i32 {
        match self {
            Body::Sun => naif::SUN,
            Body::Moon => naif::MOON,
        }
    }
}

/// A geocentric apparent place of date.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ApparentPlace {
    /// Right ascension and declination of date; distance is the
    /// light-time-corrected geocentric range, km.
    pub equatorial: Equatorial,
    /// Ecliptic longitude and latitude of date, same distance.
    pub ecliptic: Ecliptic,
    /// One-way light time from the body to the geocentre, s.
    pub light_time_s: f64,
}

/// Apparent places of the Sun and the Moon at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunMoon {
    pub sun: ApparentPlace,
    pub moon: ApparentPlace,
}

/// Everything that depends on the epoch but not on the body.
#[derive(Clone, Copy, Debug)]
struct EpochFrame {
    jd_tt: f64,
    et: f64,
    /// Barycentric position of the geocentre, km, ICRF.
    earth_pos: Vec3,
    /// Barycentric velocity of the geocentre divided by c.
    earth_beta: Vec3,
    /// ICRF → true equator and equinox of date.
    to_true: Mat3,
    /// True obliquity of date, radians.
    eps: f64,
}

/// Geocentric apparent Sun and Moon from a JPL DE ephemeris file.
///
/// `Send + Sync`, so it fits the GUI's `Arc<dyn Ephemeris>`:
///
/// ```no_run
/// use std::sync::Arc;
/// use mooncalc_core::Ephemeris;
/// use mooncalc_spk::de440::De440Ephemeris;
/// # fn main() -> Result<(), mooncalc_spk::SpkError> {
/// let eph: Arc<dyn Ephemeris> = Arc::new(De440Ephemeris::open("de440s.bsp")?);
/// let moon = eph.moon_equatorial(2_460_000.5);
/// # Ok(()) }
/// ```
pub struct De440Ephemeris {
    spk: Spk<'static>,
    model: FrameModel,
    bias: Mat3,
    range: (f64, f64),
    name: String,
    cache: Mutex<Option<EpochFrame>>,
}

impl std::fmt::Debug for De440Ephemeris {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("De440Ephemeris")
            .field("name", &self.name)
            .field("model", &self.model)
            .field("range", &self.range)
            .finish_non_exhaustive()
    }
}

impl De440Ephemeris {
    /// Wrap an open SPK. Fails if it lacks coverage of the Sun, Moon, Earth
    /// or Earth–Moon barycentre, or if the Sun and Moon cannot be chained to
    /// the Earth (for example an unsupported segment type or frame).
    pub fn new(spk: Spk<'static>) -> Result<Self, SpkError> {
        let mut missing = None;
        for b in REQUIRED_BODIES {
            if spk.coverage(b).is_none() {
                missing = Some(b);
                break;
            }
        }
        let (start, end) = match (missing, spk.common_coverage(&REQUIRED_BODIES)) {
            (None, Some(span)) => span,
            (body, _) => {
                return Err(SpkError::NoCoverage {
                    body: body.unwrap_or(naif::SUN),
                    et: f64::NAN,
                });
            }
        };
        // The end loses 1 s so the TT↔ET round trip cannot step past it.
        let (start, end) = (start + LIGHT_TIME_MARGIN_S, end - 1.0);
        if start >= end {
            return Err(SpkError::Corrupt(
                "the Sun, Moon and Earth segments overlap too little".into(),
            ));
        }
        // Evaluate once so a file that cannot be chained fails here, not at
        // the first NaN.
        let mid = 0.5 * (start + end);
        for b in [naif::SUN, naif::MOON] {
            spk.position(b, naif::EARTH, mid)?;
        }
        let name = describe(&spk);
        Ok(Self {
            spk,
            model: FrameModel::default(),
            bias: frame_bias_matrix(),
            range: (time::et_to_jd_tt(start), time::et_to_jd_tt(end)),
            name,
            cache: Mutex::new(None),
        })
    }

    /// Parse an SPK held in memory (the WASM path: fetch the kernel, then
    /// hand over the bytes).
    pub fn from_bytes<T>(bytes: T) -> Result<Self, SpkError>
    where
        T: AsRef<[u8]> + Send + 'static,
    {
        Self::new(Spk::from_bytes(bytes)?)
    }

    /// Choose the precession–nutation model (default
    /// [`FrameModel::Iau2006`]).
    pub fn with_frame_model(mut self, model: FrameModel) -> Self {
        self.model = model;
        self.name = describe_model(&self.name, model);
        *self.cache.get_mut().unwrap_or_else(|e| e.into_inner()) = None;
        self
    }

    /// The precession–nutation model in use.
    pub fn frame_model(&self) -> FrameModel {
        self.model
    }

    /// The underlying SPK.
    pub fn spk(&self) -> &Spk<'static> {
        &self.spk
    }

    /// Geocentric apparent place of `body` at a TT Julian Day.
    pub fn apparent(&self, body: Body, jd_tt: f64) -> Result<ApparentPlace, SpkError> {
        let frame = self.epoch_frame(jd_tt)?;
        self.place(body, &frame)
    }

    /// Geocentric apparent places of the Sun and the Moon at a TT Julian
    /// Day, sharing one Earth state and one rotation matrix.
    pub fn sun_moon(&self, jd_tt: f64) -> Result<SunMoon, SpkError> {
        let frame = self.epoch_frame(jd_tt)?;
        Ok(SunMoon {
            sun: self.place(Body::Sun, &frame)?,
            moon: self.place(Body::Moon, &frame)?,
        })
    }

    /// The astrometric geocentric vector of `body` (light time applied, no
    /// aberration), km, ICRF, and the light time in seconds.
    pub fn astrometric(&self, body: Body, jd_tt: f64) -> Result<(Vec3, f64), SpkError> {
        let frame = self.epoch_frame(jd_tt)?;
        self.light_time_vector(body, &frame)
    }

    fn epoch_frame(&self, jd_tt: f64) -> Result<EpochFrame, SpkError> {
        if let Ok(guard) = self.cache.try_lock()
            && let Some(f) = *guard
            && f.jd_tt.to_bits() == jd_tt.to_bits()
        {
            return Ok(f);
        }
        let et = time::jd_tt_to_et(jd_tt);
        let (earth_pos, earth_vel) =
            self.spk
                .state(naif::EARTH, naif::SOLAR_SYSTEM_BARYCENTER, et)?;
        let (to_true, eps) = match self.model {
            FrameModel::Iau2006 => {
                let n = nutation::nutation(jd_tt);
                let eps_a = mean_obliquity_iau2006(jd_tt);
                let eps = eps_a + n.deps;
                // N = R1(−ε) · R3(−Δψ) · R1(ε_A), IERS Conventions 2010
                // eq. 5.30 with the IAU 2006 ε_A.
                let nut = mul(&rot_x(-eps), &mul(&rot_z(-n.dpsi), &rot_x(eps_a)));
                let pb = mul(&precession_matrix_iau2006(jd_tt), &self.bias);
                (mul(&nut, &pb), eps)
            }
            FrameModel::Iau1976 => (
                j2000_to_true_of_date_matrix(jd_tt),
                NutationObliquity::at(jd_tt).eps,
            ),
        };
        let frame = EpochFrame {
            jd_tt,
            et,
            earth_pos,
            earth_beta: earth_vel.map(|v| v / C_KM_S),
            to_true,
            eps,
        };
        if let Ok(mut guard) = self.cache.try_lock() {
            *guard = Some(frame);
        }
        Ok(frame)
    }

    fn light_time_vector(&self, body: Body, f: &EpochFrame) -> Result<(Vec3, f64), SpkError> {
        let target = body.naif_id();
        let mut tau = 0.0;
        for _ in 0..8 {
            let b = self
                .spk
                .position(target, naif::SOLAR_SYSTEM_BARYCENTER, f.et - tau)?;
            let p = [
                b[0] - f.earth_pos[0],
                b[1] - f.earth_pos[1],
                b[2] - f.earth_pos[2],
            ];
            let next = norm(&p) / C_KM_S;
            let converged = (next - tau).abs() < LIGHT_TIME_TOL_S;
            tau = next;
            if converged {
                return Ok((p, tau));
            }
        }
        Err(SpkError::Corrupt(format!(
            "light time to body {target} did not converge"
        )))
    }

    fn place(&self, body: Body, f: &EpochFrame) -> Result<ApparentPlace, SpkError> {
        let (p, light_time_s) = self.light_time_vector(body, f)?;
        let dist = norm(&p);
        let u = aberrate(&p.map(|x| x / dist), &f.earth_beta);
        let r = matrix::apply(&f.to_true, &u);
        let (ra, dec, _) = matrix::to_spherical(&r);
        let (lon, lat, _) = matrix::to_spherical(&matrix::apply(&rot_x(f.eps), &r));
        Ok(ApparentPlace {
            equatorial: Equatorial {
                ra,
                dec,
                distance_km: dist,
            },
            ecliptic: Ecliptic {
                lon,
                lat,
                distance_km: dist,
            },
            light_time_s,
        })
    }

    fn place_or_nan(&self, body: Body, jd_tt: f64) -> ApparentPlace {
        self.apparent(body, jd_tt).unwrap_or(ApparentPlace {
            equatorial: Equatorial {
                ra: f64::NAN,
                dec: f64::NAN,
                distance_km: f64::NAN,
            },
            ecliptic: Ecliptic {
                lon: f64::NAN,
                lat: f64::NAN,
                distance_km: f64::NAN,
            },
            light_time_s: f64::NAN,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl De440Ephemeris {
    /// Open a DE440/DE440s `.bsp` file. Data records are read from the file
    /// on demand (a few kilobytes per new epoch range), so opening is
    /// instant and memory stays small.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, SpkError> {
        Self::new(Spk::open(path)?)
    }

    /// Read the whole file into memory first (32 MB for DE440s, 114 MB for
    /// DE440): no file I/O after opening.
    pub fn open_in_memory(path: impl AsRef<std::path::Path>) -> Result<Self, SpkError> {
        Self::new(Spk::open_in_memory(path)?)
    }
}

impl Ephemeris for De440Ephemeris {
    fn name(&self) -> &str {
        &self.name
    }

    fn valid_range(&self) -> (f64, f64) {
        self.range
    }

    fn sun_equatorial(&self, jd_tt: f64) -> Equatorial {
        self.place_or_nan(Body::Sun, jd_tt).equatorial
    }

    fn moon_equatorial(&self, jd_tt: f64) -> Equatorial {
        self.place_or_nan(Body::Moon, jd_tt).equatorial
    }

    fn sun_ecliptic(&self, jd_tt: f64) -> Ecliptic {
        self.place_or_nan(Body::Sun, jd_tt).ecliptic
    }

    fn moon_ecliptic(&self, jd_tt: f64) -> Ecliptic {
        self.place_or_nan(Body::Moon, jd_tt).ecliptic
    }
}

/// Mean obliquity of the ecliptic, IAU 2006 (Capitaine, Wallace & Chapront
/// 2003, eq. 39; IERS Conventions 2010 eq. 5.40), radians, at a TT Julian
/// Day.
pub fn mean_obliquity_iau2006(jd_tt: f64) -> f64 {
    const C: [f64; 6] = [
        84_381.406,
        -46.836_769,
        -0.000_183_1,
        0.002_003_40,
        -0.000_000_576,
        -0.000_000_043_4,
    ];
    let t = (jd_tt - J2000) / DAYS_PER_CENTURY;
    C.iter().rev().fold(0.0, |acc, &k| acc * t + k) * ARCSEC
}

/// GMST (IAU 2006) minus GMST (IAU 1982), radians, as a function of TT.
///
/// Adding it to the core's GAST gives the sidereal time that matches
/// [`FrameModel::Iau2006`] right ascensions, so hour angles agree with the
/// IAU 2006 Earth-rotation model. GMST 2006 is `ERA + 0.014506″ +
/// 4612.156534″ t + 1.3915817″ t² − …` (IERS Conventions 2010 eq. 5.32);
/// GMST 1982 is Meeus eq. 12.4 (Aoki et al. 1982), whose rotation rate
/// exceeds the Earth rotation angle's by 4612.4329″ per century and
/// whose quadratic term is 1.3965588″ (0.000387933° × 3600). The UT1-only
/// parts cancel, leaving a slowly varying polynomial; the TT − UT1 offset
/// changes it by under 1 µas.
pub fn sidereal_time_offset(jd_tt: f64) -> f64 {
    let t = (jd_tt - J2000) / DAYS_PER_CENTURY;
    // GMST 1982: rotation 360.985 647 366 29°/day = ERA rate + 4612.4329″/cy
    // (ERA rate 360.985 612 288 087 6°/day), quadratic 1.396 5588″, cubic
    // −1/38 710 000° = −0.000 093 0″; constant 280.460 618 37° against ERA's
    // 280.460 618 375 04° (−0.000 018″).
    let gmst82_rate = (360.985_647_366_29 - 360.985_612_288_087_6) * 3600.0 * DAYS_PER_CENTURY;
    let gmst82 = -0.000_018 + t * (gmst82_rate + t * (1.396_558_8 - t * 0.000_093_0));
    let gmst06 = 0.014_506
        + t * (4_612.156_534
            + t * (1.391_581_7 + t * (-0.000_000_44 + t * (-0.000_029_956 - t * 0.000_000_036_8))));
    (gmst06 - gmst82) * ARCSEC
}

fn norm(v: &Vec3) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Relativistic stellar aberration of the unit vector `u` for an observer
/// moving at `beta = v / c` (*Explanatory Supplement* 3rd ed., eq. 7.40).
fn aberrate(u: &Vec3, beta: &Vec3) -> Vec3 {
    let vv = beta[0] * beta[0] + beta[1] * beta[1] + beta[2] * beta[2];
    let binv = (1.0 - vv).sqrt();
    let uv = u[0] * beta[0] + u[1] * beta[1] + u[2] * beta[2];
    let f = 1.0 + uv / (1.0 + binv);
    let d = 1.0 + uv;
    [
        (binv * u[0] + f * beta[0]) / d,
        (binv * u[1] + f * beta[1]) / d,
        (binv * u[2] + f * beta[2]) / d,
    ]
}

/// "JPL DE440" from a JPL segment name such as `DE-0440LE-0440`, otherwise
/// "JPL SPK".
fn describe(spk: &Spk<'_>) -> String {
    let de = spk
        .segments()
        .iter()
        .find(|s| s.target == naif::MOON)
        .and_then(|s| {
            let digits = s.name.strip_prefix("DE-")?.get(..4)?;
            let n: u32 = digits.parse().ok()?;
            Some(format!("JPL DE{n}"))
        })
        .unwrap_or_else(|| "JPL SPK".into());
    describe_model(&de, FrameModel::default())
}

fn describe_model(name: &str, model: FrameModel) -> String {
    let base = name.split(" (").next().unwrap_or(name);
    match model {
        FrameModel::Iau2006 => format!("{base} (IAU 2006/1980)"),
        FrameModel::Iau1976 => format!("{base} (IAU 1976/1980)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aberration_is_unit_and_matches_first_order() {
        // 30 km/s perpendicular to the line of sight: 20.6″, plus a
        // second-order term of order V²/2 ≈ 1 mas.
        let beta = [0.0, 30.0 / C_KM_S, 0.0];
        let u = aberrate(&[1.0, 0.0, 0.0], &beta);
        assert!((norm(&u) - 1.0).abs() < 1e-15);
        let angle = u[1].atan2(u[0]) / ARCSEC;
        let first_order = beta[1] / ARCSEC;
        assert!((angle - first_order).abs() < 0.002, "{angle} {first_order}");
        // Along the motion the direction does not change.
        let u = aberrate(&[0.0, 1.0, 0.0], &beta);
        assert!(u[0].abs() < 1e-18 && (u[1] - 1.0).abs() < 1e-15);
        // Oblique, any vector stays unit length.
        let s = 1.0 / 3f64.sqrt();
        let u = aberrate(&[s, -s, s], &[1e-4, 2e-5, -3e-5]);
        assert!((norm(&u) - 1.0).abs() < 1e-15);
    }

    #[test]
    fn obliquity_iau2006_reference_values() {
        // ε_A(J2000) = 84381.406″; IERS 2010 eq. 5.40 at T = 1:
        // 84381.406 − 46.836769 − 0.0001831 + 0.0020034 − ... = 84334.5711″.
        assert!((mean_obliquity_iau2006(J2000) / ARCSEC - 84_381.406).abs() < 1e-9);
        let t1 = mean_obliquity_iau2006(J2000 + DAYS_PER_CENTURY) / ARCSEC;
        assert!((t1 - 84_334.571_051).abs() < 1e-6, "{t1}");
        // And within 0.1″ of the Laskar (IAU 1976-based) value over ±2 cy:
        // 0.042″ at J2000 plus a 0.027″/cy rate difference.
        for t in [-2.0, -1.0, 0.0, 1.0, 2.0] {
            let jd = J2000 + t * DAYS_PER_CENTURY;
            let d = (mean_obliquity_iau2006(jd) - nutation::mean_obliquity(jd)) / ARCSEC;
            assert!(d.abs() < 0.1, "T={t}: {d}″");
        }
    }

    #[test]
    fn sidereal_time_offset_matches_iers_values() {
        // At J2000 the offset is GMST06's constant 0.014506″ (ERA and GMST82
        // share their constant to 0.02 mas).
        let o = sidereal_time_offset(J2000) / ARCSEC;
        assert!((o - 0.014_524).abs() < 1e-5, "{o}");
        // The linear term is the IAU 2000 precession-rate correction in
        // right ascension, δψ_A cos ε₀ = −0.29965″ · 0.91748 = −0.2749″/cy
        // (Capitaine et al. 2003), within 2 mas/cy.
        let rate = sidereal_time_offset(J2000 + DAYS_PER_CENTURY) / ARCSEC - o;
        assert!((rate + 0.2749).abs() < 0.007, "{rate}");
    }

    #[test]
    fn model_names() {
        assert_eq!(
            describe_model("JPL DE440 (IAU 2006/1980)", FrameModel::Iau1976),
            "JPL DE440 (IAU 1976/1980)"
        );
        assert_eq!(
            describe_model("JPL SPK", FrameModel::Iau2006),
            "JPL SPK (IAU 2006/1980)"
        );
    }
}
