//! **Report** tab: the observing circumstances of the evening and morning
//! crescent for the selected observer and local date, and every criterion's
//! verdict.
//!
//! [`ReportTab::poll`] recomputes in a background job whenever the observer,
//! the local date, the time zone or the ephemeris changes
//! ([`compute_report`]). The computation is the shared service
//! `mooncalc_app::report::location_report`, which the Tauri frontend uses
//! too: each criterion at its own evaluation time, with the day anchored at
//! **civil** noon in the user's time zone, so "the evening of 10 March" is
//! the one a local observer means. This module words and lays out its
//! [`LocationReport`].

use std::fmt::Write as _;

use mooncalc_app::dto::{
    Crescent, CrescentGeometryDto, CrescentReport, CriterionVerdict, Location, LocationReport,
    LocationReportParams, UtDateTime,
};
use mooncalc_app::report::{COVERAGE_MARGIN_DAYS, location_report, status_from_code};
use mooncalc_core::Ephemeris;
use mooncalc_core::events::CalendarDate;
use mooncalc_core::time::Instant;
use mooncalc_visibility::circumstances::Status;
use mooncalc_visibility::criteria::EvaluationTime;

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

    /// The service request.
    pub fn params(&self) -> LocationReportParams {
        LocationReportParams {
            date: self.date.into(),
            tz_offset_hours: self.tz_offset_hours,
            location: Location {
                lat_deg: self.lat_deg,
                lon_deg: self.lon_deg,
                elevation_m: self.elevation_m,
            },
            ephemeris_id: None,
        }
    }
}

/// A complete report: the request and the service's answer.
#[derive(Clone, Debug)]
pub struct ReportData {
    pub key: ReportKey,
    pub report: LocationReport,
}

/// Compute the report for `key` (`mooncalc_app::report::location_report`).
pub fn compute_report(
    key: ReportKey,
    eph: &dyn Ephemeris,
    job: &JobContext,
) -> Result<ReportData, JobError> {
    job.checkpoint()?;
    let report = location_report(eph, &key.params()).map_err(|e| JobError::Failed(e.message))?;
    Ok(ReportData { key, report })
}

/// How serious an outcome is, for colouring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Warn,
    Bad,
}

fn sun_event(c: Crescent) -> &'static str {
    match c {
        Crescent::Evening => "sunset",
        Crescent::Morning => "sunrise",
    }
}

fn jd(t: &UtDateTime) -> f64 {
    t.to_instant().jd_ut
}

/// The lag, minutes (infinite where the Moon does not set or rise).
fn lag(g: Option<&CrescentGeometryDto>) -> f64 {
    g.map_or(f64::NAN, |g| g.lag_minutes.unwrap_or(f64::INFINITY))
}

fn age(g: Option<&CrescentGeometryDto>) -> f64 {
    g.map_or(f64::NAN, |g| g.age_hours)
}

/// A sentence describing the evening (morning) at Yallop's best time.
pub fn outcome_text(c: &CrescentReport) -> (String, Tone) {
    let evening = c.crescent == Crescent::Evening;
    let g = c.geometry.as_ref();
    let Some(status) = status_from_code(&c.status) else {
        return (c.status_text.clone(), Tone::Warn);
    };
    match status {
        Status::Crescent => (
            if evening {
                format!(
                    "The crescent can be looked for after sunset: the Moon sets {} after the Sun, {} after the new Moon.",
                    fmt_minutes(lag(g)),
                    fmt_hours(age(g))
                )
            } else {
                format!(
                    "The old crescent can be looked for before sunrise: the Moon rises {} before the Sun, {} before the new Moon.",
                    fmt_minutes(lag(g)),
                    fmt_hours(age(g))
                )
            },
            Tone::Good,
        ),
        Status::BeforeConjunction => (
            if evening {
                format!(
                    "No crescent: the Moon sets after the Sun, but the new Moon has not happened yet at the best time ({} to go).",
                    fmt_hours(-age(g))
                )
            } else {
                format!(
                    "No old crescent: the Moon rises before the Sun, but the new Moon has already happened ({} ago).",
                    fmt_hours(-age(g))
                )
            },
            Tone::Bad,
        ),
        Status::MoonSetsFirst => (
            if evening {
                format!(
                    "No crescent: the Moon sets before the Sun (lag {}).",
                    fmt_minutes(lag(g))
                )
            } else {
                format!(
                    "No old crescent: the Moon rises after the Sun (lag {}).",
                    fmt_minutes(lag(g))
                )
            },
            Tone::Bad,
        ),
        Status::MoonAlwaysUp => (
            format!(
                "The Moon does not {} within 12 hours of {}; Yallop's best time is undefined (criteria evaluated at {} still apply).",
                if evening { "set" } else { "rise" },
                sun_event(c.crescent),
                sun_event(c.crescent)
            ),
            Tone::Warn,
        ),
        Status::MoonAlwaysDown => (
            "No crescent: the Moon stays below the horizon all day.".into(),
            Tone::Bad,
        ),
        Status::MidnightSun => (
            format!(
                "Polar day: the Sun does not {} on this date.",
                if evening { "set" } else { "rise" }
            ),
            Tone::Warn,
        ),
        Status::PolarNight => (
            "Polar night: the Sun stays below the horizon on this date.".into(),
            Tone::Warn,
        ),
        Status::SunAltitudeNotReached => (
            "The Sun does not reach the criterion's evaluation altitude within 12 hours.".into(),
            Tone::Warn,
        ),
        Status::OutsideEphemeris => (
            "Not computed: the date is outside the ephemeris coverage.".into(),
            Tone::Warn,
        ),
    }
}

