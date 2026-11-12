//! One-quantity rules: moonset after sunset, minimum age, minimum lag.
//!
//! None of these predicts visibility on its own. Schaefer (1996, *QJRAS* 37,
//! 759) and Odeh (2004, *Exp. Astron.* 18, 39) show that Age or Lag alone
//! has little predictive value. They are offered as quick filters and for
//! comparison with the classical rules.
//!
//! The default thresholds are the observational records quoted by Odeh
//! (2004, §7): the youngest naked-eye crescent (Pierce, 1990 Feb 25) was
//! 15 h 01 min after geocentric conjunction, and the shortest naked-eye lag was
//! 29 min.

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo, palette, two_zone};
use crate::geometry::CrescentGeometry;

/// Default minimum age: youngest naked-eye sighting, geocentric age 15 h 01 min
/// (Odeh 2004, §7.1).
pub const DEFAULT_MIN_AGE_HOURS: f64 = 15.0 + 1.0 / 60.0;
/// Default minimum lag: shortest naked-eye lag, 29 min (Odeh 2004, §7.2).
pub const DEFAULT_MIN_LAG_MINUTES: f64 = 29.0;

/// Legend of [`MoonsetAfterSunset`].
pub static MOONSET_ZONES: [ZoneInfo; 2] = [
    ZoneInfo {
        code: "V",
        description: "Moon sets after the Sun",
        rgb: palette::VISIBLE,
    },
    ZoneInfo {
        code: "I",
        description: "Moon sets before the Sun",
        rgb: palette::NOT_VISIBLE,
    },
];

/// Legend of [`MinimumAge`].
pub static AGE_ZONES: [ZoneInfo; 2] = [
    ZoneInfo {
        code: "V",
        description: "Moon at least as old as the threshold",
        rgb: palette::VISIBLE,
    },
    ZoneInfo {
        code: "I",
        description: "Moon younger than the threshold",
        rgb: palette::NOT_VISIBLE,
    },
];

/// Legend of [`MinimumLag`].
pub static LAG_ZONES: [ZoneInfo; 2] = [
    ZoneInfo {
        code: "V",
        description: "Lag at least the threshold",
        rgb: palette::VISIBLE,
    },
    ZoneInfo {
        code: "I",
        description: "Lag shorter than the threshold",
        rgb: palette::NOT_VISIBLE,
    },
];

/// The Moon sets after the Sun (evening) or rises before it (morning): the
/// necessary condition behind every other criterion. The value is the lag in
/// minutes.
#[derive(Clone, Copy, Debug, Default)]
pub struct MoonsetAfterSunset;

impl Criterion for MoonsetAfterSunset {
    fn id(&self) -> &'static str {
        "moonset-after-sunset"
    }
    fn name(&self) -> &'static str {
        "Moonset after sunset"
    }
    fn reference(&self) -> &'static str {
        "Necessary condition used by every criterion; see B. D. Yallop, \
         NAO Technical Note No. 69 (1997)"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::Sunset
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &MOONSET_ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        CriterionResult {
            zone: usize::from(g.lag_minutes <= 0.0),
            value: Some(g.lag_minutes),
        }
    }
}

/// Moon age at sunset of at least `min_hours`, from geocentric conjunction
/// ([`CrescentGeometry::age_hours`]). The value is age minus the threshold, hours.
#[derive(Clone, Copy, Debug)]
pub struct MinimumAge {
    /// Threshold, hours.
    pub min_hours: f64,
}

impl Default for MinimumAge {
    fn default() -> Self {
        Self {
            min_hours: DEFAULT_MIN_AGE_HOURS,
        }
    }
}

impl Criterion for MinimumAge {
    fn id(&self) -> &'static str {
        "age"
    }
    fn name(&self) -> &'static str {
        "Moon age"
    }
    fn reference(&self) -> &'static str {
        "Age rule; default threshold is the youngest naked-eye sighting quoted by \
         M. Sh. Odeh, Experimental Astronomy 18, 39 (2004), sec. 7.1"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::Sunset
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &AGE_ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        two_zone(g.age_hours - self.min_hours)
    }
}

/// Lag of at least `min_minutes` ([`CrescentGeometry::lag_minutes`]). The
/// value is lag minus the threshold, minutes.
#[derive(Clone, Copy, Debug)]
pub struct MinimumLag {
    /// Threshold, minutes.
    pub min_minutes: f64,
}

impl Default for MinimumLag {
    fn default() -> Self {
        Self {
            min_minutes: DEFAULT_MIN_LAG_MINUTES,
        }
    }
}

impl Criterion for MinimumLag {
    fn id(&self) -> &'static str {
        "lag"
    }
    fn name(&self) -> &'static str {
        "Moonset lag"
    }
    fn reference(&self) -> &'static str {
        "Lag rule; default threshold is the shortest naked-eye lag quoted by \
         M. Sh. Odeh, Experimental Astronomy 18, 39 (2004), sec. 7.2"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::Sunset
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &LAG_ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        two_zone(g.lag_minutes - self.min_minutes)
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
    fn moonset_after_sunset() {
        assert_eq!(MoonsetAfterSunset.evaluate(&at(20.0, 30.0)).zone, 0);
        assert_eq!(MoonsetAfterSunset.evaluate(&at(20.0, -10.0)).zone, 1);
        assert_eq!(MoonsetAfterSunset.evaluate(&at(20.0, 0.0)).zone, 1);
        assert_eq!(MoonsetAfterSunset.evaluate(&at(20.0, 0.5)).zone, 0);
    }

    #[test]
    fn minimum_age() {
        let c = MinimumAge::default();
        assert_eq!(c.evaluate(&at(30.0, 0.0)).zone, 0);
        assert_eq!(c.evaluate(&at(10.0, 0.0)).zone, 1);
        assert_eq!(c.evaluate(&at(DEFAULT_MIN_AGE_HOURS, 0.0)).zone, 0);
        assert_eq!(c.evaluate(&at(15.0, 0.0)).zone, 1);
        let c = MinimumAge { min_hours: 24.0 };
        assert_eq!(c.evaluate(&at(20.0, 0.0)).zone, 1);
        assert!((c.evaluate(&at(26.0, 0.0)).value.unwrap() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn minimum_lag() {
        let c = MinimumLag::default();
        assert_eq!(c.evaluate(&at(0.0, 60.0)).zone, 0);
        assert_eq!(c.evaluate(&at(0.0, 10.0)).zone, 1);
        assert_eq!(c.evaluate(&at(0.0, 29.0)).zone, 0);
        assert_eq!(c.evaluate(&at(0.0, 28.9)).zone, 1);
        let c = MinimumLag { min_minutes: 48.0 };
        assert_eq!(c.evaluate(&at(0.0, 40.0)).zone, 1);
    }
}
