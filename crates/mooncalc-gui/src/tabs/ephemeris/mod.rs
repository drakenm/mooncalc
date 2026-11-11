//! **Ephemeris** tab: tables and event lists for the Sun and Moon.
//!
//! Sub-views, each computed on a background job keyed by its inputs
//! ([`cached::CachedJob`]) while it is visible:
//!
//! * [`table`]: configurable Sun/Moon ephemeris table
//!   (`mooncalc_core::tables`), copyable as CSV/TSV.
//! * [`riseset`]: a month of Sun/Moon rise, transit and set with azimuths,
//!   and twilight (`mooncalc_core::events`).
//! * [`phases`]: the principal lunar phases of a year with the Moon's age.
//! * [`apsides`]: perigee/apogee, node passages and maximum declinations.
//! * [`eclipses`]: solar and lunar eclipses of a range of years with local
//!   circumstances at the app location.
//! * [`seasons`]: equinoxes and solstices, and the equation of time.
//!
//! Times are shown in the app time zone (`Settings::time.zone()`), each
//! with the offset in force at that instant.
//! Clicking an event time sets the app date/time to it.

use mooncalc_atlas::Zone;
use std::sync::Arc;

use mooncalc_core::{Ephemeris, Instant};
use serde::{Deserialize, Serialize};

use crate::backend::BackendChoice;
use crate::datetime::DateTimeFields;
use crate::settings::Settings;

use super::{Tab, TabContext};

pub mod apsides;
pub(crate) mod cached;
pub(crate) mod chart;
pub mod eclipses;
pub(crate) mod format;
pub mod phases;
pub mod riseset;
pub mod seasons;
pub mod table;

/// The ephemeris used by the Ephemeris and Moon tabs: the shared backend
/// ([`crate::backend::resolve`], cached, falling back to Meeus when DE440
/// is unavailable). Job requests include [`EphKey`], so changing the
/// backend recomputes.
pub(crate) fn ephemeris(settings: &Settings) -> Arc<dyn Ephemeris> {
    crate::backend::resolve(settings.backend, &settings.de440_path).eph
}

/// Look-back and look-ahead, days, of views that find the previous or next
/// new Moon or another monthly event (the Moon's age, lunations, apsides,
/// eclipses): a lunation and a day.
pub(crate) const LUNATION_MARGIN: f64 = 31.0;

/// Margin, days, of views that only interpolate or search within a day or
/// two of their span (rise/set, seasons).
pub(crate) const DAY_MARGIN: f64 = 2.0;

/// The coverage problem of the shared ephemeris for the UT span
/// `[start, end]` with the given look-back and look-ahead, days (see
/// [`crate::backend::ActiveEphemeris::span_problem`]). A view shows it
/// instead of computing.
pub(crate) fn coverage(
    settings: &Settings,
    start: f64,
    end: f64,
    before: f64,
    after: f64,
) -> Option<String> {
    crate::backend::resolve(settings.backend, &settings.de440_path)
        .span_problem(start, end, before, after)
}

/// The coverage problem for calendar year `year` (app time zone) with
/// `margin` days either side.
pub(crate) fn year_coverage(settings: &Settings, year: i32, margin: f64) -> Option<String> {
    let (a, b) = year_bounds(year, settings.time.zone());
    coverage(settings, a.jd_ut, b.jd_ut, margin, margin)
}

/// Identity of the ephemeris backend, part of every job request.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EphKey {
    backend: BackendChoice,
    de440_path: String,
}

impl EphKey {
    pub(crate) fn of(settings: &Settings) -> Self {
        Self {
            backend: settings.backend.effective(),
            de440_path: settings.de440_path.clone(),
        }
    }
}

/// The sub-views of the tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EphemerisView {
    #[default]
    Table,
    RiseSet,
    Phases,
    ApsidesNodes,
    Eclipses,
    Seasons,
}

impl EphemerisView {
    pub const ALL: [Self; 6] = [
        Self::Table,
        Self::RiseSet,
        Self::Phases,
        Self::ApsidesNodes,
        Self::Eclipses,
        Self::Seasons,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Table => "Table",
            Self::RiseSet => "Rise/set",
            Self::Phases => "Lunar phases",
            Self::ApsidesNodes => "Apsides & nodes",
            Self::Eclipses => "Eclipses",
            Self::Seasons => "Seasons",
        }
    }
}

/// Persisted choices of the Ephemeris tab.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EphemerisSettings {
    pub view: EphemerisView,
    /// Table columns, step, row count, start and angle format.
    pub table: table::TableSettings,
    /// Eclipse list span and kinds.
    pub eclipses: eclipses::EclipseSettings,
}

/// State of the Ephemeris tab.
#[derive(Default)]
pub struct EphemerisTab {
    table: table::TableView,
    riseset: riseset::RiseSetView,
    phases: phases::PhasesView,
    apsides: apsides::ApsidesView,
    eclipses: eclipses::EclipsesView,
    seasons: seasons::SeasonsView,
}

