//! The criterion list for the selector.
//!
//! Populated from [`mooncalc_visibility::criteria::all`]. That list is empty
//! until the criteria package lands; the selector copes with that.

/// One entry of the criterion selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CriterionOption {
    /// `Criterion::id`, stored in `Settings::criterion_id`.
    pub id: String,
    /// `Criterion::name`, shown to the user.
    pub name: String,
    /// `Criterion::reference`, shown as a tooltip.
    pub reference: String,
}

/// Every available criterion, in display order.
pub fn criterion_options() -> Vec<CriterionOption> {
    mooncalc_visibility::criteria::all()
        .iter()
        .map(|c| CriterionOption {
            id: c.id().to_owned(),
            name: c.name().to_owned(),
            reference: c.reference().to_owned(),
        })
        .collect()
}

/// The option for `id`, if it exists.
pub fn find<'a>(options: &'a [CriterionOption], id: &str) -> Option<&'a CriterionOption> {
    options.iter().find(|o| o.id == id)
}

/// The id to use: `current` if it is available, otherwise the first option,
/// otherwise `current` unchanged (nothing to choose from yet).
pub fn resolve_id(options: &[CriterionOption], current: &str) -> String {
    match (find(options, current), options.first()) {
        (Some(o), _) | (None, Some(o)) => o.id.clone(),
        (None, None) => current.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(id: &str) -> CriterionOption {
        CriterionOption {
            id: id.into(),
            name: id.to_uppercase(),
            reference: String::new(),
        }
    }

    #[test]
    fn resolve_keeps_valid_ids_and_falls_back() {
        let opts = [opt("yallop"), opt("odeh")];
        assert_eq!(resolve_id(&opts, "odeh"), "odeh");
        assert_eq!(resolve_id(&opts, "gone"), "yallop");
        assert_eq!(resolve_id(&[], "yallop"), "yallop");
    }

    #[test]
    fn listing_criteria_does_not_panic() {
        let opts = criterion_options();
        assert!(opts.iter().all(|o| !o.id.is_empty()));
    }
}