/// Short reason for a criterion without a verdict, from the status at its
/// evaluation time.
fn no_verdict_reason(v: &CriterionVerdict, crescent: Crescent) -> &'static str {
    let evening = crescent == Crescent::Evening;
    match status_from_code(&v.status) {
        None | Some(Status::Crescent) => "—",
        Some(Status::BeforeConjunction) if evening => "before new Moon",
        Some(Status::BeforeConjunction) => "after new Moon",
        Some(Status::MoonSetsFirst) if evening => "Moon sets first",
        Some(Status::MoonSetsFirst) => "Moon rises after Sun",
        Some(Status::MoonAlwaysUp) => "best time undefined",
        Some(Status::MoonAlwaysDown) => "Moon always down",
        Some(Status::MidnightSun) => "polar day",
        Some(Status::PolarNight) => "polar night",
        Some(Status::SunAltitudeNotReached) => "Sun altitude not reached",
        Some(Status::OutsideEphemeris) => "outside ephemeris",
    }
}

/// The criterion's evaluation-time rule, short: "best time", "sunset" or
/// "sunrise", "Sun at -4.0°".
fn evaluation_label(v: &CriterionVerdict, crescent: Crescent) -> String {
    match mooncalc_app::criteria::get(&v.criterion_id).map(|c| c.evaluation_time()) {
        Ok(EvaluationTime::YallopBestTime) => "best time".into(),
        Ok(EvaluationTime::Sunset) => sun_event(crescent).into(),
        Ok(EvaluationTime::SunAltitude(a)) => format!("Sun at {a:+.1}°"),
        Err(_) => v.evaluation_time.clone(),
    }
}

fn value_text(v: &CriterionVerdict) -> String {
    v.verdict
        .as_ref()
        .and_then(|z| z.value)
        .map_or_else(String::new, |v| format!("{v:+.3}"))
}

fn heading(c: Crescent) -> &'static str {
    match c {
        Crescent::Evening => "Evening crescent (after sunset)",
        Crescent::Morning => "Morning crescent (before sunrise)",
    }
}

