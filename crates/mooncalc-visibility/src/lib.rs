//! Crescent visibility.
//!
//! * [`geometry`] holds [`CrescentGeometry`], the observing circumstances a
//!   criterion needs (computed from an ephemeris by the circumstances work
//!   package; criteria themselves are pure functions of it).
//! * [`circumstances`] computes it from an ephemeris for a place and date
//!   (sunset/moonset, lag, best time, conjunction, polar cases).
//! * [`criteria`] holds the [`Criterion`] trait and every implemented criterion.

pub mod circumstances;
pub mod criteria;
pub mod geometry;

pub use criteria::{Criterion, CriterionResult, ZoneInfo};
pub use geometry::CrescentGeometry;
