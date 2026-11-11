//! Maunder's ARCV–DAZ criterion.
//!
//! Source: E. W. Maunder,
//! *J. Brit. Astron. Assoc.* 21, 355 (1911), table on p. 359, as reproduced
//! and fitted by Yallop (1997, NAO TN 69) Table 1 and eq. 3.1:
//!
//! `ARCV > 11 − |DAZ|/20 − DAZ²/100`
//!
//! which fits the table (DAZ 0, 5, 10, 15, 20° → ARCV 11.0, 10.5, 9.5, 8.0,
//! 6.0°) exactly. ARCV is the geocentric airless arc of vision. Like
//! Fotheringham, Maunder worked with quantities **at sunset**.
//!
//! The data extend to DAZ = 20°. Beyond that the quadratic is used as
//! published, which becomes very permissive past about 25°.

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo, palette, two_zone};
use crate::geometry::CrescentGeometry;

/// Legend.
pub static ZONES: [ZoneInfo; 2] = [
    ZoneInfo {
        code: "V",
        description: "Visible: ARCV above Maunder's curve",
        rgb: palette::VISIBLE,
    },
    ZoneInfo {
        code: "I",
        description: "Not visible: ARCV below Maunder's curve",
        rgb: palette::NOT_VISIBLE,
    },
];

/// Minimum ARCV (degrees) at a given DAZ (degrees), Yallop eq. 3.1.
pub fn arcv_limit(daz_deg: f64) -> f64 {
    let d = daz_deg.abs();
    11.0 - d / 20.0 - d * d / 100.0
}

/// Maunder (1911). The value is ARCV minus the limit, degrees.
#[derive(Clone, Copy, Debug, Default)]
pub struct Maunder;

impl Criterion for Maunder {
    fn id(&self) -> &'static str {
        "maunder"
    }
    fn name(&self) -> &'static str {
        "Maunder (1911)"
    }
    fn reference(&self) -> &'static str {
        "E. W. Maunder, J. Brit. Astron. Assoc. 21, 355 (1911); \
         fitted in B. D. Yallop, NAO Technical Note No. 69 (1997), eq. 3.1"
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
    fn fits_maunder_table_exactly() {
        for (daz, arcv) in [
            (0.0, 11.0),
            (5.0, 10.5),
            (10.0, 9.5),
            (15.0, 8.0),
            (20.0, 6.0),
        ] {
            assert!((arcv_limit(daz) - arcv).abs() < 1e-12);
            assert!((arcv_limit(-daz) - arcv).abs() < 1e-12);
        }
    }

    #[test]
    fn pass_fail_boundary() {
        assert_eq!(Maunder.evaluate(&arcv_daz(12.0, 5.0)).zone, 0);
        assert_eq!(Maunder.evaluate(&arcv_daz(9.0, 5.0)).zone, 1);
        assert_eq!(Maunder.evaluate(&arcv_daz(9.5, 10.0)).zone, 0);
        assert_eq!(Maunder.evaluate(&arcv_daz(9.49, 10.0)).zone, 1);
    }
}
