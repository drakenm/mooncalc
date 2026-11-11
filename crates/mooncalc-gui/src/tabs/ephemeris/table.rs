//! Ephemeris table: rows of `mooncalc_core::tables::EphemerisRow` at a
//! configurable start, step and count, with selectable columns, shown in a
//! virtualised table and copyable as CSV/TSV.

use std::sync::Arc;

use egui_extras::{Column, TableBuilder};
use mooncalc_core::Instant;
use mooncalc_core::angle::DEG;
use mooncalc_core::tables::{EphemerisRow, TableOptions, ephemeris_table};
use mooncalc_core::topocentric::AU_KM;
use serde::{Deserialize, Serialize};

use super::cached::CachedJob;
use super::format::{self, AngleStyle};
use super::{EphKey, busy, ephemeris, error_line, set_app_time};
use crate::jobs::parallel_rows;
use crate::tabs::TabContext;

/// Largest table the UI will compute.
pub const MAX_ROWS: usize = 200_000;
/// Rows per parallel work unit.
const CHUNK: usize = 256;

/// Step units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepUnit {
    Minutes,
    Hours,
    #[default]
    Days,
}

impl StepUnit {
    pub const ALL: [Self; 3] = [Self::Minutes, Self::Hours, Self::Days];

    pub fn label(self) -> &'static str {
        match self {
            Self::Minutes => "minutes",
            Self::Hours => "hours",
            Self::Days => "days",
        }
    }

    pub fn days(self) -> f64 {
        match self {
            Self::Minutes => 1.0 / 1440.0,
            Self::Hours => 1.0 / 24.0,
            Self::Days => 1.0,
        }
    }
}

/// Table columns (besides the date/time, which is always shown).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Col {
    JdUt,
    SunRa,
    SunDec,
    SunLon,
    SunLat,
    SunDist,
    SunSd,
    SunHp,
    MoonRa,
    MoonDec,
    MoonLon,
    MoonLat,
    MoonDist,
    MoonSd,
    MoonHp,
    Elongation,
    PhaseAngle,
    Illumination,
    BrightLimb,
    Age,
    SunAlt,
    SunAz,
    MoonAlt,
    MoonAz,
    MoonTopoSd,
    TopoElongation,
    LibLon,
    LibLat,
    AxisP,
    Colongitude,
}

impl Col {
    pub const ALL: [Self; 30] = [
        Self::JdUt,
        Self::SunRa,
        Self::SunDec,
        Self::SunLon,
        Self::SunLat,
        Self::SunDist,
        Self::SunSd,
        Self::SunHp,
        Self::MoonRa,
        Self::MoonDec,
        Self::MoonLon,
        Self::MoonLat,
        Self::MoonDist,
        Self::MoonSd,
        Self::MoonHp,
        Self::Elongation,
        Self::PhaseAngle,
        Self::Illumination,
        Self::BrightLimb,
        Self::Age,
        Self::SunAlt,
        Self::SunAz,
        Self::MoonAlt,
        Self::MoonAz,
        Self::MoonTopoSd,
        Self::TopoElongation,
        Self::LibLon,
        Self::LibLat,
        Self::AxisP,
        Self::Colongitude,
    ];

    pub const GROUPS: [&'static str; 6] =
        ["Time", "Sun", "Moon", "Phase", "Topocentric", "Libration"];

    pub fn group(self) -> &'static str {
        use Col::*;
        match self {
            JdUt => "Time",
            SunRa | SunDec | SunLon | SunLat | SunDist | SunSd | SunHp => "Sun",
            MoonRa | MoonDec | MoonLon | MoonLat | MoonDist | MoonSd | MoonHp => "Moon",
            Elongation | PhaseAngle | Illumination | BrightLimb | Age => "Phase",
            SunAlt | SunAz | MoonAlt | MoonAz | MoonTopoSd | TopoElongation => "Topocentric",
            LibLon | LibLat | AxisP | Colongitude => "Libration",
        }
    }

