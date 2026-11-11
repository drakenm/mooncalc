//! The SAAO criterion of Caldwell & Laney.
//!
//! Source: J. A. R. Caldwell and C. D. Laney, "First Visibility of the Lunar
//! Crescent", *African Skies* No. 5, 15–23 (2001); table "Numerical values of
//! the criterion lines in Figs. 1-4", columns `x = daz` and `y = dalt`.
//! Reproduced at five-degree steps by Odeh (2004, Exp. Astron. 18, 39) Table IV.
//!
//! The quantities are taken **at sunset** (sunrise for the old Moon):
//!
//! * DALT: apparent altitude of the Moon's **lower limb**, with topocentric
//!   parallax and refraction, degrees.
//! * DAZ: |Moon azimuth − Sun azimuth|, degrees.
//!
//! Above the upper curve a naked-eye sighting is possible under ideal
//! conditions. Below it, naked-eye sighting is improbable, though optical aid
//! may succeed, increasingly unlikely towards the lower curve. Below the
//! lower curve, sighting is impossible even with optical aid.
//!
//! The published table covers 0° ≤ DAZ ≤ 21°. Beyond 21° this module holds the
//! 21° limits. The curves still fall there, so this errs on the side of "not
//! visible". The authors note that large-DAZ data are scarce.

use super::{Criterion, CriterionResult, EvaluationTime, ZoneInfo, interpolate};
use crate::geometry::CrescentGeometry;

/// Caldwell & Laney (2001): `(DAZ, upper DALT, lower DALT)` in degrees.
// The published 6.28 is a coincidence, not an approximation of τ.
#[allow(clippy::approx_constant)]
pub const TABLE: [(f64, f64, f64); 43] = [
    (0.00, 8.19, 6.29),
    (0.50, 8.18, 6.28),
    (1.00, 8.16, 6.26),
    (1.50, 8.14, 6.24),
    (2.00, 8.10, 6.20),
    (2.50, 8.06, 6.16),
    (3.00, 8.02, 6.12),
    (3.50, 7.96, 6.06),
    (4.00, 7.91, 6.01),
    (4.50, 7.84, 5.94),
    (5.00, 7.77, 5.87),
    (5.50, 7.70, 5.80),
    (6.00, 7.62, 5.72),
    (6.50, 7.53, 5.63),
    (7.00, 7.44, 5.54),
    (7.50, 7.35, 5.45),
    (8.00, 7.26, 5.36),
    (8.50, 7.16, 5.26),
    (9.00, 7.05, 5.15),
    (9.50, 6.95, 5.05),
    (10.00, 6.84, 4.94),
    (10.50, 6.73, 4.83),
    (11.00, 6.61, 4.71),
    (11.50, 6.50, 4.60),
    (12.00, 6.38, 4.48),
    (12.50, 6.26, 4.36),
    (13.00, 6.15, 4.25),
    (13.50, 6.03, 4.13),
    (14.00, 5.91, 4.01),
    (14.50, 5.79, 3.89),
    (15.00, 5.67, 3.77),
    (15.50, 5.55, 3.65),
    (16.00, 5.43, 3.53),
    (16.50, 5.31, 3.41),
    (17.00, 5.19, 3.29),
    (17.50, 5.08, 3.18),
    (18.00, 4.96, 3.06),
    (18.50, 4.85, 2.95),
    (19.00, 4.74, 2.84),
    (19.50, 4.64, 2.74),
    (20.00, 4.53, 2.63),
    (20.50, 4.43, 2.53),
    (21.00, 4.33, 2.43),
];

/// Legend after Caldwell & Laney (2001), "Summary of Criterion Lines".
pub static ZONES: [ZoneInfo; 3] = [
    ZoneInfo {
        code: "A",
        description: "Naked-eye sighting possible in ideal conditions",
        rgb: [46, 158, 63],
    },
    ZoneInfo {
        code: "B",
        description: "Naked eye improbable; optical aid may succeed",
        rgb: [255, 217, 47],
    },
    ZoneInfo {
        code: "C",
        description: "Not visible even with optical aid",
        rgb: [150, 150, 150],
    },
];

fn column(i: usize) -> [(f64, f64); 43] {
    TABLE.map(|row| (row.0, if i == 0 { row.1 } else { row.2 }))
}

/// Upper (naked-eye) DALT limit, degrees, at a given |DAZ|.
pub fn upper_limit(daz_deg: f64) -> f64 {
    interpolate(&column(0), daz_deg.abs())
}

