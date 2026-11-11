//! Data-transfer objects: the wire contract of every MoonCalc frontend.
//!
//! These types are exchanged with the TypeScript UIs (Tauri `invoke` now,
//! `wasm-bindgen` later) and used directly by native frontends. They hold no
//! frontend types. The TypeScript mirror is
//! `frontend/tauri/src/backend/types.ts`; keep the two in step.
//!
//! # Wire conventions
//!
//! * JSON field names are `camelCase` (`#[serde(rename_all = "camelCase")]`).
//! * Unit suffixes follow the workspace convention: `lat_deg` becomes
//!   `latDeg`, `elevation_m` becomes `elevationM`. Longitude is east-positive.
//! * Date-times on the wire are **UT**. Time-zone offsets only affect which
//!   local date is meant, and display.
//! * Enums are lowercase (or kebab-case) strings.
//! * Every operation that takes input takes a single `params` object, so each
//!   transport can pass one value per call.
//! * Binary payloads (map cells, value fields, PNG files) are base64 strings
//!   (RFC 4648, standard alphabet, padded); see [`crate::codec`].

use mooncalc_core::time::Instant;
use serde::{Deserialize, Serialize};

/// Zone value that marks a map cell with no evaluation (no sunset or sunrise
/// that day, Moon below the horizon, or before conjunction).
pub const NO_DATA: u8 = 255;

/// Quantised value marking a cell without a criterion value, see
/// [`ValueFieldDto`].
pub const NO_VALUE: i16 = i16::MIN;

// ---------------------------------------------------------------------------
// Shared value types
// ---------------------------------------------------------------------------

/// An observer's position (WGS84 geodetic).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    /// Latitude, degrees, north-positive, `-90..=90`.
    pub lat_deg: f64,
    /// Longitude, degrees, **east-positive**, `-180..=180`.
    pub lon_deg: f64,
    /// Height above sea level, metres.
    pub elevation_m: f64,
}

impl From<Location> for mooncalc_core::GeoLocation {
    fn from(l: Location) -> Self {
        mooncalc_core::GeoLocation::new(l.lat_deg, l.lon_deg, l.elevation_m)
    }
}

/// A calendar date (Gregorian from 1582-10-15, Julian before; astronomical
/// year numbering).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CivilDate {
    pub year: i32,
    /// 1..=12.
    pub month: u32,
    /// 1..=31.
    pub day: u32,
}

impl From<CivilDate> for mooncalc_core::events::CalendarDate {
    fn from(d: CivilDate) -> Self {
        Self::new(d.year, d.month, d.day)
    }
}

impl From<mooncalc_core::events::CalendarDate> for CivilDate {
    fn from(d: mooncalc_core::events::CalendarDate) -> Self {
        Self {
            year: d.year,
            month: d.month,
            day: d.day,
        }
    }
}

/// A date and time in **UT**. Mirrors `mooncalc_core::CalendarDateTime`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UtDateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: f64,
}

impl UtDateTime {
    /// The calendar date part.
    pub fn date(&self) -> CivilDate {
        CivilDate {
            year: self.year,
            month: self.month,
            day: self.day,
        }
    }

    /// As an [`Instant`].
    pub fn to_instant(&self) -> Instant {
        Instant::from_calendar(
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
        )
    }
}

impl From<Instant> for UtDateTime {
    fn from(t: Instant) -> Self {
        let c = t.to_calendar();
        Self {
            year: c.year,
            month: c.month,
            day: c.day,
            hour: c.hour,
            minute: c.minute,
            second: c.second,
        }
    }
}

/// Which crescent to evaluate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Crescent {
    /// The young crescent after sunset.
    Evening,
    /// The old crescent before sunrise.
    Morning,
}

impl From<Crescent> for mooncalc_visibility::circumstances::Sighting {
    fn from(c: Crescent) -> Self {
        match c {
            Crescent::Evening => Self::Evening,
            Crescent::Morning => Self::Morning,
        }
    }
}

/// An error returned by any operation. Serialized as `{ code, message }`, and
/// surfaced to TypeScript as a rejected promise with that value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    /// Stable machine-readable code: `invalid-params`, `unknown-criterion`,
    /// `ephemeris-unavailable`, `cancelled` or `internal`.
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
}

