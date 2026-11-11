//! Persisted application settings.
//!
//! [`Settings`] holds everything that should survive a restart: the global
//! inputs (time, observer, criterion, crescent, ephemeris) and view
//! preferences. It is stored with eframe's storage under [`SETTINGS_KEY`].
//! Transient UI state (hover position, search results, running jobs) lives in
//! [`crate::app::UiState`] and the tab structs instead.
//!
//! Every struct is `#[serde(default)]`, so adding a field never invalidates
//! a saved file: missing fields take their defaults. Renaming or removing a
//! field silently resets it, so prefer adding.

use mooncalc_atlas::{LocalTime, Zone};
use mooncalc_core::{GeoLocation, Instant};
use serde::{Deserialize, Serialize};

use crate::backend::BackendChoice;
use crate::datetime;
use crate::tabs::Tab;
use crate::tabs::calendar::CalendarSettings;
use crate::tabs::ephemeris::EphemerisSettings;
use crate::tabs::map::MapSettings;
use crate::tabs::moon::MoonSettings;
use crate::tabs::sky::SkySettings;
use crate::tabs::trajectory::TrajectorySettings;

/// eframe storage key for [`Settings`].
pub const SETTINGS_KEY: &str = "mooncalc.settings";

/// All persisted settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub time: TimeSettings,
    pub location: LocationSettings,
    /// Id of the selected criterion (`Criterion::id`, e.g. "yallop").
    pub criterion_id: String,
    pub crescent: Crescent,
    pub backend: BackendChoice,
    /// Path to a DE440/DE440s `.bsp` file (used with [`BackendChoice::De440`]).
    pub de440_path: String,
    pub view: ViewSettings,
    /// Calendar-tab choices (selected Hijri calendar, month layout).
    pub calendar: CalendarSettings,
    /// Ephemeris-tab choices (sub-view, table columns and step).
    pub ephemeris_tab: EphemerisSettings,
    /// Moon-tab choices (orientation, overlays, crater).
    pub moon_tab: MoonSettings,
    /// Map-tab display options (resolution, projection, layers, export).
    pub map: MapSettings,
    /// Sky-tab view options (view, zoom, disc scale, layers, speed).
    pub sky_tab: SkySettings,
    /// Trajectory-tab options (live mode).
    pub trajectory_tab: TrajectorySettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            time: TimeSettings::default(),
            location: LocationSettings::default(),
            criterion_id: "yallop".into(),
            crescent: Crescent::Evening,
            backend: BackendChoice::Meeus,
            de440_path: String::new(),
            view: ViewSettings::default(),
            calendar: CalendarSettings::default(),
            ephemeris_tab: EphemerisSettings::default(),
            moon_tab: MoonSettings::default(),
            map: MapSettings::default(),
            sky_tab: SkySettings::default(),
            trajectory_tab: TrajectorySettings::default(),
        }
    }
}

impl Settings {
    /// The selected instant (UT).
    pub fn instant(&self) -> Instant {
        Instant::from_jd_ut(self.time.jd_ut)
    }

    /// The observer location.
    pub fn observer(&self) -> GeoLocation {
        GeoLocation::new(
            self.location.lat_deg,
            self.location.lon_deg,
            self.location.elevation_m,
        )
    }

    /// The inputs a world map depends on. Compare against the last computed
    /// request to decide whether to recompute (the observer location is not
    /// part of it: a world map covers every location).
    pub fn map_request(&self) -> MapRequest {
        MapRequest {
            jd_ut: self.time.jd_ut,
            criterion_id: self.criterion_id.clone(),
            crescent: self.crescent,
            backend: self.backend.effective(),
            de440_path: self.de440_path.clone(),
        }
    }

    /// Settings as loaded at startup: when [`TimeSettings::start_at_now`] is
    /// set, the time is reset to the current time.
    pub fn on_startup(mut self) -> Self {
        if self.time.start_at_now {
            self.time.set_now();
        }
        self
    }
}

/// Inputs that determine a world visibility map.
#[derive(Clone, Debug, PartialEq)]
pub struct MapRequest {
    pub jd_ut: f64,
    pub criterion_id: String,
    pub crescent: Crescent,
    pub backend: BackendChoice,
    pub de440_path: String,
}

/// How local times are derived.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TzMode {
    /// A fixed offset from UT, [`TimeSettings::tz_offset_hours`].
    #[default]
    Fixed,
    /// An IANA time zone, [`TimeSettings::tz_name`], whose rules (DST and
    /// historical changes) give the offset at each instant.
    Zone,
}

