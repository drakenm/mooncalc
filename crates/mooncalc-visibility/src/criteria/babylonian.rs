//! The Babylonian rule.
//!
//! Source: as summarised by M. Sh. Odeh, *Exp. Astron.* 18, 39 (2004), §2.1:
//! the crescent is visible to the naked eye if, at local sunset, the Moon is
//! more than 24 hours old **and** sets more than 48 minutes after the Sun.
//! Yallop (1997, NAO TN 69, §1) likewise notes that the earliest methods used
//! Age and Lag.
//!
//! Uses [`CrescentGeometry::age_hours`] (geocentric) at sunset and
//! [`CrescentGeometry::lag_minutes`].

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo, palette};
use crate::geometry::CrescentGeometry;

/// Minimum age, hours (exclusive).
pub const MIN_AGE_HOURS: f64 = 24.0;
/// Minimum lag, minutes (exclusive).
pub const MIN_LAG_MINUTES: f64 = 48.0;

/// Legend.
pub static ZONES: [ZoneInfo; 2] = [
    ZoneInfo {
        code: "V",
        description: "Visible: age > 24 h and lag > 48 min",
        rgb: palette::VISIBLE,
    },
    ZoneInfo {
        code: "I",
        description: "Not visible",
        rgb: palette::NOT_VISIBLE,
    },
];

/// Babylonian age-and-lag rule. Reports no figure of merit.
#[derive(Clone, Copy, Debug, Default)]
pub struct Babylonian;

impl Criterion for Babylonian {
    fn id(&self) -> &'static str {
        "babylonian"
    }
    fn name(&self) -> &'static str {
        "Babylonian (age > 24 h, lag > 48 min)"
    }
    fn reference(&self) -> &'static str {
        "Babylonian rule as given by M. Sh. Odeh, Experimental Astronomy 18, 39 (2004), sec. 2.1"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::Sunset
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        let visible = g.age_hours > MIN_AGE_HOURS && g.lag_minutes > MIN_LAG_MINUTES;
        CriterionResult {
            zone: usize::from(!visible),
            value: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(age_hours: f64, lag_minutes: f64) -> CrescentGeometry {
        CrescentGeometry {
            age_hours,
            lag_minutes,
            ..Default::default()
        }
    }

    #[test]
    fn pass_fail_boundary() {
        assert_eq!(Babylonian.evaluate(&at(30.0, 60.0)).zone, 0);
        assert_eq!(Babylonian.evaluate(&at(18.0, 35.0)).zone, 1);
        // Both conditions are required.
        assert_eq!(Babylonian.evaluate(&at(30.0, 40.0)).zone, 1);
        assert_eq!(Babylonian.evaluate(&at(20.0, 60.0)).zone, 1);
        // Strict inequalities.
        assert_eq!(Babylonian.evaluate(&at(24.0, 60.0)).zone, 1);
        assert_eq!(Babylonian.evaluate(&at(30.0, 48.0)).zone, 1);
        assert_eq!(Babylonian.evaluate(&at(24.01, 48.01)).zone, 0);
    }
}
