//! Lunar phases of a year: new Moon, first quarter, full Moon and last
//! quarter for each lunation, with the Moon's age at each phase and the
//! length of each lunation (`mooncalc_core::events::phases`: Meeus ch. 49
//! refined against the ephemeris).

use mooncalc_atlas::Zone;
use std::collections::BTreeMap;

use mooncalc_core::events::Phase;
use mooncalc_core::events::phases::{
    SYNODIC_MONTH_DAYS, meeus_phase_jde, moon_age_days, phase_near_tt, phases_between,
};
use mooncalc_core::{Ephemeris, Instant};

use super::cached::CachedJob;
use super::format;
use super::{
    EphKey, app_year, busy, ephemeris, error_line, set_app_time, time_link, year_bounds,
    year_picker,
};
use super::{LUNATION_MARGIN, coverage, year_coverage};
use crate::tabs::TabContext;
use crate::widgets::coverage::coverage_notice;

/// Epoch of Meeus' lunation numbers (k = 0: new Moon of 2000 January 6).
const LUNATION_EPOCH_JDE: f64 = 2_451_550.097_66;

/// One lunation's phases that fall in the year.
#[derive(Clone, Debug, PartialEq)]
pub struct Lunation {
    /// Meeus/NASA lunation number k.
    pub k: i64,
    /// The new Moon starting the lunation (may precede the year).
    pub new_moon: Instant,
    /// Length of the lunation (to the next new Moon), days.
    pub length_days: f64,
    /// New, first quarter, full, last quarter, when inside the year.
    pub phases: [Option<Instant>; 4],
}

impl Lunation {
    /// The Moon's age at phase `i`, days.
    pub fn age(&self, i: usize) -> Option<f64> {
        self.phases[i].map(|t| t.jd_ut - self.new_moon.jd_ut)
    }
}

/// The lunations with at least one phase in `[t0, t1)`.
pub fn lunations(eph: &dyn Ephemeris, t0: Instant, t1: Instant) -> Vec<Lunation> {
    let mut by_k: BTreeMap<i64, [Option<Instant>; 4]> = BTreeMap::new();
    for (phase, t) in phases_between(eph, t0, t1) {
        let i = Phase::ALL.iter().position(|p| *p == phase).unwrap_or(0);
        let k = ((t.jd_tt() - LUNATION_EPOCH_JDE) / SYNODIC_MONTH_DAYS - phase.fraction()).round()
            as i64;
        by_k.entry(k).or_default()[i] = Some(t);
    }
    let new_moon = |k: i64| phase_near_tt(eph, Phase::New, meeus_phase_jde(k as f64));
    by_k.into_iter()
        .map(|(k, phases)| {
            let start = new_moon(k);
            Lunation {
                k,
                new_moon: Instant::from_jd_tt(start),
                length_days: new_moon(k + 1) - start,
                phases,
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
struct Request {
    year: i32,
    tz: Zone,
    eph: EphKey,
}

/// Transient state of the phases view.
#[derive(Default)]
pub struct PhasesView {
    job: CachedJob<Request, Vec<Lunation>>,
    year: Option<i32>,
}

impl PhasesView {
    /// The year's lunations need the new Moons before and after it.
    fn coverage(&self, tc: &TabContext<'_>) -> Option<String> {
        let year = self.year.unwrap_or(app_year(tc.settings));
        year_coverage(tc.settings, year, LUNATION_MARGIN)
    }

    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>, visible: bool) {
        if visible && self.coverage(tc).is_none() {
            let req = Request {
                year: self.year.unwrap_or(app_year(tc.settings)),
                tz: tc.settings.time.zone(),
                eph: EphKey::of(tc.settings),
            };
            let eph = ephemeris(tc.settings);
            self.job
                .request(ctx, req, "Computing lunar phases", move |req, _| {
                    let (t0, t1) = year_bounds(req.year, req.tz);
                    Ok(lunations(eph.as_ref(), t0, t1))
                });
        }
        self.job.poll(tc.status);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let tz = tc.settings.time.zone();
        let eph = ephemeris(tc.settings);
        ui.horizontal_wrapped(|ui| {
            year_picker(ui, &mut self.year, app_year(tc.settings));
            ui.separator();
            let now = tc.settings.instant();
            // The age needs the previous new Moon.
            if coverage(tc.settings, now.jd_ut, now.jd_ut, LUNATION_MARGIN, 1.0).is_none() {
                ui.label(format!(
                    "Moon's age at the app time: {:.2} days",
                    moon_age_days(eph.as_ref(), now)
                ));
            }
        });
        ui.weak(
            "Age: days since the new Moon that starts the lunation. k: Meeus/NASA lunation number.",
        );
        if let Some(problem) = self.coverage(tc) {
            coverage_notice(ui, &problem);
            return;
        }
        error_line(ui, &self.job.error);
        busy(ui, self.job.is_running());
        let Some(rows) = self.job.get() else {
            return;
        };
        let mut clicked = None;
        egui::ScrollArea::both().show(ui, |ui| {
            egui::Grid::new("phases_grid")
                .striped(true)
                .spacing([18.0, 4.0])
                .show(ui, |ui| {
                    for h in [
                        "k",
                        "New Moon",
                        "First quarter",
                        "Age",
                        "Full Moon",
                        "Age",
                        "Last quarter",
                        "Age",
                        "Lunation length",
                    ] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for l in rows {
                        ui.monospace(l.k.to_string());
                        for i in 0..4 {
                            match l.phases[i] {
                                Some(t) => {
                                    if time_link(ui, format::date_time(t, tz)) {
                                        clicked = Some(t);
                                    }
                                }
                                None => {
                                    ui.weak("—");
                                }
                            }
                            if i > 0 {
                                match l.age(i) {
                                    Some(a) => ui.monospace(format!("{a:.2} d")),
                                    None => ui.weak(""),
                                };
                            }
                        }
                        let h = l.length_days * 24.0;
                        ui.monospace(format!(
                            "{}d {:02}h {:02}m",
                            (h / 24.0).floor(),
                            (h % 24.0).floor(),
                            ((h * 60.0) % 60.0).floor()
                        ));
                        ui.end_row();
                    }
                });
        });
        if let Some(t) = clicked {
            set_app_time(tc, t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::MeeusEphemeris;

    #[test]
    fn phases_of_2024() {
        let eph = MeeusEphemeris::new();
        let (t0, t1) = year_bounds(2024, Zone::UTC);
        let ls = lunations(&eph, t0, t1);
        let count: usize = ls.iter().map(|l| l.phases.iter().flatten().count()).sum();
        // 2024 has 13 new Moons (the last on 30 December), 12 first
        // quarters, 12 full Moons and 13 last quarters (USNO).
        assert_eq!(ls.iter().filter(|l| l.phases[0].is_some()).count(), 13);
        assert_eq!(count, 50);
        // The new Moon of 2024 April 8 (the total solar eclipse) is
        // lunation k = 300 (k = 0 at the new Moon of 2000 January 6).
        let april = ls.iter().find(|l| l.k == 300).unwrap();
        let c = april.new_moon.to_calendar();
        assert_eq!((c.year, c.month, c.day, c.hour), (2024, 4, 8, 18));
        // Full Moon about 14.8 days after new; ages increase.
        let full = april.age(2).unwrap();
        assert!((13.5..16.0).contains(&full));
        assert!(april.age(1).unwrap() < full && full < april.age(3).unwrap());
        assert!((april.length_days - SYNODIC_MONTH_DAYS).abs() < 0.6);
    }
}