/// The selected date and time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeSettings {
    /// The selected moment, as a Julian Day in UT. This is canonical.
    pub jd_ut: f64,
    /// Fixed time-zone offset, in hours east of Greenwich (UTC+3 = 3.0),
    /// used in [`TzMode::Fixed`].
    pub tz_offset_hours: f64,
    /// Fixed offset or IANA zone. Settings saved before zones existed have
    /// no such field and keep their fixed offset.
    #[serde(default)]
    pub tz_mode: TzMode,
    /// IANA zone for [`TzMode::Zone`], e.g. `Europe/London`; choosing a city
    /// sets it. An unknown name falls back to the fixed offset.
    pub tz_name: String,
    /// Show and edit the date in local time (UT + offset) rather than UT.
    pub edit_local: bool,
    /// Reset to the current time at startup instead of restoring `jd_ut`.
    pub start_at_now: bool,
}

impl Default for TimeSettings {
    fn default() -> Self {
        Self {
            jd_ut: datetime::round_to_minute(datetime::now_jd()),
            tz_offset_hours: 0.0,
            tz_mode: TzMode::Fixed,
            tz_name: String::new(),
            edit_local: false,
            start_at_now: true,
        }
    }
}

impl TimeSettings {
    /// The time zone for local times: the IANA zone in [`TzMode::Zone`]
    /// (the fixed offset if its name is unknown), else the fixed offset.
    pub fn zone(&self) -> Zone {
        match self.tz_mode {
            TzMode::Zone => Zone::iana_or_fixed(Some(&self.tz_name), self.tz_offset_hours),
            TzMode::Fixed => Zone::fixed_hours(self.tz_offset_hours),
        }
    }

    /// Use the IANA zone `name`. The fixed offset becomes the zone's offset
    /// now, so switching back to a fixed offset starts from it.
    pub fn set_zone(&mut self, name: &str) {
        if let Ok(z) = Zone::iana(name) {
            self.tz_mode = TzMode::Zone;
            self.tz_name = z.iana_name().unwrap_or(name).to_owned();
            self.tz_offset_hours = z.offset_hours(self.jd_ut);
        }
    }

    /// Use a fixed offset, hours east.
    pub fn set_fixed(&mut self, hours: f64) {
        self.tz_mode = TzMode::Fixed;
        self.tz_offset_hours = hours;
    }

    /// The UTC offset at the selected instant, hours.
    pub fn offset_hours(&self) -> f64 {
        self.zone().offset_hours(self.jd_ut)
    }

    /// Julian Day in local time (UT + the zone's offset at the instant).
    pub fn jd_local(&self) -> f64 {
        self.zone().to_local(self.jd_ut)
    }

    /// The Julian Day the date/time input shows for `jd_ut`: local time
    /// when editing in local time, else UT.
    pub fn to_display(&self, jd_ut: f64) -> f64 {
        if self.edit_local {
            self.zone().to_local(jd_ut)
        } else {
            jd_ut
        }
    }

    /// The UT Julian Day of a value entered in the date/time input. A local
    /// time skipped by a DST change moves forward by the gap; a repeated
    /// one takes its first occurrence.
    pub fn from_display(&self, jd: f64) -> f64 {
        if self.edit_local {
            self.zone().to_ut(jd)
        } else {
            jd
        }
    }

    /// How the local time shown for the instant maps back to UT: whether it
    /// is one of two occurrences (clocks went back).
    pub fn local_resolution(&self) -> LocalTime {
        self.zone().resolve_local(self.jd_local())
    }

    /// Set to the current time, rounded to the minute.
    pub fn set_now(&mut self) {
        self.jd_ut = datetime::round_to_minute(datetime::now_jd());
    }
}

/// The observer location.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocationSettings {
    pub name: String,
    pub lat_deg: f64,
    /// East-positive.
    pub lon_deg: f64,
    pub elevation_m: f64,
}

impl Default for LocationSettings {
    /// Mecca (Masjid al-Haram).
    fn default() -> Self {
        Self {
            name: "Mecca".into(),
            lat_deg: 21.4225,
            lon_deg: 39.8262,
            elevation_m: 277.0,
        }
    }
}

/// Evening crescent (after the new Moon, seen after sunset) or morning
/// crescent (before the new Moon, seen before sunrise).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Crescent {
    #[default]
    Evening,
    Morning,
}

impl Crescent {
    pub fn label(self) -> &'static str {
        match self {
            Crescent::Evening => "Evening (after sunset)",
            Crescent::Morning => "Morning (before sunrise)",
        }
    }
}

/// Colour theme preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemeChoice {
    Light,
    Dark,
    #[default]
    System,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::Light, ThemeChoice::Dark, ThemeChoice::System];

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
            ThemeChoice::System => "Follow system",
        }
    }

    pub fn to_egui(self) -> egui::ThemePreference {
        match self {
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
            ThemeChoice::System => egui::ThemePreference::System,
        }
    }
}

