//! Time scales.

/// A moment in time, stored as a Julian Day in Universal Time (UT1; UTC is
/// treated as equal to UT1, the difference is < 0.9 s).
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Instant {
    pub jd_ut: f64,
}

/// A broken-down calendar date and time (UT unless stated otherwise).
/// Gregorian calendar from 1582-10-15, Julian calendar before. Astronomical
/// year numbering (1 BC = year 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalendarDateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: f64,
}

impl Instant {
    pub const fn from_jd_ut(jd_ut: f64) -> Self {
        Self { jd_ut }
    }

    pub fn add_days(self, days: f64) -> Self {
        Self {
            jd_ut: self.jd_ut + days,
        }
    }
}
