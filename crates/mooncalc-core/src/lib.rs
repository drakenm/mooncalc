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

//!
//! # Modules
//!
//! * [`time`] — Julian Day ↔ calendar, sidereal time; [`deltat`] — ΔT.
//! * [`nutation`], [`precession`] — IAU 1980 nutation, obliquity, IAU 1976
//!   and IAU 2006 precession, as angles and rotation matrices ([`matrix`]).
//! * [`transform`] — ecliptic ↔ equatorial ↔ horizontal.
//! * [`topocentric`] — observer position, parallax, semi-diameters.
//! * [`refraction`] — Bennett and Sæmundsson refraction.
//! * [`illumination`] — elongation, phase angle, illuminated fraction,
//!   bright-limb position angle.
//! * [`meeus`] — the default [`Ephemeris`]: VSOP87D Sun and ELP-2000/82 Moon.
//! * [`events`] — rise/set/transit, twilight and lunar phases, for any
//!   ephemeris.
//!
//! Lunar events and physical ephemeris (Meeus ch. 27–28, 50–54), each with
//! a closed-form Meeus series and an ephemeris-agnostic refinement:
//!
//! * [`seasons`] — equinoxes, solstices, equation of time.
//! * [`apsides`] — lunar perigee and apogee.
//! * [`nodes`] — node passages and maximum declinations.
//! * [`libration`] — librations, axis position angle, colongitude.
//! * [`eclipses`] — solar and lunar eclipses, Besselian elements, local
//!   circumstances.
//! * [`tables`] — rows for daily/hourly Sun and Moon ephemeris tables.
//!
//! The crate has no dependencies and no I/O, threads or clocks in its
//! default paths, so it builds for `wasm32-unknown-unknown`.

pub mod angle;
pub mod apsides;
pub mod coords;
pub mod deltat;
pub mod eclipses;
pub mod ephemeris;
pub mod events;
pub mod illumination;
pub mod libration;
pub mod matrix;
pub mod meeus;
pub mod nodes;
pub mod nutation;
pub mod precession;
pub mod refraction;
pub(crate) mod search;
pub mod seasons;
pub mod tables;
#[cfg(test)]
mod test_support;
pub mod time;
pub mod topocentric;
pub mod transform;

pub use coords::{Ecliptic, Equatorial, GeoLocation, Horizontal};
pub use ephemeris::Ephemeris;
pub use meeus::{MeeusEphemeris, SunSeries};
pub use nutation::NutationObliquity;
pub use refraction::Atmosphere;
pub use time::{CalendarDateTime, Instant, Weekday};
