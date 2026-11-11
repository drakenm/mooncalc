//! Seasons: equinoxes and solstices of a year (Meeus ch. 27 refined
//! against the ephemeris by `mooncalc_core::seasons`) and the equation of
//! time through the year (Meeus eq. 28.1).

use mooncalc_core::Instant;
use mooncalc_core::seasons::{equation_of_time, seasons_of_year};

use super::cached::CachedJob;
use super::chart::{self, Chart, Marker};
use super::format;
use super::{DAY_MARGIN, coverage, year_coverage};
use super::{
    EphKey, app_year, busy, ephemeris, error_line, set_app_time, time_link, year_bounds,
    year_picker,
};
use crate::tabs::TabContext;
use crate::widgets::coverage::coverage_notice;

const SEASON_NAMES: [&str; 4] = [
    "March equinox",
    "June solstice",
    "September equinox",
    "December solstice",
];

/// Minutes of time per radian of hour angle.
const MIN_PER_RAD: f64 = 720.0 / std::f64::consts::PI;

/// The seasons and equation of time of one year.
#[derive(Clone, Debug, PartialEq)]
pub struct YearSeasons {
    pub year: i32,
    /// Equinoxes and solstices, in order.
    pub seasons: [Instant; 4],
    /// Equation of time at each season, minutes.
    pub eot_at_seasons: [f64; 4],
    /// Equation of time at 12h UT of each day, (instant, minutes).
    pub daily_eot: Vec<(Instant, f64)>,
}

