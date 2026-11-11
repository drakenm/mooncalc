//! Crescent sighting criteria.
//!
//! Each criterion is a pure function of [`CrescentGeometry`]. Criteria
//! classify into ordered zones (A = most visible). Maps colour cells by
//! [`ZoneInfo::rgb`], so every criterion supplies its own legend.
//!
//! # Preconditions (handled by the caller)
//!
//! Every criterion here assumes a crescent that is actually there to be
//! looked for: the Moon sets after the Sun (evening; rises before it in the
//! morning) and the evaluation instant is after conjunction (evening; before
//! it in the morning). Yallop (1997, NAO TN 69) and Odeh (2004) both show such
//! places as "not visible" on their maps. The map engine renders them as
//! `None` without calling [`Criterion::evaluate`]; see [`crescent_present`].
//! Criteria still return a sensible zone (normally the least visible one) if
//! called anyway, but they do not check the precondition themselves, except
//! where a rule is about lag or age by definition.
//!
//! # Implemented criteria
//!
//! See [`all`] for the display order. Every module cites its source.

use crate::geometry::CrescentGeometry;

pub mod alrefay;
pub mod babylonian;
pub mod bruin;
pub mod danjon;
pub mod fotheringham;
pub mod indian;
pub mod istanbul;
pub mod mabims;
pub mod maunder;
pub mod odeh;
pub mod saao;
pub mod simple;
pub mod yallop;

pub use alrefay::Alrefay;
pub use babylonian::Babylonian;
pub use bruin::Bruin;
pub use danjon::Danjon;
pub use fotheringham::Fotheringham;
pub use indian::Indian;
pub use istanbul::Istanbul1978;
pub use mabims::Mabims2021;
pub use maunder::Maunder;
pub use odeh::Odeh;
pub use saao::Saao;
pub use simple::{MinimumAge, MinimumLag, MoonsetAfterSunset};
pub use yallop::Yallop;

/// One legend entry of a criterion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneInfo {
    /// Short code shown in tables and legends, e.g. "A".
    pub code: &'static str,
    /// Description, e.g. "Easily visible to the naked eye".
    pub description: &'static str,
    /// Map colour (sRGB).
    pub rgb: [u8; 3],
}

/// The outcome of applying a criterion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CriterionResult {
    /// Index into [`Criterion::zones`].
    pub zone: usize,
    /// The criterion's own figure of merit, if it has one (Yallop q, Odeh V, ...).
    pub value: Option<f64>,
}

/// When a criterion should be evaluated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EvaluationTime {
    /// Yallop's best time: sunset + 4/9 × lag (evening) / sunrise − 4/9 × lag (morning).
    YallopBestTime,
    /// At sunset (or sunrise for the morning crescent).
    Sunset,
    /// At a given solar altitude (degrees, negative = below horizon).
    SunAltitude(f64),
}

/// A crescent sighting criterion.
pub trait Criterion: Send + Sync {
    /// Stable identifier, e.g. "yallop".
    fn id(&self) -> &'static str;
    /// Display name, e.g. "Yallop (1997)".
    fn name(&self) -> &'static str;
    /// Bibliographic reference.
    fn reference(&self) -> &'static str;
    /// When the circumstances should be computed for this criterion.
    fn evaluation_time(&self) -> EvaluationTime;
    /// Legend, most visible first.
    fn zones(&self) -> &'static [ZoneInfo];
    /// Classify the given circumstances.
    fn evaluate(&self, g: &CrescentGeometry) -> CriterionResult;
}

/// Every implemented criterion, in display order: the modern single-parameter
/// criteria (Yallop, Odeh) first, then the other modern ones, the classical
/// ARCV–DAZ curves, the rule-of-thumb criteria and finally the one-quantity
/// rules.
pub fn all() -> Vec<Box<dyn Criterion>> {
    vec![
        Box::new(Yallop),
        Box::new(Odeh),
        Box::new(Saao),
        Box::new(Alrefay),
        Box::new(Bruin),
        Box::new(Indian),
        Box::new(Maunder),
        Box::new(Fotheringham),
        Box::new(Danjon::default()),
        Box::new(Istanbul1978),
        Box::new(Mabims2021),
        Box::new(Babylonian),
        Box::new(MoonsetAfterSunset),
        Box::new(MinimumAge::default()),
        Box::new(MinimumLag::default()),
    ]
}

/// Look a criterion up by its [`Criterion::id`].
pub fn by_id(id: &str) -> Option<Box<dyn Criterion>> {
    all().into_iter().find(|c| c.id() == id)
}

/// Whether there is a crescent to look for at all: the Moon sets after the
/// Sun (lag > 0) and the evaluation is after conjunction (age > 0).
///
/// [`CrescentGeometry::lag_minutes`] and [`CrescentGeometry::age_hours`] are
/// defined so that this holds for both the evening and the morning crescent.
/// Yallop (1997, NAO TN 69) and Odeh (2004) treat places that fail it as "not
/// visible"; the map engine draws them as `None` instead of evaluating a
/// criterion.
pub fn crescent_present(g: &CrescentGeometry) -> bool {
    g.lag_minutes > 0.0 && g.age_hours > 0.0
}

