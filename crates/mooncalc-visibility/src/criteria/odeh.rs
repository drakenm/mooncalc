//! Odeh's criterion.
//!
//! Source: M. Sh. Odeh, "New criterion for lunar crescent visibility",
//! *Experimental Astronomy* 18, 39–64 (2004; DOI 10.1007/s10686-005-9002-5),
//! §5, Table V and eq. 2:
//!
//! `V = ARCV − (−0.1018 W³ + 0.7319 W² − 6.3226 W + 7.1651)`
//!
//! where ARCV is the **airless topocentric** arc of vision in degrees and W the
//! **topocentric** crescent width in arc minutes, both at Yallop's best time
//! `Tb = Ts + 4/9 Lag` (Odeh eq. 1). Zones: A `V ≥ 5.65`, B `2 ≤ V < 5.65`,
//! C `−0.96 ≤ V < 2`, D `V < −0.96`.
//!
//! Places where the Moon sets before the Sun, or where the best time precedes
//! conjunction, are "not visible"; see [`super::crescent_present`].

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo, zone_by_lower_bounds};
use crate::geometry::CrescentGeometry;

/// Inclusive lower bounds of V for zones A, B and C (Odeh 2004, §5).
pub const V_BOUNDS: [f64; 3] = [5.65, 2.0, -0.96];

/// Legend from Odeh (2004) §5, in the green / blue / magenta / red scheme of
/// ICOP's visibility maps.
pub static ZONES: [ZoneInfo; 4] = [
    ZoneInfo {
        code: "A",
        description: "Visible by naked eye",
        rgb: [0, 176, 80],
    },
    ZoneInfo {
        code: "B",
        description: "Visible by optical aid, could be seen by naked eye",
        rgb: [0, 112, 192],
    },
    ZoneInfo {
        code: "C",
        description: "Visible by optical aid only",
        rgb: [204, 0, 204],
    },
    ZoneInfo {
        code: "D",
        description: "Not visible even by optical aid",
        rgb: [220, 40, 40],
    },
];

/// The ARCV polynomial of Odeh eq. 2 for topocentric width W (arc minutes).
/// It equals Yallop's eq. 3.6 shifted down by 4.672°.
pub fn arcv_curve(width_arcmin: f64) -> f64 {
    let w = width_arcmin;
    -0.1018 * w * w * w + 0.7319 * w * w - 6.3226 * w + 7.1651
}

/// Odeh's V (eq. 2) from the airless topocentric ARCV (degrees) and the
/// topocentric width W (arc minutes).
pub fn v(arcv_topo_deg: f64, width_topo_arcmin: f64) -> f64 {
    arcv_topo_deg - arcv_curve(width_topo_arcmin)
}

/// Zone index (0 = A ... 3 = D) for a given V.
pub fn zone_for_v(v: f64) -> usize {
    zone_by_lower_bounds(v, &V_BOUNDS, true)
}

/// Odeh's crescent width W in arc minutes: `SD′ (1 − cos ARCL)` with the
/// topocentric semi-diameter and the **topocentric** elongation (Odeh 2004,
/// §5). This differs from Yallop's W′, which uses the geocentric ARCL; near
/// the visibility limit the difference shifts V by about 0.25.
///
/// Falls back to [`CrescentGeometry::width_topo_arcmin`] when no topocentric
/// semi-diameter is set (hand-built geometries, e.g. tests on tabulated W).
pub fn width_arcmin(g: &CrescentGeometry) -> f64 {
    if g.semi_diameter_topo_arcmin > 0.0 {
        g.semi_diameter_topo_arcmin * (1.0 - g.arcl_deg.to_radians().cos())
    } else {
        g.width_topo_arcmin
    }
}

/// Odeh (2004). Uses [`CrescentGeometry::arcv_topo_deg`] and the width from
/// [`width_arcmin`], at the best time.
#[derive(Clone, Copy, Debug, Default)]
pub struct Odeh;

