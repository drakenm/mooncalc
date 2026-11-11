//! **Report** tab: the observing circumstances of the evening and morning
//! crescent for the selected observer and local date, and every criterion's
//! verdict.
//!
//! [`ReportTab::poll`] recomputes in a background job whenever the observer,
//! the local date, the time zone or the ephemeris changes
//! ([`compute_report`]). Each sighting uses one interpolation table and set
//! of conjunctions, evaluated once per distinct evaluation time (Yallop's
//! best time, sunset, a Sun altitude), via
//! `mooncalc_visibility::circumstances::compute_with`. The day is anchored at
//! **civil** noon in the user's time zone, so "the evening of 10 March" is
//! the one a local observer means.

use std::fmt::Write as _;

use mooncalc_core::events::{ApparentTable, CalendarDate};
use mooncalc_core::time::Instant;
use mooncalc_core::{Ephemeris, GeoLocation};
use mooncalc_visibility::circumstances::{
    Circumstances, Outcome, Polar, Request, Settings as CircSettings, Sighting, compute_with,
    new_moons_around,
};
use mooncalc_visibility::criteria::{EvaluationTime, all};
use mooncalc_visibility::{CriterionResult, ZoneInfo};

use super::TabContext;
use super::map::swatch;
use crate::backend::{self, ActiveEphemeris};
use crate::format::{fmt_hours, fmt_local_ut, fmt_minutes};
use crate::jobs::{JobContext, JobError, JobSlot};
use crate::location::format_lat_lon;
use crate::settings::Settings;

/// What a report depends on.
#[derive(Clone, Debug, PartialEq)]
pub struct ReportKey {
    pub date: CalendarDate,
    pub tz_offset_hours: f64,
    pub name: String,
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub elevation_m: f64,
    pub ephemeris: String,
}

impl ReportKey {
    pub fn from_settings(s: &Settings, active: &ActiveEphemeris) -> Self {
        Self {
            date: CalendarDate::from_jd(s.time.jd_local()),
            tz_offset_hours: s.time.tz_offset_hours,
            name: s.location.name.clone(),
            lat_deg: s.location.lat_deg,
            lon_deg: s.location.lon_deg,
            elevation_m: s.location.elevation_m,
            ephemeris: active.eph.name().to_owned(),
        }
    }

    /// Civil noon of the local date, as UT.
    pub fn civil_noon(&self) -> Instant {
        Instant::from_jd_ut(self.date.jd0() + 0.5 - self.tz_offset_hours / 24.0)
    }

    /// When the world map for this date shows another evening at the
    /// observer's longitude (its date line is the 180° meridian): the note
    /// naming the map date that shows this report's evening.
    pub fn map_date_note(&self) -> Option<mooncalc_app::dto::MapDateNote> {
        mooncalc_app::dateline::report_note(self.date, self.lon_deg, self.tz_offset_hours)
    }
}

/// One criterion's verdict.
#[derive(Clone, Debug)]
pub struct CriterionRow {
    pub id: &'static str,
    pub name: &'static str,
    pub evaluation: EvaluationTime,
    pub zones: &'static [ZoneInfo],
    pub outcome: Outcome,
    pub result: Option<CriterionResult>,
}

impl CriterionRow {
    pub fn zone(&self) -> Option<&'static ZoneInfo> {
        self.result.and_then(|r| self.zones.get(r.zone))
    }
}

/// The evening or the morning.
#[derive(Clone, Debug)]
pub struct SightingReport {
    pub sighting: Sighting,
    /// At Yallop's best time.
    pub best: Outcome,
    /// At sunset (sunrise): shown when the best time is undefined.
    pub at_sun_event: Outcome,
    /// The new Moon nearest the day.
    pub conjunction: Instant,
    pub rows: Vec<CriterionRow>,
}

impl SightingReport {
    /// The circumstances to tabulate: the best time's, else the Sun event's.
    pub fn circumstances(&self) -> Option<&Circumstances> {
        self.best
            .circumstances()
            .or_else(|| self.at_sun_event.circumstances())
    }
}

