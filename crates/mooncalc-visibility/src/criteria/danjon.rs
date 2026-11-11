//! The Danjon limit.
//!
//! Source: A. Danjon, *L'Astronomie* 46, 57 (1932), and *L'Astronomie* 50 (1936). Below
//! an apparent Sun–Moon elongation of about 7° the crescent's arc length
//! shrinks to zero and the Moon cannot be seen at all. Later values:
//!
//! * 7.5°: M. Fatoohi, F. R. Stephenson and S. S. Al-Dargazelli, "The Danjon
//!   limit of first visibility of the lunar crescent", *The Observatory* 118,
//!   65 (1998);
//! * 6.4°: M. Sh. Odeh, *Exp. Astron.* 18, 39 (2004), §8, the smallest
//!   optically aided sighting in his 737-record database.
//!
//! The limit applies to the apparent (topocentric) elongation; Yallop (1997,
//! NAO TN 69, §6 E) equates an apparent 7° with a geocentric ARCL of 8°. This
//! criterion uses [`CrescentGeometry::arcl_deg`] at sunset. The elongation
//! changes by only about 0.5° per hour, so the evaluation time matters little.

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo, palette, two_zone};
use crate::geometry::CrescentGeometry;

/// Danjon's own limit (1932, 1936), degrees of apparent elongation.
pub const DANJON_LIMIT_DEG: f64 = 7.0;
/// Fatoohi, Stephenson & Al-Dargazelli (1998) revision, degrees.
pub const FATOOHI_LIMIT_DEG: f64 = 7.5;
/// Odeh (2004) value from the ICOP database, degrees.
pub const ODEH_LIMIT_DEG: f64 = 6.4;

/// Legend.
pub static ZONES: [ZoneInfo; 2] = [
    ZoneInfo {
        code: "V",
        description: "Elongation at or above the Danjon limit: may be visible",
        rgb: palette::VISIBLE,
    },
    ZoneInfo {
        code: "I",
        description: "Elongation below the Danjon limit: not visible",
        rgb: palette::NOT_VISIBLE,
    },
];

/// The Danjon limit test with a configurable threshold (default
/// [`DANJON_LIMIT_DEG`]). The value is elongation minus the limit, degrees.
#[derive(Clone, Copy, Debug)]
pub struct Danjon {
    /// Minimum apparent elongation, degrees.
    pub limit_deg: f64,
}

impl Default for Danjon {
    fn default() -> Self {
        Self {
            limit_deg: DANJON_LIMIT_DEG,
        }
    }
}

impl Criterion for Danjon {
    fn id(&self) -> &'static str {
        "danjon"
    }
    fn name(&self) -> &'static str {
        "Danjon limit"
    }
    fn reference(&self) -> &'static str {
        "A. Danjon, L'Astronomie 46, 57 (1932) and 50 (1936); see also Fatoohi et al., \
         The Observatory 118, 65 (1998) and Odeh, Experimental Astronomy 18, 39 (2004)"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::Sunset
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        two_zone(g.arcl_deg - self.limit_deg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elong(arcl_deg: f64) -> CrescentGeometry {
        CrescentGeometry {
            arcl_deg,
            arcl_geo_deg: arcl_deg + 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn pass_fail_boundary() {
        let d = Danjon::default();
        assert_eq!(d.evaluate(&elong(10.0)).zone, 0);
        assert_eq!(d.evaluate(&elong(5.0)).zone, 1);
        assert_eq!(d.evaluate(&elong(7.0)).zone, 0);
        assert_eq!(d.evaluate(&elong(6.99)).zone, 1);
    }

    #[test]
    fn configurable_limit() {
        let odeh = Danjon {
            limit_deg: ODEH_LIMIT_DEG,
        };
        let fatoohi = Danjon {
            limit_deg: FATOOHI_LIMIT_DEG,
        };
        assert_eq!(odeh.evaluate(&elong(6.8)).zone, 0);
        assert_eq!(fatoohi.evaluate(&elong(6.8)).zone, 1);
    }
}
