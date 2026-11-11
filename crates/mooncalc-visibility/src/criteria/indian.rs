//! The Indian (Schoch) ARCV–DAZ criterion.
//!
//! Source: *The Indian Astronomical Ephemeris* (India Meteorological
//! Department, 1996), p. 559, "Heliacal rising and setting of planets", based
//! on C. Schoch (1930). Reproduced and fitted by Yallop (1997, NAO TN 69)
//! Table 2 and eq. 3.2:
//!
//! `ARCV > 10.3743 − 0.0137 |DAZ| − 0.0097 DAZ²`
//!
//! from the table DAZ 0, 5, 10, 15, 20° → ARCV 10.4, 10.0, 9.3, 8.0, 6.2°.
//! ARCV is the geocentric airless arc of vision. The heliacal-visibility
//! tables use quantities **at sunset**. Yallop's q-test is the same curve
//! re-expressed in crescent width and evaluated at the best time; see
//! [`super::yallop`].
//!
//! The table extends to DAZ = 20°. Beyond that the quadratic is used as
//! published.

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo, palette, two_zone};
use crate::geometry::CrescentGeometry;

/// Legend.
pub static ZONES: [ZoneInfo; 2] = [
    ZoneInfo {
        code: "V",
        description: "Visible: ARCV above the Indian curve",
        rgb: palette::VISIBLE,
    },
    ZoneInfo {
        code: "I",
        description: "Not visible: ARCV below the Indian curve",
        rgb: palette::NOT_VISIBLE,
    },
];

/// Minimum ARCV (degrees) at a given DAZ (degrees), Yallop eq. 3.2.
pub fn arcv_limit(daz_deg: f64) -> f64 {
    let d = daz_deg.abs();
    10.3743 - 0.0137 * d - 0.0097 * d * d
}

/// Indian / Schoch criterion. The value is ARCV minus the limit, degrees.
#[derive(Clone, Copy, Debug, Default)]
pub struct Indian;

impl Criterion for Indian {
    fn id(&self) -> &'static str {
        "indian"
    }
    fn name(&self) -> &'static str {
        "Indian / Schoch (1930)"
    }
    fn reference(&self) -> &'static str {
        "The Indian Astronomical Ephemeris (1996), p. 559, after C. Schoch (1930); \
         fitted in B. D. Yallop, NAO Technical Note No. 69 (1997), eq. 3.2"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::Sunset
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        two_zone(g.arcv_deg - arcv_limit(g.daz_deg))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::arcv_daz;
    use super::*;

    #[test]
    fn fits_indian_table_within_fit_residuals() {
        for (daz, arcv) in [
            (0.0, 10.4),
            (5.0, 10.0),
            (10.0, 9.3),
            (15.0, 8.0),
            (20.0, 6.2),
        ] {
            // Least-squares quadratic: residuals up to 0.07° (at DAZ = 5°).
            assert!((arcv_limit(daz) - arcv).abs() < 0.1, "DAZ {daz}");
        }
    }

    #[test]
    fn pass_fail_boundary() {
        assert_eq!(Indian.evaluate(&arcv_daz(11.0, 0.0)).zone, 0);
        assert_eq!(Indian.evaluate(&arcv_daz(9.0, 5.0)).zone, 1);
        assert_eq!(Indian.evaluate(&arcv_daz(10.3743, 0.0)).zone, 0);
        assert_eq!(Indian.evaluate(&arcv_daz(10.374, 0.0)).zone, 1);
    }
}