    /// Column header.
    pub fn header(self) -> &'static str {
        use Col::*;
        match self {
            JdUt => "JD (UT)",
            SunRa => "Sun RA",
            SunDec => "Sun Dec",
            SunLon => "Sun λ",
            SunLat => "Sun β",
            SunDist => "Sun dist (AU)",
            SunSd => "Sun SD",
            SunHp => "Sun HP",
            MoonRa => "Moon RA",
            MoonDec => "Moon Dec",
            MoonLon => "Moon λ",
            MoonLat => "Moon β",
            MoonDist => "Moon dist (km)",
            MoonSd => "Moon SD",
            MoonHp => "Moon HP",
            Elongation => "Elongation",
            PhaseAngle => "Phase angle",
            Illumination => "Illum. (%)",
            BrightLimb => "Bright limb χ",
            Age => "Age (d)",
            SunAlt => "Sun alt",
            SunAz => "Sun az",
            MoonAlt => "Moon alt",
            MoonAz => "Moon az",
            MoonTopoSd => "Moon topo SD",
            TopoElongation => "Topo elong.",
            LibLon => "Lib. l",
            LibLat => "Lib. b",
            AxisP => "Axis P",
            Colongitude => "Colongitude",
        }
    }

    /// Longer description for the column menu.
    pub fn description(self) -> &'static str {
        use Col::*;
        match self {
            JdUt => "Julian Day (UT)",
            SunRa | MoonRa => "Geocentric apparent right ascension",
            SunDec | MoonDec => "Geocentric apparent declination",
            SunLon | MoonLon => "Apparent ecliptic longitude (true equinox of date)",
            SunLat | MoonLat => "Apparent ecliptic latitude",
            SunDist => "Geocentric distance, astronomical units",
            MoonDist => "Geocentric distance, km",
            SunSd | MoonSd => "Geocentric semi-diameter",
            SunHp | MoonHp => "Equatorial horizontal parallax",
            Elongation => "Geocentric elongation of the Moon from the Sun",
            PhaseAngle => "Sun–Moon–Earth angle",
            Illumination => "Illuminated fraction of the disc",
            BrightLimb => "Position angle of the bright limb (from north, eastward)",
            Age => "Days since the last new Moon (conjunction)",
            SunAlt | MoonAlt => "Topocentric altitude with standard refraction, app location",
            SunAz | MoonAz => "Topocentric azimuth (north through east), app location",
            MoonTopoSd => "Topocentric semi-diameter of the Moon",
            TopoElongation => "Elongation seen from the app location",
            LibLon => "Total geocentric libration in longitude",
            LibLat => "Total geocentric libration in latitude",
            AxisP => "Position angle of the Moon's axis",
            Colongitude => "Selenographic colongitude of the Sun",
        }
    }

    /// Whether the column needs the observer (topocentric quantities).
    pub fn topocentric(self) -> bool {
        self.group() == "Topocentric"
    }

    /// The cell text for one row.
    pub fn cell(self, r: &EphemerisRow, style: AngleStyle) -> String {
        use Col::*;
        let topo_sun = r.sun.topocentric.as_ref();
        let topo_moon = r.moon.topocentric.as_ref();
        let dash = || "—".to_string();
        match self {
            JdUt => format!("{:.5}", r.instant.jd_ut),
            SunRa => format::ra(r.sun.equatorial.ra, style),
            SunDec => format::signed_angle(r.sun.equatorial.dec, style),
            SunLon => format::angle_360(r.sun.ecliptic.lon, style),
            SunLat => format!("{:+.2}″", r.sun.ecliptic.lat / DEG * 3600.0),
            SunDist => format!("{:.6}", r.sun.equatorial.distance_km / AU_KM),
            SunSd => format::small_angle(r.sun.semidiameter, style),
            SunHp => format!("{:.2}″", r.sun.horizontal_parallax / DEG * 3600.0),
            MoonRa => format::ra(r.moon.equatorial.ra, style),
            MoonDec => format::signed_angle(r.moon.equatorial.dec, style),
            MoonLon => format::angle_360(r.moon.ecliptic.lon, style),
            MoonLat => format::signed_angle(r.moon.ecliptic.lat, style),
            MoonDist => format!("{:.0}", r.moon.equatorial.distance_km),
            MoonSd => format::small_angle(r.moon.semidiameter, style),
            MoonHp => format::small_angle(r.moon.horizontal_parallax, style),
            Elongation => format::angle_360(r.illumination.elongation, style),
            PhaseAngle => format::angle_360(r.illumination.phase_angle, style),
            Illumination => format!("{:.2}", r.illumination.fraction * 100.0),
            BrightLimb => format!("{:.2}°", r.illumination.bright_limb_angle / DEG),
            Age => format!("{:.3}", r.moon_age_days),
            SunAlt => {
                topo_sun.map_or_else(dash, |t| format::signed_angle(t.apparent_altitude, style))
            }
            SunAz => topo_sun.map_or_else(dash, |t| format::angle_360(t.horizontal.az, style)),
            MoonAlt => {
                topo_moon.map_or_else(dash, |t| format::signed_angle(t.apparent_altitude, style))
            }
            MoonAz => topo_moon.map_or_else(dash, |t| format::angle_360(t.horizontal.az, style)),
            MoonTopoSd => {
                topo_moon.map_or_else(dash, |t| format::small_angle(t.semidiameter, style))
            }
            TopoElongation => r
                .topocentric_illumination
                .map_or_else(dash, |i| format::angle_360(i.elongation, style)),
            LibLon => format!("{:+.3}°", r.physical.libration.lon / DEG),
            LibLat => format!("{:+.3}°", r.physical.libration.lat / DEG),
            AxisP => format!("{:.2}°", r.physical.libration.position_angle / DEG),
            Colongitude => format!("{:.2}°", r.physical.colongitude / DEG),
        }
    }
}

