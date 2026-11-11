//! The ephemeris abstraction shared by the Meeus/ELP backend (default) and
//! the JPL DE440 backend (`de440` feature).

use crate::coords::{Ecliptic, Equatorial};

/// A source of Sun and Moon positions.
///
/// Every method takes a Julian Day in **TT** (TDB is treated as equal to TT;
/// the difference is < 2 ms). Returned positions are **geocentric apparent**:
/// corrected for light time, aberration (Sun), and nutation, referred to the
/// true equator/ecliptic and equinox of date. Distances are **geometric**
/// (no light time), as parallax and semi-diameters require. Topocentric
/// corrections are the caller's job (see `coords`).
pub trait Ephemeris: Send + Sync {
    /// Short human-readable name, e.g. "Meeus/ELP-2000 (truncated)".
    fn name(&self) -> &str;

    /// Range of TT Julian Days this ephemeris supports (inclusive).
    fn valid_range(&self) -> (f64, f64);

    /// Apparent geocentric equatorial position of the Sun.
    fn sun_equatorial(&self, jd_tt: f64) -> Equatorial;

    /// Apparent geocentric equatorial position of the Moon.
    fn moon_equatorial(&self, jd_tt: f64) -> Equatorial;

    /// Apparent geocentric ecliptic position of the Sun.
    fn sun_ecliptic(&self, jd_tt: f64) -> Ecliptic;

    /// Apparent geocentric ecliptic position of the Moon.
    fn moon_ecliptic(&self, jd_tt: f64) -> Ecliptic;
}