/// View preferences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewSettings {
    pub theme: ThemeChoice,
    pub show_inputs_panel: bool,
    pub show_status_bar: bool,
    pub active_tab: Tab,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            show_inputs_panel: true,
            show_status_bar: true,
            active_tab: Tab::Map,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Settings {
        Settings {
            time: TimeSettings {
                jd_ut: 2_461_000.25,
                tz_offset_hours: 5.5,
                tz_mode: TzMode::Fixed,
                tz_name: String::new(),
                edit_local: true,
                start_at_now: false,
            },
            location: LocationSettings {
                name: "Cape Town".into(),
                lat_deg: -33.9249,
                lon_deg: 18.4241,
                elevation_m: 25.0,
            },
            criterion_id: "odeh".into(),
            crescent: Crescent::Morning,
            backend: BackendChoice::De440,
            de440_path: "C:/data/de440s.bsp".into(),
            view: ViewSettings {
                theme: ThemeChoice::Dark,
                show_inputs_panel: false,
                show_status_bar: true,
                active_tab: Tab::Calendar,
            },
            calendar: CalendarSettings {
                calendar_id: "tabular-2-civil".into(),
                ..CalendarSettings::default()
            },
            ephemeris_tab: EphemerisSettings::default(),
            moon_tab: MoonSettings::default(),
            map: MapSettings {
                cities: true,
                ..MapSettings::default()
            },
            sky_tab: SkySettings {
                view: crate::tabs::sky::SkyView::StarChart,
                span_deg: 45.0,
                show_stars: false,
                ..SkySettings::default()
            },
            trajectory_tab: TrajectorySettings { live: true },
        }
    }

    #[test]
    fn settings_round_trip_through_serde() {
        let s = sample();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let back: Settings =
            serde_json::from_str(r#"{"criterion_id":"shaukat","view":{"theme":"Light"}}"#).unwrap();
        assert_eq!(back.criterion_id, "shaukat");
        assert_eq!(back.view.theme, ThemeChoice::Light);
        assert_eq!(back.view.active_tab, Tab::Map);
        assert_eq!(back.location, LocationSettings::default());
        assert!(back.time.start_at_now);
        // Tab options added later load with their defaults.
        assert_eq!(back.sky_tab, SkySettings::default());
        assert_eq!(back.trajectory_tab, TrajectorySettings::default());
        assert_eq!(back.ephemeris_tab, EphemerisSettings::default());
    }

    #[test]
    fn startup_resets_time_only_when_requested() {
        let s = sample();
        assert_eq!(s.clone().on_startup().time.jd_ut, s.time.jd_ut);
        let mut t = sample();
        t.time.start_at_now = true;
        assert!(t.on_startup().time.jd_ut > s.time.jd_ut);
    }

    #[test]
    fn map_request_ignores_the_observer() {
        let a = sample();
        let mut b = a.clone();
        b.location.lat_deg = 0.0;
        assert_eq!(a.map_request(), b.map_request());
        b.time.jd_ut += 1.0;
        assert_ne!(a.map_request(), b.map_request());
    }

    #[test]
    fn local_time_applies_the_offset() {
        let s = sample();
        assert!((s.time.jd_local() - (s.time.jd_ut + 5.5 / 24.0)).abs() < 1e-12);
        let shown = s.time.to_display(s.time.jd_ut);
        assert!((shown - (s.time.jd_ut + 5.5 / 24.0)).abs() < 1e-12);
        assert!((s.time.from_display(shown) - s.time.jd_ut).abs() < 1e-12);
    }

    #[test]
    fn zone_mode_follows_dst() {
        let mut t = TimeSettings {
            // 2024-03-30 12:00 UT, the day before the BST change.
            jd_ut: 2_460_400.0,
            edit_local: true,
            ..TimeSettings::default()
        };
        t.set_zone("Europe/London");
        assert_eq!(t.tz_mode, TzMode::Zone);
        assert_eq!(t.offset_hours(), 0.0);
        t.jd_ut += 1.0;
        assert_eq!(t.offset_hours(), 1.0);
        assert!((t.jd_local() - (t.jd_ut + 1.0 / 24.0)).abs() < 1e-12);
        // The fixed mode keeps its own offset.
        t.set_fixed(3.0);
        assert_eq!(t.offset_hours(), 3.0);
        // An unknown zone name is ignored.
        t.set_zone("Nowhere/Land");
        assert_eq!(t.tz_mode, TzMode::Fixed);
    }

    #[test]
    fn settings_without_a_zone_keep_their_fixed_offset() {
        // A file saved before zones existed: no tz_mode or tz_name.
        let json = r#"{"time":{"jd_ut":2460400.0,"tz_offset_hours":-5.0}}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.time.tz_mode, TzMode::Fixed);
        assert_eq!(s.time.offset_hours(), -5.0);
        let mut z = s.clone();
        z.time.set_zone("America/New_York");
        let back: Settings = serde_json::from_str(&serde_json::to_string(&z).unwrap()).unwrap();
        assert_eq!(back.time.zone().iana_name(), Some("America/New_York"));
    }
}