/// Compute the seasons and the daily equation of time of `year`.
pub fn year_seasons(eph: &dyn mooncalc_core::Ephemeris, year: i32, tz: f64) -> YearSeasons {
    let jd = seasons_of_year(eph, year);
    let seasons = jd.map(Instant::from_jd_tt);
    let eot = |t: Instant| equation_of_time(eph, t.jd_tt()) * MIN_PER_RAD;
    let (t0, t1) = year_bounds(year, tz);
    let days = (t1.jd_ut - t0.jd_ut).round() as usize;
    let daily_eot = (0..days)
        .map(|d| {
            let t = t0.add_days(d as f64 + 0.5);
            (t, eot(t))
        })
        .collect();
    YearSeasons {
        year,
        seasons,
        eot_at_seasons: seasons.map(eot),
        daily_eot,
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Request {
    year: i32,
    tz: f64,
    eph: EphKey,
}

/// Transient state of the seasons view.
#[derive(Default)]
pub struct SeasonsView {
    job: CachedJob<Request, YearSeasons>,
    year: Option<i32>,
}

impl SeasonsView {
    fn coverage(&self, tc: &TabContext<'_>) -> Option<String> {
        let year = self.year.unwrap_or(app_year(tc.settings));
        year_coverage(tc.settings, year, DAY_MARGIN)
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
                .request(ctx, req, "Computing seasons", move |req, _| {
                    Ok(year_seasons(eph.as_ref(), req.year, req.tz))
                });
        }
        self.job.poll(tc.status);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let tz = tc.settings.time.tz_offset_hours;
        let eph = ephemeris(tc.settings);
        ui.horizontal_wrapped(|ui| {
            year_picker(ui, &mut self.year, app_year(tc.settings));
            ui.separator();
            let now = tc.settings.instant();
            if coverage(tc.settings, now.jd_ut, now.jd_ut, 1.0, 1.0).is_none() {
                ui.label(format!(
                    "Equation of time at the app time: {}",
                    format::signed_minutes(
                        equation_of_time(eph.as_ref(), now.jd_tt()) * MIN_PER_RAD
                    )
                ))
                .on_hover_text("Apparent minus mean solar time");
            }
        });
        if let Some(problem) = self.coverage(tc) {
            coverage_notice(ui, &problem);
            return;
        }
        error_line(ui, &self.job.error);
        busy(ui, self.job.is_running());
        let Some(ys) = self.job.get() else {
            return;
        };
        let mut clicked = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("seasons_grid")
                .striped(true)
                .spacing([18.0, 4.0])
                .show(ui, |ui| {
                    for h in ["Event", "Date/time", "Equation of time"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for ((name, t), eot) in
                        SEASON_NAMES.iter().zip(ys.seasons).zip(ys.eot_at_seasons)
                    {
                        ui.label(*name);
                        if time_link(ui, format::date_time(t, tz)) {
                            clicked = Some(t);
                        }
                        ui.monospace(format::signed_minutes(eot));
                        ui.end_row();
                    }
                });
            ui.add_space(12.0);
            ui.strong(format!("Equation of time, {}", ys.year));
            let (lo, hi) = ys
                .daily_eot
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), (_, e)| {
                    (a.min(*e), b.max(*e))
                });
            if let (Some(min), Some(max)) = (
                ys.daily_eot.iter().min_by(|a, b| a.1.total_cmp(&b.1)),
                ys.daily_eot.iter().max_by(|a, b| a.1.total_cmp(&b.1)),
            ) {
                ui.weak(format!(
                    "Minimum {} on {}, maximum {} on {}",
                    format::signed_minutes(min.1),
                    format::date(min.0, tz),
                    format::signed_minutes(max.1),
                    format::date(max.0, tz)
                ));
            }
            let points: Vec<[f64; 2]> = ys
                .daily_eot
                .iter()
                .enumerate()
                .map(|(d, (_, e))| [d as f64, *e])
                .collect();
            let month_starts: Vec<(f64, String)> = (1..=12u32)
                .map(|m| {
                    let t = Instant::from_calendar(ys.year, m, 1, 0, 0, 0.0);
                    let d = t.jd_ut - ys.daily_eot[0].0.jd_ut + 0.5;
                    (
                        d,
                        ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"]
                            [m as usize - 1]
                            .to_string(),
                    )
                })
                .collect();
            let y_ticks: Vec<(f64, String)> = chart::nice_ticks(lo.floor(), hi.ceil(), 8)
                .into_iter()
                .map(|v| (v, format!("{v:+.0}")))
                .collect();
            let mut markers = Vec::new();
            let now = tc.settings.instant().jd_ut - ys.daily_eot[0].0.jd_ut + 0.5;
            if (0.0..points.len() as f64).contains(&now) {
                markers.push(Marker {
                    x: now,
                    y: equation_of_time(eph.as_ref(), tc.settings.instant().jd_tt()) * MIN_PER_RAD,
                    color: egui::Color32::from_rgb(230, 160, 40),
                    label: "app time".into(),
                });
            }
            let width = ui.available_width().clamp(320.0, 900.0);
            chart::show(
                ui,
                egui::vec2(width, 260.0),
                &Chart {
                    x_range: (0.0, points.len() as f64),
                    y_range: (lo.floor() - 1.0, hi.ceil() + 1.0),
                    x_ticks: &month_starts,
                    y_ticks: &y_ticks,
                    x_label: "Month",
                    y_label: "E (min)",
                    points: &points,
                    markers: &markers,
                    equal_aspect: false,
                },
            );
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
    fn equation_of_time_extremes_2024() {
        let eph = MeeusEphemeris::new();
        let ys = year_seasons(&eph, 2024, 0.0);
        assert_eq!(ys.daily_eot.len(), 366);
        let c = ys.seasons[0].to_calendar();
        assert_eq!((c.month, c.day), (3, 20));
        // E reaches about −14.2 min in mid-February and +16.4 min in early
        // November (Astronomical Almanac).
        let min = ys
            .daily_eot
            .iter()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        let max = ys
            .daily_eot
            .iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        assert!((min.1 + 14.2).abs() < 0.2 && min.0.to_calendar().month == 2);
        assert!((max.1 - 16.4).abs() < 0.2 && max.0.to_calendar().month == 11);
    }
}