impl Criterion for Odeh {
    fn id(&self) -> &'static str {
        "odeh"
    }
    fn name(&self) -> &'static str {
        "Odeh (2004)"
    }
    fn reference(&self) -> &'static str {
        "M. Sh. Odeh, New criterion for lunar crescent visibility, \
         Experimental Astronomy 18, 39-64 (2004)"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::YallopBestTime
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        let v = v(g.arcv_topo_deg, width_arcmin(g));
        CriterionResult {
            zone: zone_for_v(v),
            value: Some(v),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Odeh (2004) Table V: W (arcmin) and the zone-limit ARCV1 (C), ARCV2
    /// (B) and ARCV3 (A), tabulated to 0.1°.
    const TABLE_V: [(f64, [f64; 3]); 9] = [
        (0.1, [5.6, 8.5, 12.2]),
        (0.2, [5.0, 7.9, 11.6]),
        (0.3, [4.4, 7.3, 11.0]),
        (0.4, [3.8, 6.7, 10.4]),
        (0.5, [3.2, 6.2, 9.8]),
        (0.6, [2.7, 5.6, 9.3]),
        (0.7, [2.1, 5.1, 8.7]),
        (0.8, [1.6, 4.5, 8.2]),
        (0.9, [1.0, 4.0, 7.6]),
    ];

    #[test]
    fn equation_reproduces_table_v() {
        for (w, [arcv1, arcv2, arcv3]) in TABLE_V {
            // Each tabulated limit sits on the matching V threshold.
            for (arcv, bound) in [(arcv1, -0.96), (arcv2, 2.0), (arcv3, 5.65)] {
                let got = v(arcv, w);
                assert!((got - bound).abs() < 0.06, "W {w}: V({arcv}) = {got}");
            }
        }
    }

    #[test]
    fn classification_around_table_v() {
        let geom = |arcv: f64, w: f64| CrescentGeometry {
            arcv_topo_deg: arcv,
            width_topo_arcmin: w,
            ..Default::default()
        };
        // W = 0.5′: limits 3.2 / 6.2 / 9.8.
        assert_eq!(Odeh.evaluate(&geom(10.5, 0.5)).zone, 0);
        assert_eq!(Odeh.evaluate(&geom(7.0, 0.5)).zone, 1);
        assert_eq!(Odeh.evaluate(&geom(4.0, 0.5)).zone, 2);
        assert_eq!(Odeh.evaluate(&geom(2.5, 0.5)).zone, 3);
    }

    #[test]
    fn width_uses_topocentric_elongation() {
        let g = CrescentGeometry {
            arcl_deg: 12.0,
            semi_diameter_topo_arcmin: 15.5,
            width_topo_arcmin: 9.9, // ignored when SD′ is present
            ..Default::default()
        };
        let expected = 15.5 * (1.0 - 12.0f64.to_radians().cos());
        assert!((width_arcmin(&g) - expected).abs() < 1e-12);
    }

    #[test]
    fn boundaries_are_inclusive() {
        assert_eq!(zone_for_v(5.65), 0);
        assert_eq!(zone_for_v(5.649), 1);
        assert_eq!(zone_for_v(2.0), 1);
        assert_eq!(zone_for_v(1.999), 2);
        assert_eq!(zone_for_v(-0.96), 2);
        assert_eq!(zone_for_v(-0.961), 3);
    }

    #[test]
    fn uses_topocentric_arcv() {
        let g = CrescentGeometry {
            arcv_deg: 99.0,
            arcv_topo_deg: 8.0,
            width_topo_arcmin: 0.4,
            ..Default::default()
        };
        assert!((Odeh.evaluate(&g).value.unwrap() - v(8.0, 0.4)).abs() < 1e-12);
    }

    #[test]
    fn curve_is_yallop_shifted() {
        for w in [0.1, 0.5, 1.0, 2.5] {
            let d = super::super::yallop::arcv_limit(w) - arcv_curve(w);
            assert!((d - 4.672).abs() < 1e-9);
        }
    }
}