/// A complete report.
#[derive(Clone, Debug)]
pub struct ReportData {
    pub key: ReportKey,
    pub evening: SightingReport,
    pub morning: SightingReport,
}

/// Compute the report for `key`.
pub fn compute_report(
    key: ReportKey,
    eph: &dyn Ephemeris,
    job: &JobContext,
) -> Result<ReportData, JobError> {
    let noon = key.civil_noon();
    let location = GeoLocation::new(key.lat_deg, key.lon_deg, key.elevation_m);
    let table = ApparentTable::with_default_step(eph, noon.add_days(-2.0), noon.add_days(2.0));
    let new_moons = new_moons_around(eph, noon);
    let conjunction = *new_moons
        .iter()
        .min_by(|a, b| {
            (a.jd_ut - noon.jd_ut)
                .abs()
                .total_cmp(&(b.jd_ut - noon.jd_ut).abs())
        })
        .unwrap_or(&noon);
    let criteria = all();
    let sighting_report = |sighting: Sighting| -> Result<SightingReport, JobError> {
        let mut cache: Vec<(EvaluationTime, Outcome)> = Vec::new();
        let mut outcome = |evaluation: EvaluationTime| -> Outcome {
            if let Some((_, o)) = cache.iter().find(|(e, _)| *e == evaluation) {
                return *o;
            }
            let req = Request {
                location,
                noon,
                sighting,
                evaluation,
            };
            let o = compute_with(&table, &new_moons, &req, &CircSettings::default());
            cache.push((evaluation, o));
            o
        };
        let best = outcome(EvaluationTime::YallopBestTime);
        let at_sun_event = outcome(EvaluationTime::Sunset);
        let mut rows = Vec::with_capacity(criteria.len());
        for c in &criteria {
            job.checkpoint()?;
            let o = outcome(c.evaluation_time());
            rows.push(CriterionRow {
                id: c.id(),
                name: c.name(),
                evaluation: c.evaluation_time(),
                zones: c.zones(),
                outcome: o,
                result: o.evaluate(c.as_ref()),
            });
        }
        Ok(SightingReport {
            sighting,
            best,
            at_sun_event,
            conjunction,
            rows,
        })
    };
    let evening = sighting_report(Sighting::Evening)?;
    let morning = sighting_report(Sighting::Morning)?;
    Ok(ReportData {
        key,
        evening,
        morning,
    })
}

/// How serious an outcome is, for colouring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Warn,
    Bad,
}

fn sun_event(s: Sighting) -> &'static str {
    match s {
        Sighting::Evening => "sunset",
        Sighting::Morning => "sunrise",
    }
}

/// A sentence describing an outcome.
pub fn outcome_text(o: &Outcome, s: Sighting) -> (String, Tone) {
    let evening = s == Sighting::Evening;
    match o {
        Outcome::Crescent(c) => (
            if evening {
                format!(
                    "The crescent can be looked for after sunset: the Moon sets {} after the Sun, {} after the new Moon.",
                    fmt_minutes(c.geometry.lag_minutes),
                    fmt_hours(c.geometry.age_hours)
                )
            } else {
                format!(
                    "The old crescent can be looked for before sunrise: the Moon rises {} before the Sun, {} before the new Moon.",
                    fmt_minutes(c.geometry.lag_minutes),
                    fmt_hours(c.geometry.age_hours)
                )
            },
            Tone::Good,
        ),
        Outcome::BeforeConjunction(c) => (
            if evening {
                format!(
                    "No crescent: the Moon sets after the Sun, but the new Moon has not happened yet at the best time ({} to go).",
                    fmt_hours(-c.geometry.age_hours)
                )
            } else {
                format!(
                    "No old crescent: the Moon rises before the Sun, but the new Moon has already happened ({} ago).",
                    fmt_hours(-c.geometry.age_hours)
                )
            },
            Tone::Bad,
        ),
        Outcome::MoonSetsFirst(c) => (
            if evening {
                format!(
                    "No crescent: the Moon sets before the Sun (lag {}).",
                    fmt_minutes(c.geometry.lag_minutes)
                )
            } else {
                format!(
                    "No old crescent: the Moon rises after the Sun (lag {}).",
                    fmt_minutes(c.geometry.lag_minutes)
                )
            },
            Tone::Bad,
        ),
        Outcome::MoonAlwaysUp { .. } => (
            format!(
                "The Moon does not {} within 12 hours of {}; Yallop's best time is undefined (criteria evaluated at {} still apply).",
                if evening { "set" } else { "rise" },
                sun_event(s),
                sun_event(s)
            ),
            Tone::Warn,
        ),
        Outcome::MoonAlwaysDown { .. } => (
            "No crescent: the Moon stays below the horizon all day.".into(),
            Tone::Bad,
        ),
        Outcome::NoSunEvent(Polar::MidnightSun) => (
            format!(
                "Polar day: the Sun does not {} on this date.",
                if evening { "set" } else { "rise" }
            ),
            Tone::Warn,
        ),
        Outcome::NoSunEvent(Polar::PolarNight) => (
            "Polar night: the Sun stays below the horizon on this date.".into(),
            Tone::Warn,
        ),
        Outcome::SunAltitudeNotReached { .. } => (
            "The Sun does not reach the criterion's evaluation altitude within 12 hours.".into(),
            Tone::Warn,
        ),
    }
}

