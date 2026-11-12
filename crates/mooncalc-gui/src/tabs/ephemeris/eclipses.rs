//! Eclipses of a range of years, from the shared services
//! `mooncalc_app::tables::eclipse_candidates` and `eclipse_details`
//! (`mooncalc_core::eclipses`: the Meeus ch. 54 search refined against the
//! ephemeris, Besselian elements for solar eclipses, Danjon's shadow for
//! lunar ones), with local circumstances at the app location: contacts,
//! maximum, magnitude and obscuration of solar eclipses, and whether the
//! Moon is above the horizon at each contact of a lunar eclipse. The job
//! refines one eclipse at a time, so it reports progress and can be
//! cancelled.

use mooncalc_app::dto::{Location, UtDateTime};
use mooncalc_app::tables::{
    ECLIPSE_COVERAGE_MARGIN_DAYS, EclipseDto, EclipseKindFilter, EclipsesParams, LunarContactDto,
    SolarContactDto, eclipse_candidates, eclipse_details,
};
use mooncalc_atlas::Zone;
use mooncalc_core::{Ephemeris, Instant};
use serde::{Deserialize, Serialize};

use super::cached::CachedJob;
use super::format;
use super::{EphKey, app_year, busy, ephemeris, error_line, set_app_time, time_link};
use super::{coverage, year_bounds};
use crate::jobs::{JobContext, JobError};
use crate::tabs::TabContext;
use crate::widgets::coverage::coverage_notice;

/// One eclipse with its refined details and local circumstances.
pub type EclipseItem = EclipseDto;

fn instant(t: &UtDateTime) -> Instant {
    t.to_instant()
}

/// The label of a lunar-eclipse contact.
fn contact_label(c: &LunarContactDto) -> &str {
    match c.code.as_str() {
        "P1" => "P1 penumbral begins",
        "U1" => "U1 partial begins",
        "U2" => "U2 total begins",
        "max" => "Greatest eclipse",
        "U3" => "U3 total ends",
        "U4" => "U4 partial ends",
        "P4" => "P4 penumbral ends",
        _ => &c.label,
    }
}

/// Every eclipse of `first..=last` with details and local circumstances
/// at `location`, refined one at a time with progress and cancellation.
pub fn eclipse_items(
    eph: &dyn Ephemeris,
    first: i32,
    last: i32,
    filter: EclipseKindFilter,
    location: Location,
    job: &JobContext,
) -> Result<Vec<EclipseItem>, JobError> {
    let params = EclipsesParams {
        first_year: first,
        last_year: last,
        filter,
        location: Some(location),
        ephemeris_id: None,
        // Times are formatted here, in the app zone.
        tz_offset_hours: 0.0,
        time_zone: None,
    };
    let list = eclipse_candidates(eph, &params).map_err(|e| JobError::Failed(e.message))?;
    let n = list.len().max(1) as f32;
    let mut out = Vec::with_capacity(list.len());
    for (i, e) in list.iter().enumerate() {
        job.set_progress(i as f32 / n);
        job.checkpoint()?;
        out.push(eclipse_details(eph, e, params.location));
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq)]
struct Request {
    first: i32,
    last: i32,
    filter: EclipseKindFilter,
    location: (f64, f64, f64),
    eph: EphKey,
}

/// Which eclipses the view lists (persisted).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EclipseKinds {
    #[default]
    All,
    Solar,
    Lunar,
}

impl EclipseKinds {
    const ALL: [Self; 3] = [Self::All, Self::Solar, Self::Lunar];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Solar => "Solar",
            Self::Lunar => "Lunar",
        }
    }

    fn filter(self) -> EclipseKindFilter {
        match self {
            Self::All => EclipseKindFilter::Any,
            Self::Solar => EclipseKindFilter::Solar,
            Self::Lunar => EclipseKindFilter::Lunar,
        }
    }
}

/// Persisted choices of the eclipses view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EclipseSettings {
    /// Number of years listed, from the first.
    pub years: i32,
    pub kinds: EclipseKinds,
}

impl Default for EclipseSettings {
    fn default() -> Self {
        Self {
            years: 3,
            kinds: EclipseKinds::All,
        }
    }
}

/// Transient state of the eclipses view.
#[derive(Default)]
pub struct EclipsesView {
    job: CachedJob<Request, Vec<EclipseItem>>,
    /// First year; `None` follows the app year.
    first: Option<i32>,
    selected: Option<usize>,
}

impl EclipsesView {
    fn range(&self, tc: &TabContext<'_>) -> (i32, i32) {
        let first = self.first.unwrap_or(app_year(tc.settings));
        let years = tc.settings.ephemeris_tab.eclipses.years.clamp(1, 100);
        (first, first + years - 1)
    }

