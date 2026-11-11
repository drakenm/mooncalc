//! Bruin's ARCV–width criterion.
//!
//! Source: F. Bruin, "The first visibility of the lunar crescent", *Vistas in
//! Astronomy* 21, 331–358 (1977), fig. 9, as tabulated and fitted by Yallop
//! (1997, NAO TN 69) Table 3 and eq. 3.3:
//!
//! `ARCV > 12.4023 − 9.4878 W + 3.9512 W² − 0.5632 W³`
//!
//! with Bruin's crescent width `W = 15′ (1 − cos ARCL)` (Yallop eq. 3.4;
//! Bruin used a constant 15′ semi-diameter) and the geocentric ARCL and ARCV.
//! Bruin's curves give the minimum ARCV at the optimum moment in twilight,
//! which is where Yallop's best time comes from (§4), so the criterion is
//! evaluated at the best time. Bruin's curves stop at W = 3′. The cubic is
//! used beyond that as published (its negative W³ term makes it eventually
//! permissive).

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo, palette, two_zone};
use crate::geometry::CrescentGeometry;

/// Legend.
pub static ZONES: [ZoneInfo; 2] = [
    ZoneInfo {
        code: "V",
        description: "Visible: ARCV above Bruin's curve",
        rgb: palette::VISIBLE,
    },
    ZoneInfo {
        code: "I",
        description: "Not visible: ARCV below Bruin's curve",
        rgb: palette::NOT_VISIBLE,
    },
];

/// Bruin's crescent width (arc minutes) for a geocentric ARCL in degrees,
/// Yallop eq. 3.4.
pub fn width(arcl_deg: f64) -> f64 {
    15.0 * (1.0 - arcl_deg.to_radians().cos())
}

/// Minimum ARCV (degrees) for crescent width W (arc minutes), Yallop eq. 3.3.
pub fn arcv_limit(width_arcmin: f64) -> f64 {
    let w = width_arcmin;
    12.4023 - 9.4878 * w + 3.9512 * w * w - 0.5632 * w * w * w
}

/// Bruin (1977). The value is ARCV minus the limit, degrees.
#[derive(Clone, Copy, Debug, Default)]
pub struct Bruin;

impl Criterion for Bruin {
    fn id(&self) -> &'static str {
        "bruin"
    }
    fn name(&self) -> &'static str {
        "Bruin (1977)"
    }
    fn reference(&self) -> &'static str {
        "F. Bruin, The first visibility of the lunar crescent, Vistas in Astronomy 21, 331 (1977); \
         fitted in B. D. Yallop, NAO Technical Note No. 69 (1997), eq. 3.3"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::YallopBestTime
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        two_zone(g.arcv_deg - arcv_limit(width(g.arcl_geo_deg)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::arcv_daz;
    use super::*;

    /// Yallop (1997) Table 3 (read from Bruin's fig. 9); the cubic is a
    /// least-squares fit, so allow its residuals.
    #[test]
    fn fits_bruin_table() {
        for (w, arcv) in [
            (0.3, 10.0),
            (0.5, 8.4),
            (0.7, 7.5),
            (1.0, 6.4),
            (2.0, 4.7),
            (3.0, 4.3),
        ] {
            assert!((arcv_limit(w) - arcv).abs() < 0.2, "W {w}");
        }
    }

    #[test]
    fn width_formula() {
        assert!(width(0.0).abs() < 1e-15);
        assert!((width(60.0) - 7.5).abs() < 1e-12);
    }

    #[test]
    fn pass_fail_boundary() {
        // ARCL = ARCV = 12° → W = 0.329′, limit ≈ 9.68°.
        assert_eq!(Bruin.evaluate(&arcv_daz(12.0, 0.0)).zone, 0);
        // ARCL = ARCV = 8° → W = 0.146′, limit ≈ 11.1°.
        assert_eq!(Bruin.evaluate(&arcv_daz(8.0, 0.0)).zone, 1);
        // Find the DAZ = 0 crossing by bisection; just above it is visible.
        let (mut lo, mut hi) = (8.0, 12.0);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if Bruin.evaluate(&arcv_daz(mid, 0.0)).zone == 0 {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        let m = Bruin.evaluate(&arcv_daz(hi, 0.0)).value.unwrap();
        assert!((0.0..1e-9).contains(&m));
        assert_eq!(Bruin.evaluate(&arcv_daz(lo, 0.0)).zone, 1);
    }
}