/// The circumstance rows: (label, value).
fn circumstance_rows(r: &CrescentReport, report: &LocationReport) -> Vec<(String, String)> {
    let tz = report.tz_offset_hours;
    let d = Some((report.date.year, report.date.month, report.date.day));
    let t = |u: &UtDateTime| fmt_local_ut(jd(u), tz, d);
    let new_moon = r
        .conjunction
        .as_ref()
        .map_or_else(|| "—".into(), |c| fmt_local_ut(jd(c), tz, None));
    let evening = r.crescent == Crescent::Evening;
    let mut rows = Vec::new();
    let Some(g) = &r.geometry else {
        rows.push(("New Moon".into(), new_moon));
        return rows;
    };
    rows.push((
        if evening { "Sunset" } else { "Sunrise" }.into(),
        r.sun_event.as_ref().map_or_else(|| "—".into(), t),
    ));
    rows.push((
        if evening { "Moonset" } else { "Moonrise" }.into(),
        r.moon_event.as_ref().map_or_else(
            || {
                if evening {
                    "does not set".into()
                } else {
                    "does not rise".into()
                }
            },
            t,
        ),
    ));
    rows.push(("Lag".into(), fmt_minutes(lag(Some(g)))));
    rows.push(("New Moon".into(), new_moon));
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
        if r.geometry_at_sun_event {
            format!("Evaluated at {}", sun_event(r.crescent))
        } else {
            "Best time (Yallop)".into()
        },
        r.geometry_at.as_ref().map_or_else(|| "—".into(), t),
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

fn date_text(r: &LocationReport) -> String {
    format!("{:04}-{:02}-{:02}", r.date.year, r.date.month, r.date.day)
}

/// The whole report as plain text (File → Copy report as text).
pub fn report_text(data: &ReportData) -> String {
    let r = &data.report;
    let mut out = String::new();
    let _ = writeln!(out, "MoonCalc-rs crescent visibility report");
    let _ = writeln!(
        out,
        "Location: {} ({}, {:.0} m)",
        data.key.name,
        format_lat_lon(r.location.lat_deg, r.location.lon_deg),
        r.location.elevation_m
    );
    let _ = writeln!(
        out,
        "Date: {} (local, {})",
        date_text(r),
        crate::datetime::format_utc_offset(r.tz_offset_hours)
    );
    let _ = writeln!(out, "Ephemeris: {}", r.ephemeris);
    if let Some(n) = &r.map_date_note {
        let _ = writeln!(out, "Note: {}", n.text);
    }
    for c in [&r.evening, &r.morning] {
        let _ = writeln!(out);
        let _ = writeln!(out, "== {} ==", heading(c.crescent));
        let _ = writeln!(out, "{}", outcome_text(c).0);
        for (label, value) in circumstance_rows(c, r) {
            let _ = writeln!(out, "  {label:<20} {value}");
        }
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "  {:<34} {:<14} {:<5} {:>9}  Description",
            "Criterion", "Evaluated at", "Zone", "Value"
        );
        for v in &c.criteria {
            let (code, desc) = match &v.verdict {
                Some(z) => (z.code.as_str(), z.description.as_str()),
                None => ("—", no_verdict_reason(v, c.crescent)),
            };
            let _ = writeln!(
                out,
                "  {:<34} {:<14} {:<5} {:>9}  {}",
                v.name,
                evaluation_label(v, c.crescent),
                code,
                value_text(v),
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
            let noon = key.civil_noon().jd_ut;
            if let Some(problem) =
                active.span_problem(noon, noon, COVERAGE_MARGIN_DAYS, COVERAGE_MARGIN_DAYS)
            {
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
            // Only spin while a report is actually being computed; outside the
            // ephemeris coverage nothing is submitted and the error says why.
            if self.job.is_running() {
                ui.spinner();
            }
            return;
        };
        let r = &data.report;
        let offset = crate::datetime::format_utc_offset(r.tz_offset_hours);
        ui.horizontal_wrapped(|ui| {
            ui.heading("Location report");
            ui.separator();
            ui.label(format!(
                "{} — {}, {:.0} m",
                data.key.name,
                format_lat_lon(r.location.lat_deg, r.location.lon_deg),
                r.location.elevation_m
            ));
            ui.separator();
            ui.label(format!("{} (local date, {offset})", date_text(r)));
            ui.separator();
            if ui.button("Copy as text").clicked() {
                ui.ctx().copy_text(report_text(data));
                tc.status.notify("Report copied to the clipboard");
            }
        });
        ui.weak(format!(
            "Ephemeris: {}. Times are local ({offset}) with UT in brackets. Rise/set use the \
             standard horizon (upper limb, 34′ refraction).",
            r.ephemeris,
        ));
        if let Some(n) = &r.map_date_note {
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
                    crescent_ui(&mut cols[0], &r.evening, r);
                    crescent_ui(&mut cols[1], &r.morning, r);
                });
            } else {
                crescent_ui(ui, &r.evening, r);
                ui.add_space(12.0);
                crescent_ui(ui, &r.morning, r);
            }
        });
    }
}

