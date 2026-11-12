//! The Istanbul 1978 criterion.
//!
//! Source: resolution of the International Conference on the Lunar Crescent
//! (Istanbul, November 1978): the crescent can be seen if, at sunset, the
//! Moon's altitude is at least 5° and its elongation from the Sun at least 8°.
//! The thresholds are as tabulated by Mohd Nawawi et al., *Malaysian J. Sci.*
//! 44(1), 101–120 (2025), Table 1 (the 2016 Istanbul congress adopted the same
//! values).
//!
//! The resolution does not say whether the altitude is apparent or airless,
//! or whether the elongation is topocentric. This module uses the observer's
//! view: the apparent topocentric altitude of the Moon's centre
//! ([`CrescentGeometry::moon_alt_deg`]) and the topocentric elongation
//! ([`CrescentGeometry::arcl_deg`]), at sunset.

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo, palette};
use crate::geometry::CrescentGeometry;

/// Minimum Moon altitude at sunset, degrees (inclusive).
pub const MIN_ALTITUDE_DEG: f64 = 5.0;
/// Minimum elongation at sunset, degrees (inclusive).
pub const MIN_ELONGATION_DEG: f64 = 8.0;

/// Legend.
pub static ZONES: [ZoneInfo; 2] = [
    ZoneInfo {
        code: "V",
        description: "Visible: altitude ≥ 5° and elongation ≥ 8° at sunset",
        rgb: palette::VISIBLE,
    },
    ZoneInfo {
        code: "I",
        description: "Not visible",
        rgb: palette::NOT_VISIBLE,
    },
];

/// Istanbul 1978. The value is the smaller of the two margins (altitude − 5°,
/// elongation − 8°), so it is non-negative exactly when both hold.
#[derive(Clone, Copy, Debug, Default)]
pub struct Istanbul1978;

impl Criterion for Istanbul1978 {
    fn id(&self) -> &'static str {
        "istanbul-1978"
    }
    fn name(&self) -> &'static str {
        "Istanbul (1978)"
    }
    fn reference(&self) -> &'static str {
        "International Conference on the Lunar Crescent, Istanbul (1978); thresholds as listed by \
         Mohd Nawawi et al., Malaysian Journal of Science 44(1), 101-120 (2025)"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::Sunset
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        let margin = (g.moon_alt_deg - MIN_ALTITUDE_DEG).min(g.arcl_deg - MIN_ELONGATION_DEG);
        super::two_zone(margin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(alt: f64, elong: f64) -> CrescentGeometry {
        CrescentGeometry {
            moon_alt_deg: alt,
            arcl_deg: elong,
            ..Default::default()
        }
    }

    #[test]
    fn pass_fail_boundary() {
        assert_eq!(Istanbul1978.evaluate(&at(8.0, 12.0)).zone, 0);
        assert_eq!(Istanbul1978.evaluate(&at(2.0, 6.0)).zone, 1);
        assert_eq!(Istanbul1978.evaluate(&at(4.9, 12.0)).zone, 1);
        assert_eq!(Istanbul1978.evaluate(&at(8.0, 7.9)).zone, 1);
        assert_eq!(Istanbul1978.evaluate(&at(5.0, 8.0)).zone, 0);
        let v = Istanbul1978.evaluate(&at(6.0, 8.5)).value.unwrap();
        assert!((v - 0.5).abs() < 1e-12);
    }
}