/// Map colours shared by the two-zone "visible / not visible" criteria.
pub(crate) mod palette {
    /// Visible (meets the criterion).
    pub const VISIBLE: [u8; 3] = [46, 158, 63];
    /// Not visible (fails the criterion).
    pub const NOT_VISIBLE: [u8; 3] = [150, 150, 150];
}

/// Zone index for a value against descending lower bounds, where zone `i` is
/// taken when `value > bounds[i]` (or `>=` when `inclusive`), and the last
/// zone (`bounds.len()`) when no bound is met.
pub(crate) fn zone_by_lower_bounds(value: f64, bounds: &[f64], inclusive: bool) -> usize {
    bounds
        .iter()
        .position(|&b| if inclusive { value >= b } else { value > b })
        .unwrap_or(bounds.len())
}

/// Linear interpolation in a table of `(x, y)` rows sorted by increasing `x`.
/// Outside the table the end value is held (no extrapolation).
pub(crate) fn interpolate(table: &[(f64, f64)], x: f64) -> f64 {
    let (first, last) = (table[0], table[table.len() - 1]);
    if x <= first.0 {
        return first.1;
    }
    if x >= last.0 {
        return last.1;
    }
    let i = table.partition_point(|row| row.0 <= x);
    let (x0, y0) = table[i - 1];
    let (x1, y1) = table[i];
    y0 + (y1 - y0) * (x - x0) / (x1 - x0)
}

/// A two-zone result: zone 0 (visible) when `margin >= 0`, else zone 1.
pub(crate) fn two_zone(margin: f64) -> CriterionResult {
    CriterionResult {
        zone: usize::from(margin < 0.0),
        value: Some(margin),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// A geometry with only the given ARCV (geocentric and topocentric) and
    /// DAZ set, and ARCL from Yallop (1997) eq. 2.1.
    pub fn arcv_daz(arcv_deg: f64, daz_deg: f64) -> CrescentGeometry {
        let arcl = (arcv_deg.to_radians().cos() * daz_deg.to_radians().cos())
            .acos()
            .to_degrees();
        CrescentGeometry {
            arcv_deg,
            arcv_topo_deg: arcv_deg,
            daz_deg,
            arcl_deg: arcl,
            arcl_geo_deg: arcl,
            lag_minutes: 40.0,
            age_hours: 20.0,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_lowercase_slugs() {
        let mut seen = HashSet::new();
        for c in all() {
            let id = c.id();
            assert!(!id.is_empty());
            assert!(
                id.chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'),
                "{id} is not a lowercase slug"
            );
            assert!(seen.insert(id), "duplicate id {id}");
            assert!(by_id(id).is_some());
        }
        assert!(by_id("no-such-criterion").is_none());
    }

    #[test]
    fn display_order_starts_with_yallop_then_odeh() {
        let ids: Vec<_> = all().iter().map(|c| c.id()).collect();
        assert_eq!(&ids[..2], &["yallop", "odeh"]);
    }

    #[test]
    fn every_criterion_has_a_legend_and_reference() {
        for c in all() {
            assert!(c.zones().len() >= 2, "{}", c.id());
            assert!(!c.name().is_empty());
            assert!(!c.reference().is_empty());
            let codes: HashSet<_> = c.zones().iter().map(|z| z.code).collect();
            assert_eq!(codes.len(), c.zones().len(), "{}: duplicate codes", c.id());
            // Results always index into the legend, even for degenerate input.
            for g in [
                CrescentGeometry::default(),
                test_support::arcv_daz(30.0, 5.0),
                test_support::arcv_daz(1.0, 40.0),
            ] {
                assert!(c.evaluate(&g).zone < c.zones().len(), "{}", c.id());
            }
        }
    }

    #[test]
    fn crescent_present_needs_positive_lag_and_age() {
        let mut g = test_support::arcv_daz(10.0, 0.0);
        assert!(crescent_present(&g));
        g.lag_minutes = -3.0;
        assert!(!crescent_present(&g));
        g.lag_minutes = 30.0;
        g.age_hours = -1.0;
        assert!(!crescent_present(&g));
    }

    #[test]
    fn helpers() {
        assert_eq!(zone_by_lower_bounds(1.0, &[2.0, 0.0], false), 1);
        assert_eq!(zone_by_lower_bounds(2.0, &[2.0, 0.0], false), 1);
        assert_eq!(zone_by_lower_bounds(2.0, &[2.0, 0.0], true), 0);
        assert_eq!(zone_by_lower_bounds(-1.0, &[2.0, 0.0], true), 2);
        let t = [(0.0, 10.0), (10.0, 0.0), (20.0, 5.0)];
        assert_eq!(interpolate(&t, -5.0), 10.0);
        assert_eq!(interpolate(&t, 5.0), 5.0);
        assert_eq!(interpolate(&t, 10.0), 0.0);
        assert_eq!(interpolate(&t, 15.0), 2.5);
        assert_eq!(interpolate(&t, 99.0), 5.0);
    }
}
