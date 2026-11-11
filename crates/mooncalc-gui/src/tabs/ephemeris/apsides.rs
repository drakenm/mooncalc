//! Apsides and nodes of a year: lunar perigee/apogee with distance (Meeus
//! ch. 50), node passages (ch. 51) and maximum declinations (ch. 52), each
//! refined against the ephemeris by `mooncalc_core`, from the shared
//! service `mooncalc_app::tables::apsides_nodes`.

use mooncalc_app::dto::{ApiError, UtDateTime};
use mooncalc_app::tables::{YEAR_COVERAGE_MARGIN_DAYS, YearParams};
use mooncalc_core::angle::DEG;
use mooncalc_core::apsides::{Apsis, ApsisKind};
use mooncalc_core::nodes::{DeclinationExtreme, MaxDeclination, NodeKind, NodePassage};
use mooncalc_core::{Ephemeris, Instant};

use super::cached::CachedJob;
use super::format;
use super::year_coverage;
use super::{EphKey, app_year, busy, ephemeris, error_line, set_app_time, time_link, year_picker};
use crate::jobs::JobError;
use crate::tabs::TabContext;
use crate::widgets::coverage::coverage_notice;

/// The events of one year.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct YearEvents {
    pub apsides: Vec<Apsis>,
    pub nodes: Vec<NodePassage>,
    pub declinations: Vec<MaxDeclination>,
}

fn jd_tt(t: &UtDateTime) -> f64 {
    t.to_instant().jd_tt()
}

/// The events of local `year` (`mooncalc_app::tables::apsides_nodes`).
pub fn year_events(eph: &dyn Ephemeris, year: i32, tz: f64) -> Result<YearEvents, ApiError> {
    let y = mooncalc_app::tables::apsides_nodes(
        eph,
        &YearParams {
            year,
            tz_offset_hours: tz,
            ephemeris_id: None,
        },
    )?;
    Ok(YearEvents {
        apsides: y
            .apsides
            .iter()
            .map(|a| Apsis {
                kind: if a.kind == "perigee" {
                    ApsisKind::Perigee
                } else {
                    ApsisKind::Apogee
                },
                jd_tt: jd_tt(&a.time),
                distance_km: a.distance_km,
                parallax: a.parallax_arcsec / 3600.0 * DEG,
            })
            .collect(),
        nodes: y
            .nodes
            .iter()
            .map(|n| NodePassage {
                kind: if n.kind == "ascending" {
                    NodeKind::Ascending
                } else {
                    NodeKind::Descending
                },
                jd_tt: jd_tt(&n.time),
            })
            .collect(),
        declinations: y
            .declinations
            .iter()
            .map(|d| MaxDeclination {
                kind: if d.kind == "north" {
                    DeclinationExtreme::North
                } else {
                    DeclinationExtreme::South
                },
                jd_tt: jd_tt(&d.time),
                dec: d.dec_deg * DEG,
            })
            .collect(),
    })
}

#[derive(Clone, Debug, PartialEq)]
struct Request {
    year: i32,
    tz: f64,
    eph: EphKey,
}

/// Transient state of the apsides/nodes view.
#[derive(Default)]
pub struct ApsidesView {
    job: CachedJob<Request, YearEvents>,
    year: Option<i32>,
}

impl ApsidesView {
    /// The searches bracket events up to half a month beyond the year.
    fn coverage(&self, tc: &TabContext<'_>) -> Option<String> {
        let year = self.year.unwrap_or(app_year(tc.settings));
        year_coverage(tc.settings, year, YEAR_COVERAGE_MARGIN_DAYS)
    }

    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>, visible: bool) {
        if visible && self.coverage(tc).is_none() {
            let req = Request {
                year: self.year.unwrap_or(app_year(tc.settings)),
                tz: tc.settings.time.tz_offset_hours,
                eph: EphKey::of(tc.settings),
            };
            let eph = ephemeris(tc.settings);
            self.job
                .request(ctx, req, "Computing apsides and nodes", move |req, _| {
                    year_events(eph.as_ref(), req.year, req.tz)
                        .map_err(|e| JobError::Failed(e.message))
                });
        }
        self.job.poll(tc.status);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let tz = tc.settings.time.tz_offset_hours;
        ui.horizontal_wrapped(|ui| {
            year_picker(ui, &mut self.year, app_year(tc.settings));
        });
        if let Some(problem) = self.coverage(tc) {
            coverage_notice(ui, &problem);
            return;
        }
        error_line(ui, &self.job.error);
        busy(ui, self.job.is_running());
        let Some(ev) = self.job.get() else {
            return;
        };
        let mut clicked = None;
        let mut link = |ui: &mut egui::Ui, jd_tt: f64| {
            let t = Instant::from_jd_tt(jd_tt);
            if time_link(ui, format::date_time(t, tz)) {
                clicked = Some(t);
            }
        };
        egui::ScrollArea::both().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.heading("Perigee and apogee");
                    egui::Grid::new("apsides_grid")
                        .striped(true)
                        .spacing([12.0, 3.0])
                        .show(ui, |ui| {
                            for h in ["", "Date/time", "Distance (km)", "Diameter"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for a in &ev.apsides {
                                ui.label(match a.kind {
                                    ApsisKind::Perigee => "Perigee",
                                    ApsisKind::Apogee => "Apogee",
                                });
                                link(ui, a.jd_tt);
                                ui.monospace(format!("{:.0}", a.distance_km));
                                let sd =
                                    mooncalc_core::topocentric::moon_semidiameter(a.distance_km);
                                ui.monospace(format!("{:.2}′", 2.0 * sd / DEG * 60.0));
                                ui.end_row();
                            }
                        });
                });
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.heading("Node passages");
                    egui::Grid::new("nodes_grid")
                        .striped(true)
                        .spacing([12.0, 3.0])
                        .show(ui, |ui| {
                            for h in ["", "Date/time"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for n in &ev.nodes {
                                ui.label(match n.kind {
                                    NodeKind::Ascending => "Ascending",
                                    NodeKind::Descending => "Descending",
                                });
                                link(ui, n.jd_tt);
                                ui.end_row();
                            }
                        });
                });
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.heading("Maximum declinations");
                    egui::Grid::new("maxdec_grid")
                        .striped(true)
                        .spacing([12.0, 3.0])
                        .show(ui, |ui| {
                            for h in ["", "Date/time", "Declination"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for d in &ev.declinations {
                                ui.label(match d.kind {
                                    DeclinationExtreme::North => "North",
                                    DeclinationExtreme::South => "South",
                                });
                                link(ui, d.jd_tt);
                                ui.monospace(format::signed_angle(
                                    d.dec,
                                    format::AngleStyle::Decimal,
                                ));
                                ui.end_row();
                            }
                        });
                });
            });
        });
        if let Some(t) = clicked {
            set_app_time(tc, t);
        }
    }
}