impl ApiError {
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: "invalid-params".into(),
            message: message.into(),
        }
    }

    pub fn unknown_criterion(id: &str) -> Self {
        Self {
            code: "unknown-criterion".into(),
            message: format!("unknown criterion id {id:?}"),
        }
    }

    pub fn ephemeris_unavailable(message: impl Into<String>) -> Self {
        Self {
            code: "ephemeris-unavailable".into(),
            message: message.into(),
        }
    }

    pub fn cancelled() -> Self {
        Self {
            code: "cancelled".into(),
            message: "the computation was cancelled".into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal".into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

// ---------------------------------------------------------------------------
// app_info, ephemerides
// ---------------------------------------------------------------------------

/// Static information about the running application.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Product name, "MoonCalc".
    pub name: String,
    /// Application version (the workspace version).
    pub version: String,
    /// The hosting runtime, e.g. `"tauri 2.12.5"`, `"wasm"` or `"egui"`.
    pub runtime: String,
    /// Every ephemeris backend this build knows about.
    pub ephemeris_backends: Vec<EphemerisBackendInfo>,
    /// `id` of the backend used when a request names none.
    pub default_ephemeris: String,
    /// Credit lines for embedded data (GeoNames, Natural Earth).
    pub attributions: Vec<String>,
}

/// One ephemeris backend and its state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EphemerisBackendInfo {
    /// Stable identifier: `"meeus"`, `"meeus-full"` or `"de440"`.
    pub id: String,
    /// Display name.
    pub name: String,
    /// One-line description, including any data-file requirement.
    pub description: String,
    /// Whether this build can use it (DE440 needs the `de440` feature).
    pub available: bool,
    /// Whether it needs a data file before use.
    pub needs_file: bool,
    /// Whether it is ready (always for Meeus; DE440 once a file is loaded).
    pub ready: bool,
    /// The loaded file, for file-backed ephemerides.
    pub file: Option<String>,
    /// The backend's own description of itself once ready (e.g. the SPK
    /// segments), if any.
    pub detail: Option<String>,
}

/// Input of `load_de440`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadDe440Params {
    /// Path of a DE440 or DE440s `.bsp` file (native frontends only).
    pub path: String,
}

// ---------------------------------------------------------------------------
// criteria_list
// ---------------------------------------------------------------------------

/// One legend entry: a criterion zone, most visible first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneLegendEntry {
    /// Short code, e.g. `"A"`.
    pub code: String,
    /// Description, e.g. "Easily visible to the naked eye".
    pub description: String,
    /// Map colour, sRGB.
    pub rgb: [u8; 3],
}

impl From<&mooncalc_visibility::ZoneInfo> for ZoneLegendEntry {
    fn from(z: &mooncalc_visibility::ZoneInfo) -> Self {
        Self {
            code: z.code.into(),
            description: z.description.into(),
            rgb: z.rgb,
        }
    }
}

/// A sighting criterion and its legend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CriterionInfo {
    /// Stable identifier, e.g. `"yallop"`.
    pub id: String,
    /// Display name, e.g. "Yallop (1997)".
    pub name: String,
    /// Bibliographic reference.
    pub reference: String,
    /// When the criterion is evaluated, e.g. "Best time (sunset + 4/9 lag)".
    pub evaluation_time: String,
    /// Descending value thresholds that reproduce the zones (for smooth
    /// contouring of the value field), when the zones are thresholds of one
    /// value: zone `i` is `value >= thresholds[i]`, the last zone anything
    /// lower.
    pub thresholds: Option<Vec<f32>>,
    /// Legend, most visible first. A map cell's zone value indexes this list.
    pub zones: Vec<ZoneLegendEntry>,
}

// ---------------------------------------------------------------------------
// compute_visibility_map
// ---------------------------------------------------------------------------

/// Latitude/longitude bounds of a map, degrees.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub lat_min_deg: f64,
    pub lat_max_deg: f64,
    pub lon_min_deg: f64,
    pub lon_max_deg: f64,
}

