//! Crescent visibility.
//!
//! * [`geometry`] holds [`CrescentGeometry`], the observing circumstances a
//!   criterion needs (computed from an ephemeris by the circumstances work
//!   package; criteria themselves are pure functions of it).
//! * [`circumstances`] computes it from an ephemeris for a place and date
//!   (sunset/moonset, lag, best time, conjunction, polar cases).
//! * [`criteria`] holds the [`Criterion`] trait and every implemented criterion.
//! * [`calendar`] builds Hijri month starts from crescent sightings (at a
//!   place or anywhere in a region) and from astronomical rules (Umm
//!   al-Qura, conjunction rules), for `mooncalc_hijri::ObservedCalendar`.

pub mod calendar;
pub mod circumstances;
pub mod criteria;
pub mod geometry;

pub use criteria::{Criterion, CriterionResult, ZoneInfo};
pub use geometry::CrescentGeometry;