    /// The searched years, plus the service's margin.
    fn coverage(&self, tc: &TabContext<'_>) -> Option<String> {
        let (first, last) = self.range(tc);
        let tz = tc.settings.time.zone();
        let (start, _) = year_bounds(first, tz);
        let (_, end) = year_bounds(last, tz);
        coverage(
            tc.settings,
            start.jd_ut,
            end.jd_ut,
            ECLIPSE_COVERAGE_MARGIN_DAYS,
            ECLIPSE_COVERAGE_MARGIN_DAYS,
        )
    }

    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>, visible: bool) {
        if visible && self.coverage(tc).is_none() {
            let (first, last) = self.range(tc);
            let l = &tc.settings.location;
            let req = Request {
                first,
                last,
                filter: tc.settings.ephemeris_tab.eclipses.kinds.filter(),
                location: (l.lat_deg, l.lon_deg, l.elevation_m),
                eph: EphKey::of(tc.settings),
            };
            let eph = ephemeris(tc.settings);
            self.job
                .request(ctx, req, "Computing eclipses", move |req, job| {
                    let (lat_deg, lon_deg, elevation_m) = req.location;
                    let location = Location {
                        lat_deg,
                        lon_deg,
                        elevation_m,
                    };
                    eclipse_items(eph.as_ref(), req.first, req.last, req.filter, location, job)
                });
        }
        self.job.poll(tc.status);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let tz = tc.settings.time.zone();
        ui.horizontal_wrapped(|ui| {
            let app = app_year(tc.settings);
            let mut first = self.first.unwrap_or(app);
            ui.label("From:");
            if ui.small_button("◀").clicked() {
                first -= 1;
            }
            ui.add(
                egui::DragValue::new(&mut first)
                    .range(-1999..=3000)
                    .speed(0.2),
            );
            if ui.small_button("▶").clicked() {
                first += 1;
            }
            if first != self.first.unwrap_or(app) {
                self.first = Some(first);
                self.selected = None;
            }
            ui.label("Years:");
            let es = &mut tc.settings.ephemeris_tab.eclipses;
            if ui
                .add(egui::DragValue::new(&mut es.years).range(1..=100))
                .changed()
            {
                self.selected = None;
            }
            if self.first.is_some() && ui.small_button("App year").clicked() {
                self.first = None;
                self.selected = None;
            }
            ui.separator();
            let es = &mut tc.settings.ephemeris_tab.eclipses;
            for k in EclipseKinds::ALL {
                if ui.selectable_value(&mut es.kinds, k, k.label()).changed() {
                    self.selected = None;
                }
            }
        });
        if let Some(problem) = self.coverage(tc) {
            coverage_notice(ui, &problem);
            return;
        }
        error_line(ui, &self.job.error);
        busy(ui, self.job.is_running());
        let Some(items) = self.job.get().cloned() else {
            return;
        };
        if self.selected.is_some_and(|i| i >= items.len()) {
            self.selected = None;
        }
        let mut clicked = None;
        let location = tc.settings.location.name.clone();

        egui::Panel::right("eclipse_details")
            .resizable(true)
            .default_size(380.0)
            .min_size(280.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match self.selected {
                    Some(i) => details_ui(ui, &items[i], tz, &location, &mut clicked),
                    None => {
                        ui.weak("Select an eclipse for details and local circumstances.");
                    }
                });
            });

        egui::ScrollArea::both().show(ui, |ui| {
            egui::Grid::new("eclipse_list")
                .striped(true)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    for h in [
                        "Greatest eclipse",
                        "Type",
                        "γ",
                        "Magnitude",
                        "Duration",
                        "Here",
                    ] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for (i, item) in items.iter().enumerate() {
                        let t = instant(&item.greatest);
                        if time_link(ui, format::date_time(t, tz)) {
                            clicked = Some(t);
                            self.selected = Some(i);
                        }
                        if ui
                            .selectable_label(self.selected == Some(i), &item.label)
                            .clicked()
                        {
                            self.selected = Some(i);
                        }
                        ui.monospace(format!("{:+.4}", item.gamma));
                        let (mag, dur, here) = summary(item);
                        ui.monospace(mag);
                        ui.monospace(dur);
                        ui.label(here);
                        ui.end_row();
                    }
                });
            if items.is_empty() {
                ui.weak("No eclipses in this range.");
            }
        });
        if let Some(t) = clicked {
            set_app_time(tc, t);
        }
    }
}