impl Bounds {
    /// The whole world, -90..90 by -180..180.
    pub const WORLD: Self = Self {
        lat_min_deg: -90.0,
        lat_max_deg: 90.0,
        lon_min_deg: -180.0,
        lon_max_deg: 180.0,
    };
}

/// Input of `compute_visibility_map`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapParams {
    /// Local date of the evening (or morning) the map is for. Each cell is
    /// evaluated on its own local evening of this date (the 180° meridian is
    /// the date line).
    pub date: CivilDate,
    /// `CriterionInfo::id`.
    pub criterion_id: String,
    pub crescent: Crescent,
    /// Grid spacing in degrees. 0.25 gives the full 1440 × 720 world grid.
    /// Allowed range `0.1..=10`.
    pub resolution_deg: f64,
    /// Map bounds; `None` is the whole world.
    #[serde(default)]
    pub bounds: Option<Bounds>,
    /// `EphemerisBackendInfo::id`; `None` selects `AppInfo::default_ephemeris`.
    #[serde(default)]
    pub ephemeris_id: Option<String>,
    /// Whether to send the continuous value field (default true).
    #[serde(default = "default_true")]
    pub include_values: bool,
}

fn default_true() -> bool {
    true
}

/// Geometry of a map grid. Matches `mooncalc_maps::grid::VisibilityGrid`:
/// row 0 is the northernmost row, column 0 the westernmost; cell centres sit
/// half a step in from the bounds. Equirectangular, so a cell's latitude and
/// longitude are linear in its row and column.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridSpec {
    pub lat_min_deg: f64,
    pub lat_max_deg: f64,
    pub lon_min_deg: f64,
    pub lon_max_deg: f64,
    pub rows: u32,
    pub cols: u32,
}

/// The criterion's continuous value per cell (Yallop q, Odeh V, a margin
/// ...), quantised to 16 bits.
///
/// `data` is base64 of `rows × cols` little-endian `i16`, same layout as
/// [`MapResult::cells`]. A cell's value is `offset + scale × q`, or none when
/// `q` is [`NO_VALUE`] (-32768). The scale spreads the map's own value range
/// over the 16-bit range, so the quantisation step is the range / 65 000
/// (about 10⁻⁴ for Yallop's q).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueFieldDto {
    pub scale: f64,
    pub offset: f64,
    /// The quantised value marking cells without a value (always -32768).
    pub no_value: i16,
    pub data: String,
}

/// Cells per circumstance status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCounts {
    /// A crescent to look for (the criterion was applied).
    pub crescent: u32,
    /// The best time precedes conjunction (evening) / follows it (morning).
    pub before_conjunction: u32,
    /// The Moon sets before the Sun (rises after it in the morning).
    pub moon_sets_first: u32,
    pub moon_always_up: u32,
    pub moon_always_down: u32,
    pub midnight_sun: u32,
    pub polar_night: u32,
    pub sun_altitude_not_reached: u32,
}

/// The cell with the highest criterion value (or the first cell in the most
/// visible zone when the criterion has no value).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BestCell {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub zone: u8,
    pub value: Option<f64>,
}

/// Where the time went, milliseconds. Filled in by the caller (the services
/// are clock-free so they stay portable).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapTiming {
    /// Ephemeris table and conjunctions.
    pub prepare_ms: f64,
    /// Cell evaluation (wall clock over all threads).
    pub evaluate_ms: f64,
    /// Assembling and encoding the result.
    pub encode_ms: f64,
    pub total_ms: f64,
    /// Worker threads used for the cells.
    pub threads: u32,
}

