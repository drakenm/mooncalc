//! The criterion of Alrefay et al. (2018).
//!
//! Source: T. Alrefay, S. Alsaab, F. Alshehri, A. Alghamdi et al., "Analysis
//! of observations of earliest visibility of the lunar crescent", *The
//! Observatory* 138, 267–291 (2018), eqs. 7–9. Fitted to 478 clear-sky
//! sightings by trained observers of Saudi Arabia's National Center for
//! Astronomy (1988–2015):
//!
//! * naked eye (eq. 8): `ARCV > 9.34 − 4.51 W + 3.3 W² − 1.01 W³`
//! * optical aid (eq. 9): `ARCV > 7.83 − 4.35 W + 3.22 W² − 1.02 W³`
//!
//! with `W = SD (1 − cos ARCL)` and a fixed SD of 16′ (eq. 7). ARCV and ARCL
//! are the geocentric airless arcs. The paper's tables give ARCV, ARCL and the
//! Moon's age at sunset, and its curves describe ARCV "after sunset", so the
//! criterion is evaluated **at sunset**.

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo};
use crate::geometry::CrescentGeometry;

/// Fixed lunar semi-diameter adopted by Alrefay et al. (2018), arc minutes.
pub const SEMI_DIAMETER_ARCMIN: f64 = 16.0;

/// Legend.
pub static ZONES: [ZoneInfo; 3] = [
    ZoneInfo {
        code: "A",
        description: "Visible to the naked eye",
        rgb: [46, 158, 63],
    },
    ZoneInfo {
        code: "B",
        description: "Visible with optical aid only",
        rgb: [0, 112, 192],
    },
    ZoneInfo {
        code: "C",
        description: "Not visible",
        rgb: [150, 150, 150],
    },
];

/// Crescent width (arc minutes) for a geocentric ARCL in degrees, eq. 7.
pub fn width(arcl_deg: f64) -> f64 {
    SEMI_DIAMETER_ARCMIN * (1.0 - arcl_deg.to_radians().cos())
}

/// Minimum ARCV (degrees) for naked-eye visibility, eq. 8.
pub fn naked_eye_limit(w: f64) -> f64 {
    9.34 - 4.51 * w + 3.3 * w * w - 1.01 * w * w * w
}

/// Minimum ARCV (degrees) for optically aided visibility, eq. 9.
pub fn optical_aid_limit(w: f64) -> f64 {
    7.83 - 4.35 * w + 3.22 * w * w - 1.02 * w * w * w
}

/// Alrefay et al. (2018). The value is ARCV minus the naked-eye limit,
/// degrees.
#[derive(Clone, Copy, Debug, Default)]
pub struct Alrefay;

impl Criterion for Alrefay {
    fn id(&self) -> &'static str {
        "alrefay"
    }
    fn name(&self) -> &'static str {
        "Alrefay et al. (2018)"
    }
    fn reference(&self) -> &'static str {
        "T. Alrefay et al., Analysis of observations of earliest visibility of the lunar crescent, \
         The Observatory 138, 267-291 (2018)"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::Sunset
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        let w = width(g.arcl_geo_deg);
        let zone = if g.arcv_deg > naked_eye_limit(w) {
            0
        } else if g.arcv_deg > optical_aid_limit(w) {
            1
        } else {
            2
        };
        CriterionResult {
            zone,
            value: Some(g.arcv_deg - naked_eye_limit(w)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::arcv_daz;
    use super::*;

    /// Tables IV and V of the paper: points (DAZ, ARCV) on the authors'
    /// ARCV–DAZ dividing curves, which eqs. 8 and 9 fit in terms of W.
    #[test]
    fn fits_tables_iv_and_v() {
        let naked = [
            (0.0, 8.70),
            (5.0, 8.37),
            (10.0, 7.89),
            (15.0, 7.53),
            (20.0, 7.02),
            (25.0, 6.45),
        ];
        let aided = [
            (0.0, 7.40),
            (5.0, 7.05),
            (10.0, 6.64),
            (15.0, 6.16),
            (20.0, 5.62),
            (25.0, 5.01),
        ];
        for (daz, arcv) in naked {
            let w = width(arcv_daz(arcv, daz).arcl_geo_deg);
            assert!((naked_eye_limit(w) - arcv).abs() < 0.15, "naked DAZ {daz}");
        }
        for (daz, arcv) in aided {
            let w = width(arcv_daz(arcv, daz).arcl_geo_deg);
            assert!(
                (optical_aid_limit(w) - arcv).abs() < 0.15,
                "aided DAZ {daz}"
            );
        }
        // Table IV lists W = 0.39′ for DAZ = 10°, ARCV = 7.89°.
        assert!((width(arcv_daz(7.89, 10.0).arcl_geo_deg) - 0.39).abs() < 0.005);
    }

    #[test]
    fn classification() {
        assert_eq!(Alrefay.evaluate(&arcv_daz(10.0, 0.0)).zone, 0);
        assert_eq!(Alrefay.evaluate(&arcv_daz(8.0, 0.0)).zone, 1);
        assert_eq!(Alrefay.evaluate(&arcv_daz(6.0, 0.0)).zone, 2);
    }

    #[test]
    fn boundary() {
        // Bisect the naked-eye crossing at DAZ = 5°.
        let (mut lo, mut hi) = (7.0, 10.0);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if Alrefay.evaluate(&arcv_daz(mid, 5.0)).zone == 0 {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        assert!((hi - 8.37).abs() < 0.15);
        assert_eq!(Alrefay.evaluate(&arcv_daz(lo, 5.0)).zone, 1);
        assert!(Alrefay.evaluate(&arcv_daz(hi, 5.0)).value.unwrap().abs() < 1e-9);
    }
}