/// Magnitude, duration and local-visibility summary of a list row.
fn summary(item: &EclipseItem) -> (String, String, String) {
    if let Some(solar) = &item.solar {
        let here = match &solar.local {
            Some(l) if l.max.sun_alt_deg > 0.0 => format!("{} {:.3}", l.kind, l.magnitude),
            Some(_) => "Sun below horizon at maximum".into(),
            None => "not visible".into(),
        };
        return (format!("{:.4}", item.magnitude), "—".into(), here);
    }
    let Some(d) = &item.lunar else {
        return (format!("{:.4}", item.magnitude), "—".into(), String::new());
    };
    let mag = if item.kind == "penumbral" {
        format!("{:.4} (pen.)", d.penumbral_magnitude)
    } else {
        format!("{:.4}", d.umbral_magnitude)
    };
    let dur = d
        .total_duration_min
        .map(|m| format!("total {}", format::duration_min(m)))
        .or_else(|| {
            d.partial_duration_min
                .map(|m| format!("partial {}", format::duration_min(m)))
        })
        .or_else(|| {
            d.penumbral_duration_min
                .map(|m| format!("pen. {}", format::duration_min(m)))
        })
        .unwrap_or_else(|| "—".into());
    let up = d
        .contacts
        .iter()
        .filter(|c| c.above_horizon == Some(true))
        .count();
    let here = if up == d.contacts.len() {
        "Moon up throughout".into()
    } else if up == 0 {
        "Moon below horizon".into()
    } else {
        format!("Moon up at {up} of {} contacts", d.contacts.len())
    };
    (mag, dur, here)
}