/// A computed visibility map.
///
/// # Payload format
///
/// The map travels as **raw zone indices plus a legend**, not as an image.
/// `cells` holds `rows × cols` bytes, row-major from the north-west corner.
/// Each byte indexes `legend`, or is [`NO_DATA`] (255). The optional
/// [`ValueFieldDto`] carries the criterion's value per cell so the client can
/// draw smooth contours (classify the bilinearly interpolated value with
/// `thresholds`) and show the value under the cursor.
///
/// Why not a PNG: one byte per cell is already smaller than PNG's raw RGBA
/// input (a 1440 × 720 grid is about 1.04 MB of indices, 1.4 MB as base64),
/// costs no encode or decode beyond base64, and leaves colouring to the
/// client, which can then show the zone under the cursor, highlight or hide a
/// zone, or switch palettes without asking the backend again. Use
/// `export_map_png` for a finished image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapResult {
    pub criterion_id: String,
    pub criterion_name: String,
    pub date: CivilDate,
    pub crescent: Crescent,
    pub grid: GridSpec,
    /// Base64 of `rows × cols` zone bytes; see the type docs.
    pub cells: String,
    /// The value marking cells without an evaluation (always [`NO_DATA`]).
    pub no_data: u8,
    /// The criterion's legend; zone byte `i` is `legend[i]`.
    pub legend: Vec<ZoneLegendEntry>,
    /// The value field, if requested.
    pub values: Option<ValueFieldDto>,
    /// See [`CriterionInfo::thresholds`].
    pub thresholds: Option<Vec<f32>>,
    /// Cells per legend zone (same order as `legend`).
    pub zone_counts: Vec<u32>,
    pub status_counts: StatusCounts,
    /// The geocentric new Moon nearest to the date (the one the crescent
    /// belongs to).
    pub conjunction: UtDateTime,
    /// The rule for each cell's evaluation instant, e.g. "Best time
    /// (sunset + 4/9 lag)".
    pub evaluation_time: String,
    /// The best cell on the map, if any cell has a crescent.
    pub best: Option<BestCell>,
    /// The ephemeris used.
    pub ephemeris: String,
    pub timing: MapTiming,
}

/// Input of `export_map_png`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPngParams {
    pub map: MapParams,
    /// Image width in pixels (default 2048, range 256..=8192); the height
    /// follows from the map's aspect ratio.
    #[serde(default)]
    pub width_px: Option<u32>,
    /// Draw the legend (default true).
    #[serde(default = "default_true")]
    pub legend: bool,
    /// Optional marker, e.g. the selected location.
    #[serde(default)]
    pub marker: Option<MapMarker>,
}

/// A labelled point on an exported map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapMarker {
    pub location: Location,
    pub label: Option<String>,
}

/// An encoded PNG image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PngImage {
    pub width: u32,
    pub height: u32,
    /// Base64 of the PNG file.
    pub png: String,
    /// A suggested file name, e.g. `mooncalc-yallop-2024-03-10-evening.png`.
    pub file_name: String,
}

// ---------------------------------------------------------------------------
// location_report
// ---------------------------------------------------------------------------

/// Input of `location_report`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationReportParams {
    /// The local calendar date (in the `tz_offset_hours` zone) whose evening
    /// and morning are reported.
    pub date: CivilDate,
    /// Offset of local civil time from UT, hours. Civil noon of `date`
    /// anchors the day.
    pub tz_offset_hours: f64,
    pub location: Location,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
}

/// Rise, set and transit times of the Sun and Moon on the local date.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayEventsDto {
    pub sunrise: Option<UtDateTime>,
    pub sunset: Option<UtDateTime>,
    pub solar_noon: Option<UtDateTime>,
    pub moonrise: Option<UtDateTime>,
    pub moonset: Option<UtDateTime>,
    pub moon_transit: Option<UtDateTime>,
    /// Civil twilight (Sun 6° down) begins / ends.
    pub civil_dawn: Option<UtDateTime>,
    pub civil_dusk: Option<UtDateTime>,
    /// Nautical twilight (Sun 12° down) begins / ends.
    pub nautical_dawn: Option<UtDateTime>,
    pub nautical_dusk: Option<UtDateTime>,
}

/// Wire form of `mooncalc_visibility::CrescentGeometry` (degrees, as there).
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrescentGeometryDto {
    pub arcl_deg: f64,
    pub arcl_geo_deg: f64,
    pub arcv_deg: f64,
    pub arcv_topo_deg: f64,
    pub daz_deg: f64,
    pub width_topo_arcmin: f64,
    pub width_geo_arcmin: f64,
    pub semi_diameter_topo_arcmin: f64,
    pub moon_alt_deg: f64,
    pub moon_alt_airless_deg: f64,
    pub sun_alt_deg: f64,
    pub moon_az_deg: f64,
    pub sun_az_deg: f64,
    /// `null` when the Moon does not set (rise) near the Sun event.
    pub lag_minutes: Option<f64>,
    pub age_hours: f64,
    pub illuminated_fraction: f64,
    pub moon_distance_km: f64,
}

