//! The central-area tabs.
//!
//! Each tab is a module with a state struct and two entry points:
//!
//! * `poll(&mut self, ctx: &egui::Context, tc: &mut TabContext)` runs every
//!   frame for **every** tab, visible or not: pick up background job results
//!   and start new jobs when inputs change (see [`crate::jobs`]). Report
//!   running jobs to the status bar with `tc.status.report_job`.
//! * `ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext)` draws the tab
//!   when it is active.
//!
//! [`TabContext`] gives access to the persisted [`Settings`] (the global
//! inputs: read them, or change them, e.g. a map click sets the observer)
//! and to the status bar. Tab-specific state that should persist belongs in
//! a new field of [`Settings`]; transient state belongs in the tab struct.
//!
//! To add a tab: add a variant to [`Tab`] (and to [`Tab::ALL`]), a module
//! here, a field to [`Tabs`], and arms in [`Tabs::poll`] / [`Tabs::ui`].

use serde::{Deserialize, Serialize};

use crate::settings::Settings;
use crate::status::StatusInfo;

pub mod about;
pub mod calendar;
pub mod ephemeris;
pub mod map;
pub mod moon;
pub mod report;
pub mod sky;
pub mod trajectory;

/// The tabs of the central area, in display order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    #[default]
    Map,
    Report,
    Ephemeris,
    Calendar,
    Sky,
    Moon,
    Trajectory,
    About,
}

impl Tab {
    pub const ALL: [Tab; 8] = [
        Tab::Map,
        Tab::Report,
        Tab::Ephemeris,
        Tab::Calendar,
        Tab::Sky,
        Tab::Moon,
        Tab::Trajectory,
        Tab::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Map => "Map",
            Tab::Report => "Report",
            Tab::Ephemeris => "Ephemeris",
            Tab::Calendar => "Calendar",
            Tab::Sky => "Sky",
            Tab::Moon => "Moon",
            Tab::Trajectory => "Trajectory",
            Tab::About => "About",
        }
    }
}

/// What a tab may read and change.
pub struct TabContext<'a> {
    /// The persisted settings, including the global inputs.
    pub settings: &'a mut Settings,
    /// The status bar.
    pub status: &'a mut StatusInfo,
}

/// The state of every tab.
#[derive(Default)]
pub struct Tabs {
    pub map: map::MapTab,
    pub report: report::ReportTab,
    pub ephemeris: ephemeris::EphemerisTab,
    pub calendar: calendar::CalendarTab,
    pub sky: sky::SkyTab,
    pub moon: moon::MoonTab,
    pub trajectory: trajectory::TrajectoryTab,
    pub about: about::AboutTab,
}

impl Tabs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run every tab's background bookkeeping.
    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>) {
        self.map.poll(ctx, tc);
        self.ephemeris.poll(ctx, tc);
        // Placeholder tabs have no background work yet; give them a `poll`
        // when they gain jobs.
    }

    /// Draw the active tab.
    pub fn ui(&mut self, tab: Tab, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        match tab {
            Tab::Map => self.map.ui(ui, tc),
            Tab::Report => self.report.ui(ui, tc),
            Tab::Ephemeris => self.ephemeris.ui(ui, tc),
            Tab::Calendar => self.calendar.ui(ui, tc),
            Tab::Sky => self.sky.ui(ui, tc),
            Tab::Moon => self.moon.ui(ui, tc),
            Tab::Trajectory => self.trajectory.ui(ui, tc),
            Tab::About => self.about.ui(ui, tc),
        }
    }
}

/// Body of a not-yet-implemented tab: a heading, what is coming, and where
/// the implementation plugs in.
pub(crate) fn placeholder(ui: &mut egui::Ui, title: &str, coming: &[&str], plug_in: &str) {
    ui.heading(title);
    ui.add_space(4.0);
    ui.label("Coming soon:");
    for item in coming {
        ui.label(format!("  •  {item}"));
    }
    ui.add_space(8.0);
    ui.weak(format!("Implementation hook: {plug_in}"));
}

/// A summary of the global inputs, shown by placeholder tabs so they visibly
/// react to the side panel.
pub(crate) fn inputs_summary(ui: &mut egui::Ui, settings: &Settings) {
    use crate::datetime::{DateTimeFields, format_utc_offset};
    use crate::location::format_lat_lon;

    ui.add_space(8.0);
    egui::CollapsingHeader::new("Current inputs")
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("inputs_summary")
                .num_columns(2)
                .show(ui, |ui| {
                    let t = &settings.time;
                    ui.label("Date/time (UT)");
                    ui.monospace(DateTimeFields::from_jd(t.jd_ut).to_string());
                    ui.end_row();
                    ui.label(format!("Local ({})", format_utc_offset(t.tz_offset_hours)));
                    ui.monospace(DateTimeFields::from_jd(t.jd_local()).to_string());
                    ui.end_row();
                    let l = &settings.location;
                    ui.label("Observer");
                    ui.label(format!(
                        "{} — {}, {:.0} m",
                        l.name,
                        format_lat_lon(l.lat_deg, l.lon_deg),
                        l.elevation_m
                    ));
                    ui.end_row();
                    ui.label("Criterion");
                    ui.label(&settings.criterion_id);
                    ui.end_row();
                    ui.label("Crescent");
                    ui.label(settings.crescent.label());
                    ui.end_row();
                    ui.label("Ephemeris");
                    ui.label(settings.backend.effective().label());
                    ui.end_row();
                });
        });
}