fn crescent_ui(ui: &mut egui::Ui, c: &CrescentReport, report: &LocationReport) {
    let id = match c.crescent {
        Crescent::Evening => "evening",
        Crescent::Morning => "morning",
    };
    ui.push_id(id, |ui| {
        ui.heading(heading(c.crescent));
        let (text, tone) = outcome_text(c);
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
                for (label, value) in circumstance_rows(c, report) {
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
                for v in &c.criteria {
                    ui.label(&v.name);
                    ui.weak(evaluation_label(v, c.crescent));
                    match &v.verdict {
                        Some(z) => {
                            ui.horizontal(|ui| {
                                swatch(ui, z.rgb);
                                ui.strong(&z.code);
                            });
                            ui.monospace(value_text(v));
                            ui.add(egui::Label::new(&z.description).wrap());
                        }
                        None => {
                            ui.label("—");
                            ui.label("");
                            ui.weak(no_verdict_reason(v, c.crescent));
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

    fn report(k: ReportKey) -> ReportData {
        compute_report(k, &MeeusEphemeris::new(), &JobContext::detached()).unwrap()
    }

    #[test]
    fn march_2024_evening_crescent_in_the_americas() {
        // New Moon 2024-03-10 09:00 UT: the evening crescent is visible
        // from Honolulu that evening (Yallop A/B, as on HMNAO's map), and
        // there is no old crescent that morning.
        let d = report(key(21.31, -157.86, CalendarDate::new(2024, 3, 10), -10.0));
        let r = &d.report;
        assert_eq!(r.evening.status, "crescent");
        assert_eq!(outcome_text(&r.evening).1, Tone::Good);
        let yallop = r
            .evening
            .criteria
            .iter()
            .find(|v| v.criterion_id == "yallop")
            .unwrap();
        assert!(
            yallop
                .verdict
                .as_ref()
                .is_some_and(|z| z.code == "A" || z.code == "B")
        );
        assert_eq!(
            r.evening.criteria.len(),
            mooncalc_visibility::criteria::all().len()
        );
        assert_ne!(r.morning.status, "crescent");
        let text = report_text(&d);
        assert!(text.contains("Yallop") && text.contains("Sunset"));
        assert!(text.contains("UTC-10:00"));
        assert!(text.contains("Best time (Yallop)"));
    }

    #[test]
    fn apia_report_names_the_map_date_that_shows_its_evening() {
        // Apia (UTC+13, 171.8° W): the civil evening of 2024-03-10 is
        // "Moon sets first"; the map for 2024-03-10 shows the next evening
        // there, so the report points to the map for 2024-03-09.
        let d = report(key(
            -13.8333,
            -171.7667,
            CalendarDate::new(2024, 3, 10),
            13.0,
        ));
        let note = d.report.map_date_note.as_ref().expect("note");
        assert_eq!(note.day_shift, -1);
        assert_eq!(d.report.evening.status, "moon-sets-first");
        assert!(report_text(&d).contains("The world map for 2024-03-09"));
        // London: no note.
        let london = report(key(51.5, -0.13, CalendarDate::new(2024, 3, 10), 0.0));
        assert!(london.report.map_date_note.is_none());
    }

    #[test]
    fn polar_night_is_reported() {
        let d = report(key(85.0, 0.0, CalendarDate::new(2024, 12, 31), 0.0));
        let e = &d.report.evening;
        assert_eq!(e.status, "polar-night");
        assert!(e.criteria.iter().all(|v| v.verdict.is_none()));
        assert!(outcome_text(e).0.contains("Polar night"));
        // The text report still lists the conjunction.
        let text = report_text(&d);
        assert!(text.contains("New Moon             2024-12-30"), "{text}");
        assert!(text.contains("polar night"));
    }

    #[test]
    fn moon_always_up_shows_the_sunset_circumstances() {
        // Tromsø, 2025-04-01: the young Moon does not set; the criteria
        // evaluated at sunset still give verdicts, Yallop does not.
        let mut k = key(69.65, 18.96, CalendarDate::new(2025, 4, 1), 2.0);
        k.name = "Tromso".into();
        let text = report_text(&report(k));
        for want in [
            "Yallop's best time is undefined",
            "Moonset              does not set",
            "Lag                  ∞",
            "Evaluated at sunset  19:50:33 UTC+02:00 (17:50:33 UT)",
            "Yallop (1997)                      best time      —                best time undefined",
            "SAAO (Caldwell & Laney 2001)       sunset         A       +26.928",
            "Evaluated at sunrise",
        ] {
            assert!(text.contains(want), "{want:?} not in\n{text}");
        }
    }
}