/// Short reason for a criterion without a verdict.
fn no_verdict_reason(o: &Outcome, s: Sighting) -> &'static str {
    let evening = s == Sighting::Evening;
    match o {
        Outcome::Crescent(_) => "—",
        Outcome::BeforeConjunction(_) if evening => "before new Moon",
        Outcome::BeforeConjunction(_) => "after new Moon",
        Outcome::MoonSetsFirst(_) if evening => "Moon sets first",
        Outcome::MoonSetsFirst(_) => "Moon rises after Sun",
        Outcome::MoonAlwaysUp { .. } => "best time undefined",
        Outcome::MoonAlwaysDown { .. } => "Moon always down",
        Outcome::NoSunEvent(Polar::MidnightSun) => "polar day",
        Outcome::NoSunEvent(Polar::PolarNight) => "polar night",
        Outcome::SunAltitudeNotReached { .. } => "Sun altitude not reached",
    }
}

fn evaluation_label(e: EvaluationTime, s: Sighting) -> String {
    match e {
        EvaluationTime::YallopBestTime => "best time".into(),
        EvaluationTime::Sunset => sun_event(s).into(),
        EvaluationTime::SunAltitude(a) => format!("Sun at {a:+.1}°"),
    }
}

fn heading(s: Sighting) -> &'static str {
    match s {
        Sighting::Evening => "Evening crescent (after sunset)",
        Sighting::Morning => "Morning crescent (before sunrise)",
    }
}