impl From<&mooncalc_visibility::CrescentGeometry> for CrescentGeometryDto {
    fn from(g: &mooncalc_visibility::CrescentGeometry) -> Self {
        Self {
            arcl_deg: g.arcl_deg,
            arcl_geo_deg: g.arcl_geo_deg,
            arcv_deg: g.arcv_deg,
            arcv_topo_deg: g.arcv_topo_deg,
            daz_deg: g.daz_deg,
            width_topo_arcmin: g.width_topo_arcmin,
            width_geo_arcmin: g.width_geo_arcmin,
            semi_diameter_topo_arcmin: g.semi_diameter_topo_arcmin,
            moon_alt_deg: g.moon_alt_deg,
            moon_alt_airless_deg: g.moon_alt_airless_deg,
            sun_alt_deg: g.sun_alt_deg,
            moon_az_deg: g.moon_az_deg,
            sun_az_deg: g.sun_az_deg,
            lag_minutes: g.lag_minutes.is_finite().then_some(g.lag_minutes),
            age_hours: g.age_hours,
            illuminated_fraction: g.illuminated_fraction,
            moon_distance_km: g.moon_distance_km,
        }
    }
}

/// A criterion's classification of one observation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    /// Index into the criterion's zones.
    pub zone: u8,
    pub code: String,
    pub description: String,
    pub rgb: [u8; 3],
    /// The criterion's figure of merit (Yallop q, Odeh V, ...), if any.
    pub value: Option<f64>,
}

/// One criterion's result in a report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CriterionVerdict {
    pub criterion_id: String,
    pub name: String,
    /// The criterion's evaluation-time rule.
    pub evaluation_time: String,
    /// The instant it was evaluated at, if circumstances exist.
    pub evaluated_at: Option<UtDateTime>,
    /// The circumstance status at that instant (kebab-case, as
    /// [`CrescentReport::status`]).
    pub status: String,
    /// The classification; `None` when there is no crescent to look for (maps
    /// show such places as "not visible").
    pub verdict: Option<Verdict>,
}

/// The evening or morning crescent at one place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrescentReport {
    pub crescent: Crescent,
    /// `crescent`, `before-conjunction`, `moon-sets-first`,
    /// `moon-always-up`, `moon-always-down`, `midnight-sun`, `polar-night`
    /// or `sun-altitude-not-reached`.
    pub status: String,
    /// A sentence explaining `status`.
    pub status_text: String,
    /// Sunset (evening) or sunrise (morning).
    pub sun_event: Option<UtDateTime>,
    /// Moonset (evening) or moonrise (morning) nearest the Sun event.
    pub moon_event: Option<UtDateTime>,
    /// Yallop's best time, sunset + 4/9 lag (sunrise − 4/9 lag).
    pub best_time: Option<UtDateTime>,
    /// The geocentric new Moon nearest the best time.
    pub conjunction: Option<UtDateTime>,
    /// Circumstances at the best time (at the Sun event when the Moon sets
    /// first).
    pub geometry: Option<CrescentGeometryDto>,
    /// Every criterion, in display order, each at its own evaluation time.
    pub criteria: Vec<CriterionVerdict>,
}

/// Crescent visibility at one place on one local date.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationReport {
    pub location: Location,
    pub date: CivilDate,
    pub tz_offset_hours: f64,
    pub ephemeris: String,
    pub day: DayEventsDto,
    /// The Moon's age at local civil noon (from the nearest new Moon), hours.
    pub moon_age_at_noon_hours: f64,
    /// The Moon's illuminated fraction at local civil noon, 0..1.
    pub illumination_at_noon: f64,
    pub evening: CrescentReport,
    pub morning: CrescentReport,
}