/// Lower (optical-aid) DALT limit, degrees, at a given |DAZ|.
pub fn lower_limit(daz_deg: f64) -> f64 {
    interpolate(&column(1), daz_deg.abs())
}

/// Apparent altitude of the Moon's lower limb, degrees.
///
/// Approximated as the apparent altitude of the centre
/// ([`CrescentGeometry::moon_alt_deg`]) minus the topocentric semi-diameter.
/// The refraction difference across one semi-diameter is a few hundredths of
/// a degree at the altitudes that matter, well inside the table's precision.
pub fn dalt(g: &CrescentGeometry) -> f64 {
    g.moon_alt_deg - g.semi_diameter_topo_arcmin / 60.0
}

/// SAAO criterion (Caldwell & Laney 2001), DALT vs DAZ at sunset. The
/// reported value is DALT minus the upper (naked-eye) limit, in degrees.
#[derive(Clone, Copy, Debug, Default)]
pub struct Saao;

impl Criterion for Saao {
    fn id(&self) -> &'static str {
        "saao"
    }
    fn name(&self) -> &'static str {
        "SAAO (Caldwell & Laney 2001)"
    }
    fn reference(&self) -> &'static str {
        "J. A. R. Caldwell and C. D. Laney, First Visibility of the Lunar Crescent, \
         African Skies No. 5, 15-23 (2001)"
    }
    fn evaluation_time(&self) -> EvaluationTime {
        EvaluationTime::Sunset
    }
    fn zones(&self) -> &'static [ZoneInfo] {
        &ZONES
    }
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult {
        let dalt = dalt(g);
        let zone = if dalt >= upper_limit(g.daz_deg) {
            0
        } else if dalt >= lower_limit(g.daz_deg) {
            1
        } else {
            2
        };
        CriterionResult {
            zone,
            value: Some(dalt - upper_limit(g.daz_deg)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(dalt: f64, daz: f64) -> CrescentGeometry {
        CrescentGeometry {
            moon_alt_deg: dalt,
            daz_deg: daz,
            ..Default::default()
        }
    }

    /// Odeh (2004) Table IV quotes the same curves at 5° steps (to 0.1°).
    #[test]
    fn matches_odeh_table_iv() {
        let odeh = [
            (0.0, 6.3, 8.2),
            (5.0, 5.9, 7.8),
            (10.0, 4.9, 6.8),
            (15.0, 3.8, 5.7),
            (20.0, 2.6, 4.5),
        ];
        for (daz, lo, hi) in odeh {
            assert!((lower_limit(daz) - lo).abs() <= 0.05, "DAZ {daz}");
            assert!((upper_limit(daz) - hi).abs() <= 0.05, "DAZ {daz}");
        }
    }

    #[test]
    fn table_is_sorted_and_curves_ordered() {
        for w in TABLE.windows(2) {
            assert!(w[1].0 > w[0].0);
        }
        for (_, hi, lo) in TABLE {
            assert!(hi > lo);
        }
    }

    #[test]
    fn dalt_is_lower_limb() {
        let g = CrescentGeometry {
            moon_alt_deg: 8.0,
            semi_diameter_topo_arcmin: 15.0,
            ..Default::default()
        };
        assert!((dalt(&g) - 7.75).abs() < 1e-12);
    }

    #[test]
    fn interpolates_between_rows() {
        assert!((upper_limit(10.25) - (6.84 + 6.73) / 2.0).abs() < 1e-12);
        assert!((upper_limit(-10.25) - upper_limit(10.25)).abs() < 1e-12);
        assert_eq!(upper_limit(30.0), 4.33);
    }

    #[test]
    fn classification() {
        // DAZ = 5°: limits 7.77 / 5.87.
        assert_eq!(Saao.evaluate(&geom(9.0, 5.0)).zone, 0);
        assert_eq!(Saao.evaluate(&geom(6.5, 5.0)).zone, 1);
        assert_eq!(Saao.evaluate(&geom(4.0, 5.0)).zone, 2);
        // Boundaries are inclusive.
        assert_eq!(Saao.evaluate(&geom(7.77, 5.0)).zone, 0);
        assert_eq!(Saao.evaluate(&geom(7.76, 5.0)).zone, 1);
        assert_eq!(Saao.evaluate(&geom(5.87, 5.0)).zone, 1);
        assert_eq!(Saao.evaluate(&geom(5.86, 5.0)).zone, 2);
        let v = Saao.evaluate(&geom(8.77, 5.0)).value.unwrap();
        assert!((v - 1.0).abs() < 1e-9);
    }
}
