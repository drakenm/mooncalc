//! Fotheringham's ARCV–DAZ criterion.
//!
//! Source: J. K. Fotheringham, "On the smallest visible phase of the Moon",
//! *Mon. Not. R. Astron. Soc.* 70, 527 (1910). His dividing curve, drawn from
//! 76 Athens naked-eye records with ARCV and DAZ **at sunset** (geocentric,
//! no parallax or refraction):
//!
//! | DAZ (°)  | 0  | 5    | 10   | 15   | 20   | 23  |
//! |----------|----|------|------|------|------|-----|
//! | ARCV (°) | 12 | 11.9 | 11.4 | 11.0 | 10.0 | 7.7 |
//!
//! Table as reproduced by Mohd Nawawi et al., *Malaysian J. Sci.* 44(1),
//! 101–120 (2025), Table 3.1. The minimum ARCV is linearly interpolated in it.
//! Beyond DAZ = 23° the short form `ARCV = 12 − 0.008 DAZ²` (Alrefay et al.
//! 2018, *The Observatory* 138, 267, eq. 2), which also gives 7.8° at 23°, is
//! used.

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo, interpolate, palette, two_zone};
use crate::geometry::CrescentGeometry;

/// Fotheringham (1910): `(DAZ, minimum ARCV)` in degrees.
pub const TABLE: [(f64, f64); 6] = [
    (0.0, 12.0),
    (5.0, 11.9),
    (10.0, 11.4),
    (15.0, 11.0),
    (20.0, 10.0),
    (23.0, 7.7),
];

/// Legend.
pub static ZONES: [ZoneInfo; 2] = [
    ZoneInfo {
        code: "V",
        description: "Visible: ARCV above Fotheringham's curve",
        rgb: palette::VISIBLE,
    },
    ZoneInfo {
        code: "I",
        description: "Not visible: ARCV below Fotheringham's curve",
        rgb: palette::NOT_VISIBLE,
    },
];

/// Minimum ARCV (degrees) at a given DAZ (degrees).
pub fn arcv_limit(daz_deg: f64) -> f64 {
    let d = daz_deg.abs();
    if d <= TABLE[TABLE.len() - 1].0 {
        interpolate(&TABLE, d)
    } else {
        12.0 - 0.008 * d * d
    }
}

/// Fotheringham (1910). The value is ARCV minus the limit, degrees.
#[derive(Clone, Copy, Debug, Default)]
pub struct Fotheringham;

impl Criterion for Fotheringham {
    fn id(&self) -> &'static str {
        "fotheringham"
    }
    fn name(&self) -> &'static str {
        "Fotheringham (1910)"
    }
    fn reference(&self) -> &'static str {
        "J. K. Fotheringham, Mon. Not. R. Astron. Soc. 70, 527 (1910)"
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
    fn table_values_and_interpolation() {
        for (daz, arcv) in TABLE {
            assert!((arcv_limit(daz) - arcv).abs() < 1e-12);
        }
        assert!((arcv_limit(-12.5) - 11.2).abs() < 1e-12);
        // Beyond the table, the published quadratic continues the curve.
        assert!((arcv_limit(25.0) - 7.0).abs() < 1e-12);
        assert!((arcv_limit(23.0) - (12.0 - 0.008 * 529.0)).abs() < 0.1);
    }

    #[test]
    fn pass_fail_boundary() {
        assert_eq!(Fotheringham.evaluate(&arcv_daz(13.0, 0.0)).zone, 0);
        assert_eq!(Fotheringham.evaluate(&arcv_daz(11.0, 0.0)).zone, 1);
        assert_eq!(Fotheringham.evaluate(&arcv_daz(11.4, 10.0)).zone, 0);
        assert_eq!(Fotheringham.evaluate(&arcv_daz(11.39, 10.0)).zone, 1);
    }
}
