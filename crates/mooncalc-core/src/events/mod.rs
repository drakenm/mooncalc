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

    /// The **mean-time date** of this civil date at east longitude `lon_deg`
    /// in a time zone `utc_offset_hours` ahead of UT: the date whose
    /// [`Self::local_mean_noon`] at `lon_deg` is nearest civil noon of this
    /// date, so the two days share the same evening and morning.
    ///
    /// World visibility maps evaluate each place on the local-mean-time day of
    /// the map's date (the HMNAO convention, date line on the 180° meridian),
    /// so this is the date of the map that shows a local observer's civil
    /// evening of `self` at their longitude. It equals `self` wherever the
    /// zone offset is within 12 h of mean solar time (`λ/15°`), which is
    /// nearly everywhere; east of 180° in UTC+13/+14 (Samoa, Tonga, the Line
    /// Islands) it is the day before. See [`mean_time_day_shift`].
    pub fn mean_time_date(&self, lon_deg: f64, utc_offset_hours: f64) -> Self {
        self.add_days(mean_time_day_shift(lon_deg, utc_offset_hours))
    }

    /// The inverse of [`Self::mean_time_date`]: the civil date (zone
    /// `utc_offset_hours`) whose evening and morning the local-mean-time day
    /// `self` contains at `lon_deg`. For a world map of date `self`, this is
    /// the local civil date of the evening shown at that longitude.
    pub fn civil_date_of_mean_time(&self, lon_deg: f64, utc_offset_hours: f64) -> Self {
        self.add_days(-mean_time_day_shift(lon_deg, utc_offset_hours))
    }
}

/// Days from a civil date in a zone `utc_offset_hours` ahead of UT to its
/// local-mean-time date at east longitude `lon_deg`
/// ([`CalendarDate::mean_time_date`]): `round(λ/360° − offset/24 h)`, with
/// the longitude reduced to `[−180°, 180°)` as in
/// [`CalendarDate::local_mean_noon`].
///
/// Civil noon of date `D` is `D 0h UT + 12h − offset`; local mean noon of
/// date `D′` is `D′ 0h UT + 12h − λ/15`. Their difference,
/// `(D′ − D) − (λ/360 − offset/24)` days, is smallest for this shift, and the
/// evenings (each the sunset after the Sun's transit nearest the anchor) are
/// then the same. The shift is −1 where the civil date runs ahead of mean
/// time by more than half a day (Apia, 171.8° W, UTC+13: −1.02 → −1), +1
/// where it runs behind, and 0 elsewhere (London; Ürümqi on UTC+8 at
/// 87.6° E: −0.09 → 0). The tie at exactly half a day is resolved away from
/// zero; there the transits lie about 12 h from both anchors and the sunset
/// assignment depends on the equation of time, a case no inhabited zone hits.
pub fn mean_time_day_shift(lon_deg: f64, utc_offset_hours: f64) -> i32 {
    let lon = (lon_deg + 180.0).rem_euclid(360.0) - 180.0;
    (lon / 360.0 - utc_offset_hours / 24.0).round() as i32
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

    #[test]
    fn mean_time_date_follows_the_180_degree_date_line() {
        let d = CalendarDate::new(2024, 3, 10);
        // Apia and Nuku'alofa (UTC+13), Kiritimati (UTC+14), the Chatham
        // Islands (UTC+12:45 at 176.6° W) and 180° itself on UTC+12 (180° is
        // the map's western edge) are a day ahead of mean time: their civil
        // 10 March is the map's 9 March.
        for (lon, tz) in [
            (-171.7667, 13.0),
            (-175.2011, 13.0),
            (-157.4, 14.0),
            (-176.56, 12.75),
            (180.0, 12.0),
        ] {
            assert_eq!(mean_time_day_shift(lon, tz), -1, "{lon} {tz}");
            assert_eq!(d.mean_time_date(lon, tz), CalendarDate::new(2024, 3, 9));
            assert_eq!(
                d.civil_date_of_mean_time(lon, tz),
                CalendarDate::new(2024, 3, 11)
            );
            // The anchors are then within 12 h of each other.
            let civil_noon = d.jd0() + 0.5 - tz / 24.0;
            let mean_noon = d.mean_time_date(lon, tz).local_mean_noon(lon).jd_ut;
            assert!((civil_noon - mean_noon).abs() < 0.5);
        }
        // No shift: London, Ürümqi on Beijing time, Tarawa (UTC+12 at
        // 173° E), Suva (UTC+12 at 178.4° E), Honolulu, and 180° on UTC-12.
        for (lon, tz) in [
            (-0.1276, 0.0),
            (87.6, 8.0),
            (172.977, 12.0),
            (178.44, 12.0),
            (-157.86, -10.0),
            (180.0, -12.0),
        ] {
            assert_eq!(mean_time_day_shift(lon, tz), 0, "{lon} {tz}");
            assert_eq!(d.mean_time_date(lon, tz), d);
            assert_eq!(d.civil_date_of_mean_time(lon, tz), d);
        }
        // A civil date behind mean time (173° E kept on UTC-10) maps forward.
        assert_eq!(mean_time_day_shift(173.0, -10.0), 1);
        assert_eq!(
            d.mean_time_date(173.0, -10.0),
            CalendarDate::new(2024, 3, 11)
        );
    }
}
