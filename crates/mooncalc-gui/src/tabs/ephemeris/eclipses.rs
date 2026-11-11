//! Eclipses of a range of years (`mooncalc_core::eclipses`): the Meeus
//! ch. 54 search refined against the ephemeris (Besselian elements for
//! solar eclipses, Danjon's shadow for lunar ones), with local
//! circumstances at the app location: contacts, maximum, magnitude and
//! obscuration of solar eclipses (`local_solar_eclipse`), and whether the
//! Moon is above the horizon at each contact of a lunar eclipse.

use mooncalc_core::angle::DEG;
use mooncalc_core::eclipses::{
    Eclipse, EclipseContact, EclipseFilter, LocalEclipseKind, LocalSolarEclipse,
    LunarEclipseDetails, LunarEclipseKind, SolarEclipseDetails, SolarEclipseKind,
    eclipses_in_years, local_solar_eclipse, refine_lunar_eclipse, refine_solar_eclipse,
};
use mooncalc_core::events::{Body, DirectSource, Horizon, Observer};
use mooncalc_core::{Ephemeris, GeoLocation, Instant};
use serde::{Deserialize, Serialize};

use super::cached::CachedJob;
use super::format;
use super::{EphKey, app_year, busy, ephemeris, error_line, set_app_time, time_link};
use super::{LUNATION_MARGIN, coverage, year_bounds};
use crate::tabs::TabContext;
use crate::widgets::coverage::coverage_notice;

/// The Moon at one contact of a lunar eclipse, seen from the observer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoonAtContact {
    pub label: &'static str,
    pub time: Instant,
    /// Topocentric airless altitude of the Moon's centre, radians.
    pub alt: f64,
    /// Upper limb above the horizon (34′ refraction).
    pub above: bool,
}

/// One eclipse with its refined details and local circumstances.
#[derive(Clone, Debug, PartialEq)]
pub enum EclipseItem {
    Solar {
        details: SolarEclipseDetails,
        local: Option<LocalSolarEclipse>,
    },
    Lunar {
        details: LunarEclipseDetails,
        contacts: Vec<MoonAtContact>,
    },
}

impl EclipseItem {
    pub fn greatest(&self) -> Instant {
        Instant::from_jd_tt(match self {
            Self::Solar { details, .. } => details.jd_tt,
            Self::Lunar { details, .. } => details.jd_tt,
        })
    }

    pub fn kind_label(&self) -> String {
        match self {
            Self::Solar { details, .. } => format!("Solar {}", solar_kind(details.kind)),
            Self::Lunar { details, .. } => format!("Lunar {}", lunar_kind(details.kind)),
        }
    }

    pub fn gamma(&self) -> f64 {
        match self {
            Self::Solar { details, .. } => details.gamma,
            Self::Lunar { details, .. } => details.gamma,
        }
    }
}

fn solar_kind(k: SolarEclipseKind) -> &'static str {
    match k {
        SolarEclipseKind::Partial => "partial",
        SolarEclipseKind::Annular => "annular",
        SolarEclipseKind::Total => "total",
        SolarEclipseKind::Hybrid => "hybrid",
    }
}

fn lunar_kind(k: LunarEclipseKind) -> &'static str {
    match k {
        LunarEclipseKind::Penumbral => "penumbral",
        LunarEclipseKind::Partial => "partial",
        LunarEclipseKind::Total => "total",
    }
}

fn local_kind(k: LocalEclipseKind) -> &'static str {
    match k {
        LocalEclipseKind::Partial => "partial",
        LocalEclipseKind::Annular => "annular",
        LocalEclipseKind::Total => "total",
    }
}

/// The Moon at each contact of a lunar eclipse, for an observer.
pub fn lunar_contacts(
    eph: &dyn Ephemeris,
    d: &LunarEclipseDetails,
    loc: &GeoLocation,
) -> Vec<MoonAtContact> {
    let src = DirectSource::new(eph);
    let obs = Observer::new(loc);
    [
        ("P1 penumbral begins", d.p1),
        ("U1 partial begins", d.u1),
        ("U2 total begins", d.u2),
        ("Greatest eclipse", Some(d.jd_tt)),
        ("U3 total ends", d.u3),
        ("U4 partial ends", d.u4),
        ("P4 penumbral ends", d.p4),
    ]
    .into_iter()
    .filter_map(|(label, jd)| {
        let time = Instant::from_jd_tt(jd?);
        let look = obs.look(&src, Body::Moon, time.jd_ut);
        let target = Horizon::STANDARD.target(Body::Moon, look.distance_km, loc.elevation_m);
        Some(MoonAtContact {
            label,
            time,
            alt: look.alt,
            above: look.alt > target,
        })
    })
    .collect()
}

