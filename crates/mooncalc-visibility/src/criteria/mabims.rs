//! The MABIMS 2021 ("3 – 6.4") criterion.
//!
//! Source: the criterion adopted in 2021 by the religious-affairs ministers
//! of Brunei, Indonesia, Malaysia and Singapore (MABIMS), replacing the 1995
//! "2-3-8" rule: at sunset the Moon's altitude is at least 3° and its
//! elongation at least 6.4°. Thresholds as tabulated by Mohd Nawawi et al.,
//! *Malaysian J. Sci.* 44(1), 101–120 (2025), Table 1. The 6.4° elongation is
//! Odeh's (2004) Danjon limit.
//!
//! As applied by the member states, the altitude is the topocentric altitude
//! of the Moon's centre and the elongation is geocentric. This module uses
//! [`CrescentGeometry::moon_alt_deg`] (apparent topocentric) and
//! [`CrescentGeometry::arcl_geo_deg`], at sunset.

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo, palette};
use crate::geometry::CrescentGeometry;

/// Minimum Moon altitude at sunset, degrees (inclusive).
pub const MIN_ALTITUDE_DEG: f64 = 3.0;
/// Minimum elongation at sunset, degrees (inclusive).
pub const MIN_ELONGATION_DEG: f64 = 6.4;

/// Legend.
pub static ZONES: [ZoneInfo; 2] = [
    ZoneInfo {
        code: "V",
        description: "Visible: altitude ≥ 3° and elongation ≥ 6.4° at sunset",
        rgb: palette::VISIBLE,
    },
    ZoneInfo {
        code: "I",
        description: "Not visible",
        rgb: palette::NOT_VISIBLE,
    },
];

/// MABIMS 2021. The value is the smaller of the two margins.
#[derive(Clone, Copy, Debug, Default)]
pub struct Mabims2021;

impl Criterion for Mabims2021 {
    fn id(&self) -> &'static str {
        "mabims-2021"
    }
    fn name(&self) -> &'static str {
        "MABIMS (2021)"
    }
    fn reference(&self) -> &'static str {
        "MABIMS criterion (2021): altitude >= 3 deg, elongation >= 6.4 deg at sunset; \
         as listed by Mohd Nawawi et al., Malaysian Journal of Science 44(1), 101-120 (2025)"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::Sunset
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        let margin = (g.moon_alt_deg - MIN_ALTITUDE_DEG).min(g.arcl_geo_deg - MIN_ELONGATION_DEG);
        super::two_zone(margin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(alt: f64, elong_geo: f64) -> CrescentGeometry {
        CrescentGeometry {
            moon_alt_deg: alt,
            arcl_geo_deg: elong_geo,
            arcl_deg: elong_geo - 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn pass_fail_boundary() {
        assert_eq!(Mabims2021.evaluate(&at(6.0, 10.0)).zone, 0);
        assert_eq!(Mabims2021.evaluate(&at(1.0, 4.0)).zone, 1);
        assert_eq!(Mabims2021.evaluate(&at(2.9, 10.0)).zone, 1);
        assert_eq!(Mabims2021.evaluate(&at(6.0, 6.3)).zone, 1);
        assert_eq!(Mabims2021.evaluate(&at(3.0, 6.4)).zone, 0);
    }
}