fn details_ui(
    ui: &mut egui::Ui,
    item: &EclipseItem,
    tz: Zone,
    location: &str,
    clicked: &mut Option<Instant>,
) {
    let mut link = |ui: &mut egui::Ui, t: &UtDateTime| {
        let t = instant(t);
        if time_link(ui, format::date_time_seconds(t, tz)) {
            *clicked = Some(t);
        }
    };
    ui.heading(&item.label);
    if let Some(d) = &item.solar {
        egui::Grid::new("solar_details")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Greatest eclipse");
                link(ui, &item.greatest);
                ui.end_row();
                ui.label("γ");
                ui.monospace(format!("{:+.4}", item.gamma));
                ui.end_row();
                ui.label("Magnitude");
                ui.monospace(format!("{:.4}", item.magnitude));
                ui.end_row();
                ui.label("Central");
                ui.label(if d.central { "yes" } else { "no" });
                ui.end_row();
                ui.label("Point of greatest eclipse");
                ui.label(crate::location::format_lat_lon(
                    d.greatest_lat_deg,
                    d.greatest_lon_deg,
                ));
                ui.end_row();
                ui.label("Sun altitude there");
                ui.monospace(format!("{:.1}°", d.sun_alt_deg));
                ui.end_row();
            });
        ui.add_space(8.0);
        ui.strong(format!("Local circumstances — {location}"));
        match &d.local {
            None => {
                ui.label("No phase of this eclipse reaches the observer.");
            }
            Some(l) => {
                egui::Grid::new("solar_local")
                    .striped(true)
                    .num_columns(4)
                    .show(ui, |ui| {
                        for h in ["Contact", "Time", "Sun alt", "Sun az"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        let rows: [(&str, Option<&SolarContactDto>); 5] = [
                            ("C1 partial begins", Some(&l.c1)),
                            ("C2 central begins", l.c2.as_ref()),
                            ("Maximum", Some(&l.max)),
                            ("C3 central ends", l.c3.as_ref()),
                            ("C4 partial ends", Some(&l.c4)),
                        ];
                        for (label, c) in rows {
                            let Some(c) = c else { continue };
                            ui.label(label);
                            link(ui, &c.time);
                            let alt = c.sun_alt_deg;
                            let text = format!("{alt:+.1}°");
                            if alt < 0.0 {
                                ui.colored_label(ui.visuals().warn_fg_color, text)
                                    .on_hover_text("Sun below the horizon");
                            } else {
                                ui.monospace(text);
                            }
                            ui.monospace(format!("{:.1}°", c.sun_az_deg));
                            ui.end_row();
                        }
                    });
                egui::Grid::new("solar_local_values")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Local type");
                        ui.label(&l.kind);
                        ui.end_row();
                        ui.label("Magnitude");
                        ui.monospace(format!("{:.4}", l.magnitude));
                        ui.end_row();
                        ui.label("Obscuration");
                        ui.monospace(format!("{:.2} %", l.obscuration * 100.0));
                        ui.end_row();
                        ui.label("Moon/Sun diameter ratio");
                        ui.monospace(format!("{:.4}", l.moon_sun_ratio));
                        ui.end_row();
                        if let Some(m) = l.central_duration_min {
                            ui.label(format!("Duration of {}", l.kind));
                            ui.monospace(format::duration_min(m));
                            ui.end_row();
                        }
                        ui.label("Duration (C1–C4)");
                        ui.monospace(format::duration_min(l.duration_min));
                        ui.end_row();
                    });
                if l.max.sun_alt_deg <= 0.0 {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "The Sun is below the horizon at maximum.",
                    );
                }
            }
        }
    } else if let Some(d) = &item.lunar {
        egui::Grid::new("lunar_details")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Greatest eclipse");
                link(ui, &item.greatest);
                ui.end_row();
                ui.label("γ");
                ui.monospace(format!("{:+.4}", item.gamma));
                ui.end_row();
                ui.label("Umbral magnitude");
                ui.monospace(format!("{:.4}", d.umbral_magnitude));
                ui.end_row();
                ui.label("Penumbral magnitude");
                ui.monospace(format!("{:.4}", d.penumbral_magnitude));
                ui.end_row();
                for (label, m) in [
                    ("Penumbral duration", d.penumbral_duration_min),
                    ("Partial duration", d.partial_duration_min),
                    ("Total duration", d.total_duration_min),
                ] {
                    if let Some(m) = m {
                        ui.label(label);
                        ui.monospace(format::duration_min(m));
                        ui.end_row();
                    }
                }
                ui.label("Moon in zenith at");
                ui.label(crate::location::format_lat_lon(
                    d.zenith_lat_deg,
                    d.zenith_lon_deg,
                ));
                ui.end_row();
            });
        ui.add_space(8.0);
        ui.strong(format!("Contacts — {location}"));
        egui::Grid::new("lunar_contacts")
            .striped(true)
            .num_columns(4)
            .show(ui, |ui| {
                for h in ["Contact", "Time", "Moon alt", "Visible"] {
                    ui.strong(h);
                }
                ui.end_row();
                for c in &d.contacts {
                    ui.label(contact_label(c));
                    link(ui, &c.time);
                    ui.monospace(
                        c.moon_alt_deg
                            .map_or_else(|| "—".into(), |a| format!("{a:+.1}°")),
                    );
                    if c.above_horizon == Some(true) {
                        ui.label("above horizon");
                    } else {
                        ui.colored_label(ui.visuals().warn_fg_color, "below horizon");
                    }
                    ui.end_row();
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::MeeusEphemeris;

    #[test]
    fn eclipses_of_2024_at_dallas() {
        // 2024: total solar (Apr 8), annular solar (Oct 2), penumbral lunar
        // (Mar 25), partial lunar (Sep 18).
        let eph = MeeusEphemeris::new();
        let dallas = Location {
            lat_deg: 32.78,
            lon_deg: -96.80,
            elevation_m: 140.0,
        };
        let items = eclipse_items(
            &eph,
            2024,
            2024,
            EclipseKindFilter::Any,
            dallas,
            &JobContext::detached(),
        )
        .unwrap();
        let kinds: Vec<&str> = items.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "Lunar penumbral",
                "Solar total",
                "Lunar partial",
                "Solar annular"
            ]
        );
        // Dallas is in the path of totality on 2024 April 8: maximum at
        // about 18:42:40 UT (NASA: totality 18:40:44–18:44:39), a quarter
        // of an hour after local noon, so the Sun is near its meridian
        // altitude 90° − 32.8° + 7.5° ≈ 64.7°.
        let l = items[1].solar.as_ref().unwrap().local.as_ref().unwrap();
        assert_eq!(l.kind, "total");
        assert!(l.obscuration > 0.999);
        let max = instant(&l.max.time);
        let nasa = Instant::from_calendar(2024, 4, 8, 18, 42, 40.0);
        assert!((max.jd_ut - nasa.jd_ut).abs() * 86_400.0 < 60.0);
        let totality_min = l.central_duration_min.unwrap();
        assert!((totality_min - 3.92).abs() < 0.2, "{totality_min}");
        let alt = l.max.sun_alt_deg;
        assert!((63.5..64.7).contains(&alt), "{alt}");
        assert!(summary(&items[1]).2.starts_with("total 1.0"));
        // The penumbral lunar eclipse of 2024 March 25 (greatest ≈ 07:13 UT)
        // is visible from Dallas (Moon high in the night sky).
        let contacts = &items[0].lunar.as_ref().unwrap().contacts;
        let greatest = contacts.iter().find(|c| c.code == "max").unwrap();
        assert_eq!(contact_label(greatest), "Greatest eclipse");
        assert_eq!(greatest.above_horizon, Some(true));
        assert!(greatest.moon_alt_deg.unwrap() > 30.0);
        assert_eq!(contacts.len(), 3); // P1, greatest, P4
        assert_eq!(summary(&items[0]).2, "Moon up throughout");
    }
}