/// Persisted table choices.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TableSettings {
    /// Start at the app date/time instead of `start_jd_ut`.
    pub follow_app_time: bool,
    pub start_jd_ut: f64,
    pub step: f64,
    pub step_unit: StepUnit,
    pub count: usize,
    pub columns: Vec<Col>,
    pub sexagesimal: bool,
}

impl Default for TableSettings {
    fn default() -> Self {
        Self {
            follow_app_time: true,
            start_jd_ut: 2_460_310.5,
            step: 1.0,
            step_unit: StepUnit::Days,
            count: 31,
            columns: vec![
                Col::SunRa,
                Col::SunDec,
                Col::MoonRa,
                Col::MoonDec,
                Col::MoonDist,
                Col::Elongation,
                Col::Illumination,
                Col::Age,
                Col::MoonAlt,
                Col::MoonAz,
            ],
            sexagesimal: true,
        }
    }
}

impl TableSettings {
    pub fn step_days(&self) -> f64 {
        (self.step * self.step_unit.days()).max(1.0 / 1440.0)
    }

    /// Selected columns in display order.
    pub fn visible_columns(&self) -> Vec<Col> {
        Col::ALL
            .into_iter()
            .filter(|c| self.columns.contains(c))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Request {
    start_jd_ut: f64,
    step_days: f64,
    count: usize,
    /// Observer (lat, lon, elevation) when topocentric columns are shown.
    location: Option<(f64, f64, f64)>,
    eph: EphKey,
}

/// Transient state of the table view.
#[derive(Default)]
pub struct TableView {
    job: CachedJob<Request, Arc<Vec<EphemerisRow>>>,
    start_text: String,
    start_text_for: Option<(f64, f64)>,
    start_error: bool,
}

impl TableView {
    fn start(tc: &TabContext<'_>) -> f64 {
        let s = &tc.settings.ephemeris_tab.table;
        if s.follow_app_time {
            tc.settings.time.jd_ut
        } else {
            s.start_jd_ut
        }
    }

    fn request(tc: &TabContext<'_>) -> Request {
        let s = &tc.settings.ephemeris_tab.table;
        let topo = s.columns.iter().any(|c| c.topocentric());
        let l = &tc.settings.location;
        Request {
            start_jd_ut: Self::start(tc),
            step_days: s.step_days(),
            count: s.count.clamp(1, MAX_ROWS),
            location: topo.then_some((l.lat_deg, l.lon_deg, l.elevation_m)),
            eph: EphKey::of(tc.settings),
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>, visible: bool) {
        if visible {
            let req = Self::request(tc);
            let eph = ephemeris(tc.settings);
            self.job
                .request(ctx, req, "Computing ephemeris table", move |req, job| {
                    let opts = TableOptions {
                        location: req
                            .location
                            .map(|(lat, lon, h)| mooncalc_core::GeoLocation::new(lat, lon, h)),
                        ..TableOptions::default()
                    };
                    let start = Instant::from_jd_ut(req.start_jd_ut);
                    let chunks = req.count.div_ceil(CHUNK);
                    let parts = parallel_rows(job, chunks, |i| {
                        let first = i * CHUNK;
                        let n = CHUNK.min(req.count - first);
                        let t0 = start.add_days(req.step_days * first as f64);
                        ephemeris_table(eph.as_ref(), t0, req.step_days, n, &opts)
                    })?;
                    Ok(Arc::new(parts.into_iter().flatten().collect()))
                });
        }
        self.job.poll(tc.status);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let tz = tc.settings.time.tz_offset_hours;
        self.controls(ui, tc, tz);
        error_line(ui, &self.job.error);
        busy(ui, self.job.is_running());

        let Some(rows) = self.job.get().cloned() else {
            return;
        };
        let s = &tc.settings.ephemeris_tab.table;
        let style = AngleStyle::from_sexagesimal(s.sexagesimal);
        let cols = s.visible_columns();
        let app_jd = tc.settings.time.jd_ut;
        let step = rows
            .get(1)
            .map_or(1.0, |r| r.instant.jd_ut - rows[0].instant.jd_ut);
        let current = rows
            .iter()
            .position(|r| (r.instant.jd_ut - app_jd).abs() < step * 0.5);

        let mut clicked: Option<Instant> = None;
        let text_height = egui::TextStyle::Monospace.resolve(ui.style()).size + 4.0;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            let mut table = TableBuilder::new(ui)
                .id_salt("ephemeris_table")
                .striped(true)
                .resizable(true)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::auto().at_least(130.0));
            for _ in &cols {
                table = table.column(Column::auto().at_least(60.0));
            }
            table
                .header(text_height + 4.0, |mut header| {
                    header.col(|ui| {
                        ui.strong(format!("Date/time ({})", format::zone_label(tz)));
                    });
                    for c in &cols {
                        header.col(|ui| {
                            ui.strong(c.header()).on_hover_text(c.description());
                        });
                    }
                })
                .body(|body| {
                    body.rows(text_height, rows.len(), |mut row| {
                        let r = &rows[row.index()];
                        row.set_selected(Some(row.index()) == current);
                        row.col(|ui| {
                            ui.monospace(format::date_time(r.instant, tz));
                        });
                        for c in &cols {
                            row.col(|ui| {
                                ui.monospace(c.cell(r, style));
                            });
                        }
                        if row.response().clicked() {
                            clicked = Some(r.instant);
                        }
                    });
                });
        });
        if let Some(t) = clicked {
            let s = &mut tc.settings.ephemeris_tab.table;
            if s.follow_app_time {
                // Keep the table where it is rather than restarting it at
                // the clicked row.
                s.follow_app_time = false;
                s.start_jd_ut = rows[0].instant.jd_ut;
            }
            set_app_time(tc, t);
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>, tz: f64) {
        let start = Self::start(tc);
        let s = &mut tc.settings.ephemeris_tab.table;
        ui.horizontal_wrapped(|ui| {
            ui.label("Start:");
            ui.checkbox(&mut s.follow_app_time, "App time")
                .on_hover_text("Start the table at the app date/time");
            if self.start_text_for != Some((start, tz)) {
                self.start_text = format::date_time(Instant::from_jd_ut(start), tz);
                self.start_text_for = Some((start, tz));
                self.start_error = false;
            }
            let edit = egui::TextEdit::singleline(&mut self.start_text)
                .desired_width(150.0)
                .font(egui::TextStyle::Monospace)
                .hint_text("YYYY-MM-DD HH:MM");
            let edit = if self.start_error {
                edit.text_color(ui.visuals().error_fg_color)
            } else {
                edit
            };
            let r = ui
                .add_enabled(!s.follow_app_time, edit)
                .on_hover_text(format!("Local time ({})", format::zone_label(tz)));
            if r.lost_focus() {
                match format::parse_date_time(&self.start_text, tz) {
                    Some(t) => {
                        s.start_jd_ut = t.jd_ut;
                        self.start_error = false;
                    }
                    None => self.start_error = true,
                }
            }
            ui.separator();
            ui.label("Step:");
            ui.add(
                egui::DragValue::new(&mut s.step)
                    .range(1.0..=1000.0)
                    .speed(0.1)
                    .fixed_decimals(0),
            );
            egui::ComboBox::from_id_salt("table_step_unit")
                .selected_text(s.step_unit.label())
                .width(80.0)
                .show_ui(ui, |ui| {
                    for u in StepUnit::ALL {
                        ui.selectable_value(&mut s.step_unit, u, u.label());
                    }
                });
            ui.label("Rows:");
            ui.add(
                egui::DragValue::new(&mut s.count)
                    .range(1..=MAX_ROWS)
                    .speed(1.0),
            );
            ui.separator();
            ui.selectable_value(&mut s.sexagesimal, true, "Sexagesimal");
            ui.selectable_value(&mut s.sexagesimal, false, "Decimal");
            ui.separator();
            ui.menu_button("Columns ▾", |ui| {
                for group in Col::GROUPS {
                    ui.label(egui::RichText::new(group).strong());
                    for c in Col::ALL.into_iter().filter(|c| c.group() == group) {
                        let mut on = s.columns.contains(&c);
                        if ui
                            .checkbox(&mut on, c.header())
                            .on_hover_text(c.description())
                            .changed()
                        {
                            if on {
                                s.columns.push(c);
                            } else {
                                s.columns.retain(|x| *x != c);
                            }
                        }
                    }
                }
            });
        });
        let rows = self.job.get().cloned();
        ui.horizontal_wrapped(|ui| {
            let enabled = rows.is_some();
            let style = AngleStyle::from_sexagesimal(s.sexagesimal);
            let cols = s.visible_columns();
            for (label, sep) in [("Copy CSV", ','), ("Copy TSV", '\t')] {
                if ui
                    .add_enabled(enabled, egui::Button::new(label))
                    .on_hover_text("Copy the whole table to the clipboard")
                    .clicked()
                    && let Some(rows) = &rows
                {
                    ui.ctx().copy_text(delimited(rows, &cols, style, tz, sep));
                    tc.status.notify(format!("Copied {} rows", rows.len()));
                }
            }
            if let Some(rows) = &rows {
                ui.weak(format!(
                    "{} rows · click a row to set the app time",
                    rows.len()
                ));
            }
        });
    }
}

/// The table as delimited text (CSV with `,`, TSV with a tab), with a
/// header row. CSV fields containing the separator or quotes are quoted.
pub fn delimited(
    rows: &[EphemerisRow],
    cols: &[Col],
    style: AngleStyle,
    tz: f64,
    sep: char,
) -> String {
    let quote = |s: String| -> String {
        if sep == ',' && (s.contains(',') || s.contains('"')) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else if sep == '\t' {
            s.replace('\t', " ")
        } else {
            s
        }
    };
    let mut out = String::new();
    let mut header = vec![quote(format!("Date/time ({})", format::zone_label(tz)))];
    header.extend(cols.iter().map(|c| quote(c.header().to_string())));
    out.push_str(&header.join(&sep.to_string()));
    out.push('\n');
    for r in rows {
        let mut line = vec![quote(format::date_time_seconds(r.instant, tz))];
        line.extend(cols.iter().map(|c| quote(c.cell(r, style))));
        out.push_str(&line.join(&sep.to_string()));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::MeeusEphemeris;

    #[test]
    fn csv_and_tsv() {
        let eph = MeeusEphemeris::new();
        let opts = TableOptions {
            location: Some(mooncalc_core::GeoLocation::new(21.4225, 39.8262, 277.0)),
            ..TableOptions::default()
        };
        let start = Instant::from_calendar(2024, 4, 8, 0, 0, 0.0);
        let rows = ephemeris_table(&eph, start, 1.0 / 24.0, 3, &opts);
        let cols = Col::ALL.to_vec();
        let csv = delimited(&rows, &cols, AngleStyle::Sexagesimal, 3.0, ',');
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("Date/time (UTC+03:00),JD (UT),Sun RA"));
        assert!(lines[1].starts_with("2024-04-08 03:00:00,2460408.50000,"));
        let tsv = delimited(&rows, &cols, AngleStyle::Decimal, 0.0, '\t');
        for line in tsv.lines() {
            assert_eq!(line.split('\t').count(), cols.len() + 1);
        }
        // Every column renders something for a topocentric row.
        for c in Col::ALL {
            assert_ne!(c.cell(&rows[0], AngleStyle::Decimal), "—", "{c:?}");
        }
    }

    #[test]
    fn every_column_is_in_a_group() {
        for c in Col::ALL {
            assert!(Col::GROUPS.contains(&c.group()));
        }
        assert_eq!(TableSettings::default().visible_columns().len(), 10);
    }
}