/// The circumstance rows: (label, value).
fn circumstance_rows(r: &SightingReport, key: &ReportKey) -> Vec<(String, String)> {
    let tz = key.tz_offset_hours;
    let d = Some((key.date.year, key.date.month, key.date.day));
    let t = |jd: f64| fmt_local_ut(jd, tz, d);
    let evening = r.sighting == Sighting::Evening;
    let mut rows = Vec::new();
    let Some(c) = r.circumstances() else {
        rows.push((
            "New Moon".into(),
            fmt_local_ut(r.conjunction.jd_ut, tz, None),
        ));
        return rows;
    };
    let g = &c.geometry;
    rows.push((
        if evening { "Sunset" } else { "Sunrise" }.into(),
        t(c.sun_event.jd_ut),
    ));
    rows.push((
        if evening { "Moonset" } else { "Moonrise" }.into(),
        c.moon_event.map_or_else(
            || {
                if evening {
                    "does not set".into()
                } else {
                    "does not rise".into()
                }
            },
            |m| t(m.jd_ut),
        ),
    ));
    rows.push(("Lag".into(), fmt_minutes(g.lag_minutes)));
    rows.push((
        "New Moon".into(),
        fmt_local_ut(c.conjunction.jd_ut, tz, None),
    ));
    let at_best = r.best.circumstances().is_some();
    rows.push((
        if evening {
            "Moon age"
        } else {
            "Time to new Moon"
        }
        .into(),
        fmt_hours(g.age_hours),
    ));
    rows.push((
        if at_best {
            "Best time (Yallop)".into()
        } else {
            format!("Evaluated at {}", sun_event(r.sighting))
        },
        t(c.evaluation.jd_ut),
    ));
    rows.push((
        "Illuminated".into(),
        format!("{:.2}%", g.illuminated_fraction * 100.0),
    ));
    rows.push((
        "Moon alt / az".into(),
        format!(
            "{:.2}° (airless {:.2}°) / {:.2}°",
            g.moon_alt_deg, g.moon_alt_airless_deg, g.moon_az_deg
        ),
    ));
    rows.push((
        "Sun alt / az".into(),
        format!("{:.2}° / {:.2}°", g.sun_alt_deg, g.sun_az_deg),
    ));
    rows.push((
        "ARCL".into(),
        format!("{:.3}° (geocentric {:.3}°)", g.arcl_deg, g.arcl_geo_deg),
    ));
    rows.push((
        "ARCV".into(),
        format!("{:.3}° (topocentric {:.3}°)", g.arcv_deg, g.arcv_topo_deg),
    ));
    rows.push(("DAZ".into(), format!("{:.3}°", g.daz_deg)));
    rows.push((
        "Width W′ / W".into(),
        format!(
            "{:.3}′ topocentric / {:.3}′ geocentric",
            g.width_topo_arcmin, g.width_geo_arcmin
        ),
    ));
    rows.push((
        "Semi-diameter".into(),
        format!("{:.2}′", g.semi_diameter_topo_arcmin),
    ));
    rows.push((
        "Moon distance".into(),
        format!("{:.0} km", g.moon_distance_km),
    ));
    rows
}

/// The whole report as plain text (File → Copy report as text).
pub fn report_text(data: &ReportData) -> String {
    let k = &data.key;
    let mut out = String::new();
    let _ = writeln!(out, "MoonCalc-rs crescent visibility report");
    let _ = writeln!(
        out,
        "Location: {} ({}, {:.0} m)",
        k.name,
        format_lat_lon(k.lat_deg, k.lon_deg),
        k.elevation_m
    );
    let _ = writeln!(
        out,
        "Date: {:04}-{:02}-{:02} (local, {})",
        k.date.year,
        k.date.month,
        k.date.day,
        crate::datetime::format_utc_offset(k.tz_offset_hours)
    );
    let _ = writeln!(out, "Ephemeris: {}", k.ephemeris);
    if let Some(n) = k.map_date_note() {
        let _ = writeln!(out, "Note: {}", n.text);
    }
    for r in [&data.evening, &data.morning] {
        let _ = writeln!(out);
        let _ = writeln!(out, "== {} ==", heading(r.sighting));
        let _ = writeln!(out, "{}", outcome_text(&r.best, r.sighting).0);
        for (label, value) in circumstance_rows(r, k) {
            let _ = writeln!(out, "  {label:<20} {value}");
        }
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "  {:<34} {:<14} {:<5} {:>9}  Description",
            "Criterion", "Evaluated at", "Zone", "Value"
        );
        for row in &r.rows {
            let (code, desc) = match row.zone() {
                Some(z) => (z.code.to_owned(), z.description.to_owned()),
                None => (
                    "—".into(),
                    no_verdict_reason(&row.outcome, r.sighting).into(),
                ),
            };
            let value = row
                .result
                .and_then(|r| r.value)
                .map_or_else(String::new, |v| format!("{v:+.3}"));
            let _ = writeln!(
                out,
                "  {:<34} {:<14} {:<5} {:>9}  {}",
                row.name,
                evaluation_label(row.evaluation, r.sighting),
                code,
                value,
                desc
            );
        }
    }
    out
}

/// State of the Report tab.
#[derive(Default)]
pub struct ReportTab {
    job: JobSlot<ReportData>,
    data: Option<ReportData>,
    submitted: Option<ReportKey>,
    error: Option<String>,
}

