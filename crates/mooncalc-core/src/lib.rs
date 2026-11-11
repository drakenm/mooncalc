//! Astronomical core for MoonCalc.
//!
//! # Conventions (shared by every crate in the workspace)
//!
//! * All angles are **radians** in `f64` unless the field or function name
//!   ends in `_deg`, `_arcmin`, `_arcsec` or `_hours`.
//! * Longitude is **east-positive**; latitude is north-positive.
//! * Azimuth is measured from **north through east** (N = 0, E = π/2).
//! * Time is carried as an [`Instant`] (UT, as a Julian Day). Ephemerides
//!   take TT/TDB Julian Days (`jd_tt`); convert with [`Instant::jd_tt`].
//! * Distances are kilometres.

pub mod angle;
pub mod coords;
pub mod ephemeris;
pub mod time;

pub use coords::{Ecliptic, Equatorial, GeoLocation, Horizontal};
pub use ephemeris::Ephemeris;
pub use time::{CalendarDateTime, Instant};
