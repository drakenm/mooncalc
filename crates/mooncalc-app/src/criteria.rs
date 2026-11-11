//! The criteria registry and its legends.

use std::sync::OnceLock;

use mooncalc_maps::engine::field_thresholds;
use mooncalc_visibility::Criterion;
use mooncalc_visibility::criteria::EvaluationTime;

use crate::dto::{ApiError, CriterionInfo};

/// Every criterion, in display order (`mooncalc_visibility::criteria::all`),
/// built once and shared, so services can hold `&'static dyn Criterion`.
pub fn all() -> &'static [Box<dyn Criterion>] {
    static REGISTRY: OnceLock<Vec<Box<dyn Criterion>>> = OnceLock::new();
    REGISTRY.get_or_init(mooncalc_visibility::criteria::all)
}

/// Look a criterion up by id.
pub fn get(id: &str) -> Result<&'static dyn Criterion, ApiError> {
    all()
        .iter()
        .find(|c| c.id() == id)
        .map(|c| &**c)
        .ok_or_else(|| ApiError::unknown_criterion(id))
}

/// Human-readable evaluation-time rule.
pub fn evaluation_time_text(t: EvaluationTime) -> String {
    match t {
        EvaluationTime::YallopBestTime => "Best time (sunset + 4/9 lag)".into(),
        EvaluationTime::Sunset => "Sunset".into(),
        EvaluationTime::SunAltitude(a) => format!("Sun at {a:+}° altitude"),
    }
}

/// The wire description of one criterion.
pub fn info(c: &dyn Criterion) -> CriterionInfo {
    CriterionInfo {
        id: c.id().into(),
        name: c.name().into(),
        reference: c.reference().into(),
        evaluation_time: evaluation_time_text(c.evaluation_time()),
        thresholds: field_thresholds(c),
        zones: c.zones().iter().map(Into::into).collect(),
    }
}

/// Every criterion with its legend, in display order.
pub fn list() -> Vec<CriterionInfo> {
    all().iter().map(|c| info(&**c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_the_visibility_crate() {
        let l = list();
        assert_eq!(l.len(), mooncalc_visibility::criteria::all().len());
        assert_eq!(l[0].id, "yallop");
        assert_eq!(l[0].zones.len(), 6);
        assert_eq!(l[0].thresholds.as_ref().unwrap().len(), 5);
        assert!(
            l.iter()
                .all(|c| !c.reference.is_empty() && c.zones.len() >= 2)
        );
        assert_eq!(get("odeh").unwrap().id(), "odeh");
        assert_eq!(get("nope").err().unwrap().code, "unknown-criterion");
        // The evening-sunset criteria describe their own rule.
        assert!(l.iter().any(|c| c.evaluation_time == "Sunset"));
    }
}