// ---------------------------------------------------------------------------
// atlas
// ---------------------------------------------------------------------------

/// Input of `search_cities`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitySearchParams {
    /// Free text, case- and diacritic-insensitive; `City, Country` narrows.
    pub query: String,
    /// Maximum number of results (default 20, capped at 100).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Input of `nearest_city`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NearestCityParams {
    pub lat_deg: f64,
    pub lon_deg: f64,
}

/// A city from the atlas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct City {
    pub name: String,
    /// First-level administrative division (state, province), if known.
    pub admin1: Option<String>,
    /// Country name.
    pub country: String,
    /// ISO 3166-1 alpha-2 code.
    pub country_code: String,
    pub location: Location,
    /// IANA time-zone name, e.g. `"Asia/Riyadh"`, if known.
    pub time_zone: Option<String>,
    /// Standard-time offset from UT, hours.
    pub std_offset_hours: f64,
    /// Daylight-saving offset from UT, hours (equal to the standard offset
    /// where DST is not observed).
    pub dst_offset_hours: f64,
    pub population: Option<u64>,
}

/// The city nearest a point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NearestCity {
    pub city: City,
    pub distance_km: f64,
}

// ---------------------------------------------------------------------------
// earliest_sighting
// ---------------------------------------------------------------------------

/// Input of `earliest_sighting`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EarliestParams {
    /// Any date near the new Moon of interest; the conjunction nearest to
    /// noon UT of this date is scanned.
    pub date: CivilDate,
    pub criterion_id: String,
    /// Grid spacing in degrees, `0.25..=10` (default 2).
    #[serde(default)]
    pub resolution_deg: Option<f64>,
    /// Evenings to scan, starting the day before the conjunction (default 4,
    /// range 1..=10).
    #[serde(default)]
    pub evenings: Option<u32>,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
}

/// Where and when a zone was first reached.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstSightingDto {
    pub date: CivilDate,
    /// The best cell among those reaching the zone or better.
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub value: Option<f64>,
    /// How many cells reach the zone or better on that date.
    pub cells: u32,
    /// The atlas city nearest the best cell, as `Name, Country`, followed
    /// by `, <distance> km` when it is more than 25 km away.
    pub nearest_city: Option<String>,
}

/// One zone of the earliest-sighting result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EarliestZone {
    pub zone: u8,
    pub code: String,
    pub description: String,
    pub rgb: [u8; 3],
    /// `None` if no scanned evening reaches the zone.
    pub first: Option<FirstSightingDto>,
}

/// Result of `earliest_sighting`: for every zone except the last ("not
/// visible"), the first evening any cell reaches that zone or better.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EarliestResult {
    pub criterion_id: String,
    pub criterion_name: String,
    pub conjunction: UtDateTime,
    /// The evenings actually evaluated (the scan stops once every zone is
    /// reached).
    pub dates_scanned: Vec<CivilDate>,
    pub zones: Vec<EarliestZone>,
    pub resolution_deg: f64,
    pub timing: MapTiming,
}

// ---------------------------------------------------------------------------
// basemap
// ---------------------------------------------------------------------------

/// Natural Earth scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BasemapScale {
    /// 1:110m.
    #[default]
    #[serde(rename = "110m")]
    Ne110m,
    /// 1:50m.
    #[serde(rename = "50m")]
    Ne50m,
}

/// Input of `basemap`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BasemapParams {
    #[serde(default)]
    pub scale: BasemapScale,
    /// Douglas–Peucker tolerance, degrees (default 0.02 at 110m, 0.01 at
    /// 50m; 0 keeps every point).
    #[serde(default)]
    pub tolerance_deg: Option<f64>,
}

/// Polylines as flat coordinate arrays: each line is
/// `[lon0, lat0, lon1, lat1, ...]` in degrees, rounded to 0.001°.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolylineSet {
    pub lines: Vec<Vec<f32>>,
    /// Total points over all lines.
    pub points: u32,
}

/// Coastlines and country borders for drawing over the zone layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Basemap {
    pub scale: BasemapScale,
    pub coastlines: PolylineSet,
    pub borders: PolylineSet,
    pub attribution: String,
}
