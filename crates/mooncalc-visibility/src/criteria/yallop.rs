//! Yallop's single-parameter q-test.
//!
//! Source: B. D. Yallop, *A Method for Predicting the First Sighting of the
//! New Crescent Moon*, HM Nautical Almanac Office NAO Technical Note No. 69
//! (1997 June, updated 1998 April).
//!
//! * Eq. 6.1: `q = (ARCV − (11.8371 − 6.3226 W′ + 0.7319 W′² − 0.1018 W′³)) / 10`,
//!   the Indian method (eq. 3.6) re-expressed in the **topocentric** crescent
//!   width W′ (arc minutes, eq. 3.10).
//! * ARCV is the **geocentric** airless altitude difference Sun–Moon (§2).
//! * Both are taken at the best time `Tb = Ts + 4/9 Lag` (eq. 4.1).
//! * Zones from Table 5.
//!
//! If the Moon sets before the Sun, or the best time is before conjunction,
//! Yallop's maps show "not visible"; see [`super::crescent_present`].

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo, zone_by_lower_bounds};
use crate::geometry::CrescentGeometry;

/// Lower bounds of q for zones A–E (Table 5); zone F is everything at or
/// below −0.293. Each bound is exclusive: zone A is `q > +0.216`.
pub const Q_BOUNDS: [f64; 5] = [0.216, -0.014, -0.160, -0.232, -0.293];

/// Legend from Yallop (1997) Table 5, with a sequential green-to-red palette
/// (TN 69 does not specify map colours).
pub static ZONES: [ZoneInfo; 6] = [
    ZoneInfo {
        code: "A",
        description: "Easily visible (ARCL ≥ 12°)",
        rgb: [46, 158, 63],
    },
    ZoneInfo {
        code: "B",
        description: "Visible under perfect conditions",
        rgb: [166, 216, 84],
    },
    ZoneInfo {
        code: "C",
        description: "May need optical aid to find the crescent",
        rgb: [255, 217, 47],
    },
    ZoneInfo {
        code: "D",
        description: "Will need optical aid to find the crescent",
        rgb: [252, 141, 89],
    },
    ZoneInfo {
        code: "E",
        description: "Not visible with a telescope (ARCL ≤ 8.5°)",
        rgb: [215, 48, 39],
    },
    ZoneInfo {
        code: "F",
        description: "Not visible, below the Danjon limit (ARCL ≤ 8°)",
        rgb: [150, 150, 150],
    },
];

/// Minimum ARCV for visibility as a function of crescent width W (arc
/// minutes): the Indian method in Yallop's form, eq. 3.6.
pub fn arcv_limit(width_arcmin: f64) -> f64 {
    let w = width_arcmin;
    11.8371 - 6.3226 * w + 0.7319 * w * w - 0.1018 * w * w * w
}

/// Yallop's q (eq. 6.1) from the geocentric ARCV (degrees) and the
/// topocentric crescent width W′ (arc minutes).
pub fn q(arcv_deg: f64, width_topo_arcmin: f64) -> f64 {
    (arcv_deg - arcv_limit(width_topo_arcmin)) / 10.0
}

/// Zone index (0 = A ... 5 = F) for a given q.
pub fn zone_for_q(q: f64) -> usize {
    zone_by_lower_bounds(q, &Q_BOUNDS, false)
}

/// Yallop (1997) q-test. Uses [`CrescentGeometry::arcv_deg`] and
/// [`CrescentGeometry::width_topo_arcmin`] at the best time.
#[derive(Clone, Copy, Debug, Default)]
pub struct Yallop;

impl Criterion for Yallop {
    fn id(&self) -> &'static str {
        "yallop"
    }
    fn name(&self) -> &'static str {
        "Yallop (1997)"
    }
    fn reference(&self) -> &'static str {
        "B. D. Yallop, A Method for Predicting the First Sighting of the New Crescent Moon, \
         HM Nautical Almanac Office, NAO Technical Note No. 69 (1997, updated 1998)"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::YallopBestTime
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        let q = q(g.arcv_deg, g.width_topo_arcmin);
        CriterionResult {
            zone: zone_for_q(q),
            value: Some(q),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows of Yallop (1997) Table 4: (record no., ARCV, W′, published q,
    /// zone). ARCV and W′ are tabulated to 0.1° and 0.01′, so q can only be
    /// reproduced to about ±0.01.
    const TABLE_4: [(u32, f64, f64, f64, usize); 10] = [
        (37, 31.1, 3.6, 3.730, 0),
        (131, 11.8, 0.36, 0.211, 1),
        (36, 9.1, 0.42, -0.018, 2),
        (46, 9.6, 0.29, -0.049, 2),
        (281, 8.9, 0.20, -0.164, 3),
        (15, 8.9, 0.18, -0.189, 3),
        (271, 8.2, 0.18, -0.248, 4),
        (226, 7.9, 0.17, -0.287, 4),
        (7, 5.0, 0.63, -0.316, 5),
        (256, 4.2, 0.07, -0.720, 5),
    ];

    #[test]
    fn reproduces_table_4() {
        for (no, arcv, w, q_pub, zone) in TABLE_4 {
            let g = CrescentGeometry {
                arcv_deg: arcv,
                width_topo_arcmin: w,
                ..Default::default()
            };
            let r = Yallop.evaluate(&g);
            let q = r.value.unwrap();
            assert!(
                (q - q_pub).abs() < 0.01,
                "No. {no}: q = {q}, published {q_pub}"
            );
            assert_eq!(r.zone, zone, "No. {no}: q = {q}");
        }
    }

    /// Yallop §6 derives the A, D and E limits from DAZ = 0 and ARCL = 12°,
    /// 8.5° and 8°, with Bruin's geocentric W = 15′(1 − cos ARCL) (eq. 3.4).
    #[test]
    fn limits_derived_in_section_6() {
        for (arcl, q_pub) in [(12.0_f64, 0.216), (8.5, -0.232), (8.0, -0.293)] {
            let w = 15.0 * (1.0 - arcl.to_radians().cos());
            assert!((q(arcl, w) - q_pub).abs() < 0.001, "ARCL {arcl}");
        }
    }

    #[test]
    fn boundaries_are_exclusive_lower_bounds() {
        assert_eq!(zone_for_q(0.2161), 0);
        assert_eq!(zone_for_q(0.216), 1);
        assert_eq!(zone_for_q(-0.014), 2);
        assert_eq!(zone_for_q(-0.0139), 1);
        assert_eq!(zone_for_q(-0.160), 3);
        assert_eq!(zone_for_q(-0.232), 4);
        assert_eq!(zone_for_q(-0.293), 5);
        assert_eq!(zone_for_q(-0.2929), 4);
        assert_eq!(zone_for_q(-5.0), 5);
    }

    #[test]
    fn uses_topocentric_width_and_geocentric_arcv() {
        let g = CrescentGeometry {
            arcv_deg: 10.0,
            arcv_topo_deg: 0.0,
            width_topo_arcmin: 0.3,
            width_geo_arcmin: 9.0,
            ..Default::default()
        };
        let expected = (10.0 - arcv_limit(0.3)) / 10.0;
        assert!((Yallop.evaluate(&g).value.unwrap() - expected).abs() < 1e-12);
    }
}