/// Every eclipse of `first..=last` with details and local circumstances.
pub fn eclipse_items(
    eph: &dyn Ephemeris,
    first: i32,
    last: i32,
    filter: EclipseFilter,
    loc: &GeoLocation,
    mut progress: impl FnMut(f32) -> bool,
) -> Option<Vec<EclipseItem>> {
    let list = eclipses_in_years(first, last, filter);
    let n = list.len().max(1) as f32;
    let mut out = Vec::with_capacity(list.len());
    for (i, e) in list.iter().enumerate() {
        if !progress(i as f32 / n) {
            return None;
        }
        out.push(match e {
            Eclipse::Solar(s) => {
                let details = refine_solar_eclipse(eph, s);
                EclipseItem::Solar {
                    local: local_solar_eclipse(eph, details.jd_tt, loc),
                    details,
                }
            }
            Eclipse::Lunar(l) => {
                let details = refine_lunar_eclipse(eph, l);
                EclipseItem::Lunar {
                    contacts: lunar_contacts(eph, &details, loc),
                    details,
                }
            }
        });
    }
    Some(out)
}

#[derive(Clone, Debug, PartialEq)]
struct Request {
    first: i32,
    last: i32,
    filter: EclipseFilter,
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

    fn filter(self) -> EclipseFilter {
        match self {
            Self::All => EclipseFilter::Any,
            Self::Solar => EclipseFilter::Solar,
            Self::Lunar => EclipseFilter::Lunar,
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

    /// The searched years, plus the lunations around them.
    fn coverage(&self, tc: &TabContext<'_>) -> Option<String> {
        let (first, last) = self.range(tc);
        let tz = tc.settings.time.tz_offset_hours;
        let (start, _) = year_bounds(first, tz);
        let (_, end) = year_bounds(last, tz);
        coverage(
            tc.settings,
            start.jd_ut,
            end.jd_ut,
            LUNATION_MARGIN,
            LUNATION_MARGIN,
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
                    let loc = GeoLocation::new(req.location.0, req.location.1, req.location.2);
                    eclipse_items(eph.as_ref(), req.first, req.last, req.filter, &loc, |f| {
                        job.set_progress(f);
                        !job.is_cancelled()
                    })
                    .ok_or(crate::jobs::JobError::Cancelled)
                });
        }
        self.job.poll(tc.status);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let tz = tc.settings.time.tz_offset_hours;
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
                        let t = item.greatest();
                        if time_link(ui, format::date_time(t, tz)) {
                            clicked = Some(t);
                            self.selected = Some(i);
                        }
                        if ui
                            .selectable_label(self.selected == Some(i), item.kind_label())
                            .clicked()
                        {
                            self.selected = Some(i);
                        }
                        ui.monospace(format!("{:+.4}", item.gamma()));
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
    match item {
        EclipseItem::Solar { details, local } => {
            let here = match local {
                Some(l) if l.max.sun.alt > 0.0 => {
                    format!("{} {:.3}", local_kind(l.kind), l.magnitude)
                }
                Some(_) => "Sun below horizon at maximum".into(),
                None => "not visible".into(),
            };
            (format!("{:.4}", details.magnitude), "—".into(), here)
        }
        EclipseItem::Lunar { details, contacts } => {
            let mag = if details.kind == LunarEclipseKind::Penumbral {
                format!("{:.4} (pen.)", details.penumbral_magnitude)
            } else {
                format!("{:.4}", details.umbral_magnitude)
            };
            let dur = details
                .total_duration_min()
                .map(|m| format!("total {}", format::duration_min(m)))
                .or_else(|| {
                    details
                        .partial_duration_min()
                        .map(|m| format!("partial {}", format::duration_min(m)))
                })
                .or_else(|| {
                    details
                        .penumbral_duration_min()
                        .map(|m| format!("pen. {}", format::duration_min(m)))
                })
                .unwrap_or_else(|| "—".into());
            let up = contacts.iter().filter(|c| c.above).count();
            let here = if up == contacts.len() {
                "Moon up throughout".into()
            } else if up == 0 {
                "Moon below horizon".into()
            } else {
                format!("Moon up at {up} of {} contacts", contacts.len())
            };
            (mag, dur, here)
        }
    }
}

fn details_ui(
    ui: &mut egui::Ui,
    item: &EclipseItem,
    tz: f64,
    location: &str,
    clicked: &mut Option<Instant>,
) {
    let mut link = |ui: &mut egui::Ui, t: Instant| {
        if time_link(ui, format::date_time_seconds(t, tz)) {
            *clicked = Some(t);
        }
    };
    ui.heading(item.kind_label());
    match item {
        EclipseItem::Solar { details: d, local } => {
            egui::Grid::new("solar_details")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Greatest eclipse");
                    link(ui, Instant::from_jd_tt(d.jd_tt));
                    ui.end_row();
                    ui.label("γ");
                    ui.monospace(format!("{:+.4}", d.gamma));
                    ui.end_row();
                    ui.label("Magnitude");
                    ui.monospace(format!("{:.4}", d.magnitude));
                    ui.end_row();
                    ui.label("Central");
                    ui.label(if d.central { "yes" } else { "no" });
                    ui.end_row();
                    ui.label("Point of greatest eclipse");
                    ui.label(crate::location::format_lat_lon(d.lat_deg, d.lon_deg));
                    ui.end_row();
                    ui.label("Sun altitude there");
                    ui.monospace(format!("{:.1}°", d.sun_alt / DEG));
                    ui.end_row();
                });
            ui.add_space(8.0);
            ui.strong(format!("Local circumstances — {location}"));
            match local {
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
                            let rows: [(&str, Option<&EclipseContact>); 5] = [
                                ("C1 partial begins", Some(&l.c1)),
                                ("C2 central begins", l.c2.as_ref()),
                                ("Maximum", Some(&l.max)),
                                ("C3 central ends", l.c3.as_ref()),
                                ("C4 partial ends", Some(&l.c4)),
                            ];
                            for (label, c) in rows {
                                let Some(c) = c else { continue };
                                ui.label(label);
                                link(ui, Instant::from_jd_tt(c.jd_tt));
                                let alt = c.sun.alt / DEG;
                                let text = format!("{alt:+.1}°");
                                if alt < 0.0 {
                                    ui.colored_label(ui.visuals().warn_fg_color, text)
                                        .on_hover_text("Sun below the horizon");
                                } else {
                                    ui.monospace(text);
                                }
                                ui.monospace(format!("{:.1}°", c.sun.az / DEG));
                                ui.end_row();
                            }
                        });
                    egui::Grid::new("solar_local_values")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("Local type");
                            ui.label(local_kind(l.kind));
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
                            if let (Some(c2), Some(c3)) = (l.c2, l.c3) {
                                ui.label(format!("Duration of {}", local_kind(l.kind)));
                                ui.monospace(format::duration_min((c3.jd_tt - c2.jd_tt) * 1440.0));
                                ui.end_row();
                            }
                            ui.label("Duration (C1–C4)");
                            ui.monospace(format::duration_min((l.c4.jd_tt - l.c1.jd_tt) * 1440.0));
                            ui.end_row();
                        });
                    if l.max.sun.alt <= 0.0 {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            "The Sun is below the horizon at maximum.",
                        );
                    }
                }
            }
        }
        EclipseItem::Lunar {
            details: d,
            contacts,
        } => {
            egui::Grid::new("lunar_details")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Greatest eclipse");
                    link(ui, Instant::from_jd_tt(d.jd_tt));
                    ui.end_row();
                    ui.label("γ");
                    ui.monospace(format!("{:+.4}", d.gamma));
                    ui.end_row();
                    ui.label("Umbral magnitude");
                    ui.monospace(format!("{:.4}", d.umbral_magnitude));
                    ui.end_row();
                    ui.label("Penumbral magnitude");
                    ui.monospace(format!("{:.4}", d.penumbral_magnitude));
                    ui.end_row();
                    for (label, m) in [
                        ("Penumbral duration", d.penumbral_duration_min()),
                        ("Partial duration", d.partial_duration_min()),
                        ("Total duration", d.total_duration_min()),
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
                    for c in contacts {
                        ui.label(c.label);
                        link(ui, c.time);
                        ui.monospace(format!("{:+.1}°", c.alt / DEG));
                        if c.above {
                            ui.label("above horizon");
                        } else {
                            ui.colored_label(ui.visuals().warn_fg_color, "below horizon");
                        }
                        ui.end_row();
                    }
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::MeeusEphemeris;

    #[test]
    fn eclipses_of_2024_at_dallas() {
        // 2024: annular-free list — total solar (Apr 8), annular solar
        // (Oct 2), penumbral lunar (Mar 25), partial lunar (Sep 18).
        let eph = MeeusEphemeris::new();
        let dallas = GeoLocation::new(32.78, -96.80, 140.0);
        let items = eclipse_items(&eph, 2024, 2024, EclipseFilter::Any, &dallas, |_| true).unwrap();
        let kinds: Vec<String> = items.iter().map(EclipseItem::kind_label).collect();
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
        let EclipseItem::Solar { local: Some(l), .. } = &items[1] else {
            panic!("no local circumstances")
        };
        assert_eq!(l.kind, LocalEclipseKind::Total);
        assert!(l.obscuration > 0.999);
        let max = Instant::from_jd_tt(l.max.jd_tt);
        let nasa = Instant::from_calendar(2024, 4, 8, 18, 42, 40.0);
        assert!((max.jd_ut - nasa.jd_ut).abs() * 86_400.0 < 60.0);
        let (c2, c3) = (l.c2.unwrap(), l.c3.unwrap());
        let totality_min = (c3.jd_tt - c2.jd_tt) * 1440.0;
        assert!((totality_min - 3.92).abs() < 0.2, "{totality_min}");
        let alt = l.max.sun.alt / DEG;
        assert!((63.5..64.7).contains(&alt), "{alt}");
        // The penumbral lunar eclipse of 2024 March 25 (greatest ≈ 07:13 UT)
        // is visible from Dallas (Moon high in the night sky).
        let EclipseItem::Lunar { contacts, .. } = &items[0] else {
            panic!()
        };
        let greatest = contacts
            .iter()
            .find(|c| c.label == "Greatest eclipse")
            .unwrap();
        assert!(greatest.above && greatest.alt > 30.0 * DEG);
        assert_eq!(contacts.len(), 3); // P1, greatest, P4
    }
}