impl ReportTab {
    /// The current report, if computed.
    pub fn data(&self) -> Option<&ReportData> {
        self.data.as_ref()
    }

    /// The report as text, for the clipboard.
    pub fn text(&self) -> Option<String> {
        self.data.as_ref().map(report_text)
    }

    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>) {
        let active = backend::resolve(tc.settings.backend, &tc.settings.de440_path);
        let key = ReportKey::from_settings(tc.settings, &active);
        if self.submitted.as_ref() != Some(&key) {
            self.submitted = Some(key.clone());
            if let Some(problem) = active.coverage_problem(key.civil_noon().jd_ut, 3.0) {
                self.job.cancel();
                self.error = Some(problem);
            } else {
                let eph = active.eph.clone();
                let repaint = ctx.clone();
                self.job.submit(
                    "Computing report",
                    move || repaint.request_repaint(),
                    move |job| compute_report(key, eph.as_ref(), job),
                );
            }
        }
        match self.job.poll() {
            Some(Ok(d)) => {
                self.data = Some(d);
                self.error = None;
            }
            Some(Err(e)) => self.error = Some(e),
            None => {}
        }
        if let Some(p) = self.job.progress() {
            tc.status.report_job(p);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, format!("Report: {e}"));
        }
        let Some(data) = &self.data else {
            ui.spinner();
            return;
        };
        let k = &data.key;
        ui.horizontal_wrapped(|ui| {
            ui.heading("Location report");
            ui.separator();
            ui.label(format!(
                "{} — {}, {:.0} m",
                k.name,
                format_lat_lon(k.lat_deg, k.lon_deg),
                k.elevation_m
            ));
            ui.separator();
            ui.label(format!(
                "{:04}-{:02}-{:02} (local date, {})",
                k.date.year,
                k.date.month,
                k.date.day,
                crate::datetime::format_utc_offset(k.tz_offset_hours)
            ));
            ui.separator();
            if ui.button("Copy as text").clicked() {
                ui.ctx().copy_text(report_text(data));
                tc.status.notify("Report copied to the clipboard");
            }
        });
        ui.weak(format!(
            "Ephemeris: {}. Times are local ({}) with UT in brackets. Rise/set use the \
             standard horizon (upper limb, 34′ refraction).",
            k.ephemeris,
            crate::datetime::format_utc_offset(k.tz_offset_hours)
        ));
        if let Some(n) = k.map_date_note() {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!("Map date: {}", n.text))
                        .color(ui.visuals().warn_fg_color),
                )
                .wrap(),
            );
        }
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            if ui.available_width() > 1100.0 {
                ui.columns(2, |cols| {
                    sighting_ui(&mut cols[0], &data.evening, k);
                    sighting_ui(&mut cols[1], &data.morning, k);
                });
            } else {
                sighting_ui(ui, &data.evening, k);
                ui.add_space(12.0);
                sighting_ui(ui, &data.morning, k);
            }
        });
    }
}

