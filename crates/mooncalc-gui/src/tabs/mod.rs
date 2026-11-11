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
        self.report.poll(ctx, tc);
        self.calendar.poll(ctx, tc);
        self.sky.poll(ctx, tc);
        self.trajectory.poll(ctx, tc);
        // The Moon tab computes in `ui` (one ephemeris row and a raster).
    }

    /// Carry out a File-menu action that needs a tab's data.
    pub fn handle_menu(
        &mut self,
        action: crate::panels::menu::MenuAction,
        ctx: &egui::Context,
        tc: &mut TabContext<'_>,
    ) {
        use crate::panels::menu::MenuAction;
        match action {
            MenuAction::ExportMap(format) => {
                tc.settings.view.active_tab = Tab::Map;
                self.map.open_export(format);
            }
            MenuAction::CopyReport => match self.report.text() {
                Some(text) => {
                    ctx.copy_text(text);
                    tc.status.notify("Report copied to the clipboard");
                }
                None => tc.status.error("The report has not been computed yet"),
            },
        }
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
