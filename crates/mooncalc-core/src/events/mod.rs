//! Events: rise, set, transit and twilight of the Sun and Moon, and the
//! lunar phases.
//!
//! Everything is backend-agnostic. Phases take any [`crate::Ephemeris`];
//! horizon events take a [`PositionSource`], which is either an ephemeris
//! evaluated directly ([`DirectSource`]) or an interpolated table
//! ([`ApparentTable`]) for fast repeated searches such as world maps.
//!
//! * [`riseset`]: [`day_events`], [`twilight`], and the [`Searcher`] behind
//!   them (robust bracketing search and a fast local solver).
//! * [`phases`]: new Moon, quarters and full Moon by root finding on
//!   `λ☾ − λ☉`, with Meeus ch. 49 as the first guess; the Moon's age.
//! * [`source`], [`observer`]: position sources and the observer's local
//!   frame. [`solve`]: Brent's method and bracketing.

pub mod observer;
pub mod phases;
pub mod riseset;
pub mod solve;
pub mod source;

pub use observer::{LocalFrame, Look, Observer};
pub use phases::{Phase, meeus_phase_jde, moon_age_days, nearest_new_moon, phase_near};
pub use riseset::{
    Circumpolar, DayEvents, Event, EventKind, Horizon, Searcher, Twilight, day_events, twilight,
};
pub use source::{ApparentTable, Body, DirectSource, PositionSource};

use crate::time::{Instant, julian_day};

/// A calendar date without a time of day (Gregorian from 1582-10-15, Julian
/// before; astronomical year numbering).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CalendarDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl CalendarDate {
    pub const fn new(year: i32, month: u32, day: u32) -> Self {
        Self { year, month, day }
    }

    /// Julian Day of 0h UT on this date.
    pub fn jd0(&self) -> f64 {
        julian_day(self.year, self.month, self.day as f64)
    }

    /// The date of a Julian Day (the UT date when `jd` is UT).
    pub fn from_jd(jd: f64) -> Self {
        let c = Instant::from_jd_ut(jd).to_calendar();
        Self::new(c.year, c.month, c.day)
    }

    /// The date `days` later (negative for earlier).
    pub fn add_days(&self, days: i32) -> Self {
        Self::from_jd(self.jd0() + days as f64 + 0.5)
    }

    /// UT bounds `[start, end)` of this date in a time zone `utc_offset_hours`
    /// ahead of UT (e.g. +3 for Mecca).
    pub fn local_day(&self, utc_offset_hours: f64) -> (Instant, Instant) {
        let start = self.jd0() - utc_offset_hours / 24.0;
        (Instant::from_jd_ut(start), Instant::from_jd_ut(start + 1.0))
    }

    /// 12h **local mean time** on this date at east longitude `lon_deg`, as
    /// UT: `0h UT + 12h − λ/15`. Longitudes are first reduced to
    /// `[−180°, 180°)`, so the calendar date line is the 180° meridian (not
    /// the political International Date Line).
    pub fn local_mean_noon(&self, lon_deg: f64) -> Instant {
        let lon = (lon_deg + 180.0).rem_euclid(360.0) - 180.0;
        Instant::from_jd_ut(self.jd0() + 0.5 - lon / 360.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_date_helpers() {
        let d = CalendarDate::new(2024, 2, 28);
        assert_eq!(d.add_days(1), CalendarDate::new(2024, 2, 29));
        assert_eq!(d.add_days(2), CalendarDate::new(2024, 3, 1));
        assert_eq!(d.add_days(-28), CalendarDate::new(2024, 1, 31));
        assert_eq!(CalendarDate::from_jd(d.jd0() + 0.99), d);
        let (s, e) = d.local_day(3.0);
        assert!((e.jd_ut - s.jd_ut - 1.0).abs() < 1e-12);
        assert!((s.jd_ut - (d.jd0() - 0.125)).abs() < 1e-12);
        // Mean noon at 90° E is 06h UT; at 180° E it is the same as at 180° W.
        assert!((d.local_mean_noon(90.0).jd_ut - (d.jd0() + 0.25)).abs() < 1e-12);
        assert_eq!(d.local_mean_noon(180.0), d.local_mean_noon(-180.0));
        assert!((d.local_mean_noon(-179.0).jd_ut - (d.jd0() + 0.5 + 179.0 / 360.0)).abs() < 1e-12);
    }
}