fn sighting_ui(ui: &mut egui::Ui, r: &SightingReport, key: &ReportKey) {
    let id = match r.sighting {
        Sighting::Evening => "evening",
        Sighting::Morning => "morning",
    };
    ui.push_id(id, |ui| {
        ui.heading(heading(r.sighting));
        let (text, tone) = outcome_text(&r.best, r.sighting);
        let color = match tone {
            Tone::Good => egui::Color32::from_rgb(40, 160, 70),
            Tone::Warn => ui.visuals().warn_fg_color,
            Tone::Bad => ui.visuals().error_fg_color,
        };
        ui.add(egui::Label::new(egui::RichText::new(text).color(color)).wrap());
        ui.add_space(4.0);
        egui::Grid::new("circumstances")
            .num_columns(2)
            .spacing([12.0, 3.0])
            .striped(true)
            .show(ui, |ui| {
                for (label, value) in circumstance_rows(r, key) {
                    ui.label(label);
                    ui.monospace(value);
                    ui.end_row();
                }
            });
        ui.add_space(8.0);
        ui.strong("Criteria");
        egui::Grid::new("criteria")
            .num_columns(5)
            .spacing([8.0, 3.0])
            .striped(true)
            .show(ui, |ui| {
                ui.weak("Criterion");
                ui.weak("At");
                ui.weak("Zone");
                ui.weak("Value");
                ui.weak("Description");
                ui.end_row();
                for row in &r.rows {
                    ui.label(row.name);
                    ui.weak(evaluation_label(row.evaluation, r.sighting));
                    match row.zone() {
                        Some(z) => {
                            ui.horizontal(|ui| {
                                swatch(ui, z.rgb);
                                ui.strong(z.code);
                            });
                            ui.monospace(
                                row.result
                                    .and_then(|r| r.value)
                                    .map_or_else(String::new, |v| format!("{v:+.3}")),
                            );
                            ui.add(egui::Label::new(z.description).wrap());
                        }
                        None => {
                            ui.label("—");
                            ui.label("");
                            ui.weak(no_verdict_reason(&row.outcome, r.sighting));
                        }
                    }
                    ui.end_row();
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::MeeusEphemeris;

    fn key(lat: f64, lon: f64, date: CalendarDate, tz: f64) -> ReportKey {
        ReportKey {
            date,
            tz_offset_hours: tz,
            name: "Test".into(),
            lat_deg: lat,
            lon_deg: lon,
            elevation_m: 0.0,
            ephemeris: "meeus".into(),
        }
    }

    #[test]
    fn march_2024_evening_crescent_in_the_americas() {
        // New Moon 2024-03-10 09:00 UT: the evening crescent is visible
        // from Honolulu that evening (Yallop A/B, as on HMNAO's map), and
        // there is no old crescent that morning.
        let eph = MeeusEphemeris::new();
        let d = compute_report(
            key(21.31, -157.86, CalendarDate::new(2024, 3, 10), -10.0),
            &eph,
            &JobContext::detached(),
        )
        .unwrap();
        assert!(matches!(d.evening.best, Outcome::Crescent(_)));
        let yallop = d.evening.rows.iter().find(|r| r.id == "yallop").unwrap();
        assert!(
            yallop
                .zone()
                .is_some_and(|z| z.code == "A" || z.code == "B")
        );
        assert_eq!(d.evening.rows.len(), all().len());
        assert!(!matches!(d.morning.best, Outcome::Crescent(_)));
        let text = report_text(&d);
        assert!(text.contains("Yallop") && text.contains("Sunset"));
        assert!(text.contains("UTC-10:00"));
    }

    #[test]
    fn apia_report_names_the_map_date_that_shows_its_evening() {
        // Apia (UTC+13, 171.8° W): the civil evening of 2024-03-10 is
        // "Moon sets first"; the map for 2024-03-10 shows the next evening
        // there, so the report points to the map for 2024-03-09.
        let eph = MeeusEphemeris::new();
        let k = key(-13.8333, -171.7667, CalendarDate::new(2024, 3, 10), 13.0);
        let note = k.map_date_note().expect("note");
        assert_eq!(note.day_shift, -1);
        let d = compute_report(k, &eph, &JobContext::detached()).unwrap();
        assert!(matches!(d.evening.best, Outcome::MoonSetsFirst(_)));
        assert!(report_text(&d).contains("The world map for 2024-03-09"));
        // London: no note.
        let london = key(51.5, -0.13, CalendarDate::new(2024, 3, 10), 0.0);
        assert!(london.map_date_note().is_none());
    }

    #[test]
    fn polar_night_is_reported() {
        let eph = MeeusEphemeris::new();
        let d = compute_report(
            key(85.0, 0.0, CalendarDate::new(2024, 12, 31), 0.0),
            &eph,
            &JobContext::detached(),
        )
        .unwrap();
        assert!(matches!(
            d.evening.best,
            Outcome::NoSunEvent(Polar::PolarNight)
        ));
        assert!(d.evening.rows.iter().all(|r| r.result.is_none()));
        assert!(
            outcome_text(&d.evening.best, Sighting::Evening)
                .0
                .contains("Polar night")
        );
        // The text report still lists the conjunction.
        assert!(report_text(&d).contains("New Moon"));
    }
}