impl EphemerisTab {
    /// Start jobs for the visible sub-view; collect finished results.
    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>) {
        let visible = tc.settings.view.active_tab == Tab::Ephemeris;
        let view = tc.settings.ephemeris_tab.view;
        let on = |v: EphemerisView| visible && view == v;
        self.table.poll(ctx, tc, on(EphemerisView::Table));
        self.riseset.poll(ctx, tc, on(EphemerisView::RiseSet));
        self.phases.poll(ctx, tc, on(EphemerisView::Phases));
        self.apsides.poll(ctx, tc, on(EphemerisView::ApsidesNodes));
        self.eclipses.poll(ctx, tc, on(EphemerisView::Eclipses));
        self.seasons.poll(ctx, tc, on(EphemerisView::Seasons));
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        ui.horizontal_wrapped(|ui| {
            for v in EphemerisView::ALL {
                ui.selectable_value(&mut tc.settings.ephemeris_tab.view, v, v.title());
            }
            ui.separator();
            ui.weak(format!(
                "Times in {} · {}",
                format::zone_label(tc.settings.time.zone()),
                ephemeris(tc.settings).name()
            ));
        });
        ui.separator();
        match tc.settings.ephemeris_tab.view {
            EphemerisView::Table => self.table.ui(ui, tc),
            EphemerisView::RiseSet => self.riseset.ui(ui, tc),
            EphemerisView::Phases => self.phases.ui(ui, tc),
            EphemerisView::ApsidesNodes => self.apsides.ui(ui, tc),
            EphemerisView::Eclipses => self.eclipses.ui(ui, tc),
            EphemerisView::Seasons => self.seasons.ui(ui, tc),
        }
    }
}

/// Set the app date/time to `t` and say so in the status bar.
pub(crate) fn set_app_time(tc: &mut TabContext<'_>, t: Instant) {
    tc.settings.time.jd_ut = t.jd_ut;
    let tz = tc.settings.time.zone();
    tc.status.notify(format!(
        "Time set to {} {}",
        format::date_time(t, tz),
        format::zone_label(tz)
    ));
}

/// A clickable monospace time cell; returns `true` when clicked.
pub(crate) fn time_link(ui: &mut egui::Ui, text: impl Into<String>) -> bool {
    ui.add(
        egui::Label::new(egui::RichText::new(text.into()).monospace()).sense(egui::Sense::click()),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .on_hover_text("Click to set the app date/time to this instant")
    .clicked()
}

/// A clickable time cell for an optional instant (`—` when absent).
/// Returns the instant when clicked.
pub(crate) fn time_cell(
    ui: &mut egui::Ui,
    t: Option<Instant>,
    tz: Zone,
    with_date: bool,
) -> Option<Instant> {
    match t {
        Some(t) => {
            let text = if with_date {
                format::date_time(t, tz)
            } else {
                format::time_hm(t, tz)
            };
            time_link(ui, text).then_some(t)
        }
        None => {
            ui.weak("—");
            None
        }
    }
}

/// The year of the app date in the app time zone.
pub(crate) fn app_year(settings: &Settings) -> i32 {
    DateTimeFields::from_jd(settings.time.jd_local()).year
}

/// UT bounds of a calendar year in zone `tz` (local midnight to midnight).
pub(crate) fn year_bounds(year: i32, tz: Zone) -> (Instant, Instant) {
    tz.year_bounds(year)
}

/// A year selector bound to the app year until edited. Returns the year.
pub(crate) fn year_picker(ui: &mut egui::Ui, year: &mut Option<i32>, app_year: i32) -> i32 {
    let mut y = year.unwrap_or(app_year);
    ui.label("Year:");
    if ui
        .small_button("◀")
        .on_hover_text("Previous year")
        .clicked()
    {
        y -= 1;
    }
    ui.add(egui::DragValue::new(&mut y).range(-3000..=5000).speed(0.2));
    if ui.small_button("▶").on_hover_text("Next year").clicked() {
        y += 1;
    }
    if y != year.unwrap_or(app_year) {
        *year = Some(y);
    }
    if year.is_some()
        && ui
            .small_button("App year")
            .on_hover_text("Follow the year of the app date")
            .clicked()
    {
        *year = None;
        y = app_year;
    }
    y
}

/// A spinner and label while `running`.
pub(crate) fn busy(ui: &mut egui::Ui, running: bool) {
    if running {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Computing…");
        });
    }
}

/// An error line, if any.
pub(crate) fn error_line(ui: &mut egui::Ui, error: &Option<String>) {
    if let Some(e) = error {
        ui.colored_label(ui.visuals().error_fg_color, format!("Failed: {e}"));
    }
}
