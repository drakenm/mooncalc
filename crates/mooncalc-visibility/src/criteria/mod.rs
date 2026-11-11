//! Crescent sighting criteria.
//!
//! Each criterion is a pure function of [`CrescentGeometry`]. Criteria
//! classify into ordered zones (A = most visible). Maps colour cells by
//! [`ZoneInfo::rgb`], so every criterion supplies its own legend.

use crate::geometry::CrescentGeometry;

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

/// Every implemented criterion, in display order.
pub fn all() -> Vec<Box<dyn Criterion>> {
    Vec::new()
}
