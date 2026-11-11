//! Apsides and nodes of a year: lunar perigee/apogee with distance (Meeus
//! ch. 50), node passages (ch. 51) and maximum declinations (ch. 52), each
//! refined against the ephemeris by `mooncalc_core`.

use mooncalc_core::Instant;
use mooncalc_core::angle::DEG;
use mooncalc_core::apsides::{Apsis, ApsisKind, apsides_between};
use mooncalc_core::nodes::{
    DeclinationExtreme, MaxDeclination, NodeKind, NodePassage, max_declinations_between,
    node_passages_between,
};

use super::cached::CachedJob;
use super::format;
use super::{
    EphKey, app_year, busy, ephemeris, error_line, set_app_time, time_link, year_bounds,
    year_picker,
};
use crate::tabs::TabContext;

/// The events of one year.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct YearEvents {
    pub apsides: Vec<Apsis>,
    pub nodes: Vec<NodePassage>,
    pub declinations: Vec<MaxDeclination>,
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
    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>, visible: bool) {
        if visible {
            let req = Request {
                year: self.year.unwrap_or(app_year(tc.settings)),
                tz: tc.settings.time.tz_offset_hours,
                eph: EphKey::of(tc.settings),
            };
            let eph = ephemeris(tc.settings);
            self.job
                .request(ctx, req, "Computing apsides and nodes", move |req, job| {
                    let (t0, t1) = year_bounds(req.year, req.tz);
                    let (a, b) = (t0.jd_tt(), t1.jd_tt());
                    let e = Some(eph.as_ref());
                    let apsides = apsides_between(e, a, b);
                    job.set_progress(0.4);
                    job.checkpoint()?;
                    let nodes = node_passages_between(e, a, b);
                    job.set_progress(0.7);
                    job.checkpoint()?;
                    let mut declinations = max_declinations_between(e, a, b);
                    declinations.sort_by(|x, y| x.jd_tt.total_cmp(&y.jd_tt));
                    Ok(YearEvents {
                        apsides,
                        nodes,
                        declinations,
                    })
                });
        }
        self.job.poll(tc.status);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let tz = tc.settings.time.tz_offset_hours;
        ui.horizontal_wrapped(|ui| {
            year_picker(ui, &mut self.year, app_year(tc.settings));
        });
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
