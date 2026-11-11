//! Data-transfer objects exchanged with the TypeScript UI.
//!
//! These types are the wire contract of every MoonCalc frontend transport:
//! Tauri `invoke` today, `wasm-bindgen` later. They are deliberately free of
//! Tauri types so they can move unchanged into the planned `mooncalc-app`
//! services crate (see `docs/architecture.md`), which the egui GUI will share.
//!
//! The TypeScript mirror is `frontend/tauri/src/backend/types.ts`; keep the two
//! in step.
//!
//! # Wire conventions
//!
//! * JSON field names are `camelCase` (`#[serde(rename_all = "camelCase")]`).
//! * Unit suffixes follow the workspace convention: `lat_deg` becomes
//!   `latDeg`, `elevation_m` becomes `elevationM`. Longitude is east-positive.
//! * Date-times on the wire are **UT**. Time-zone offsets only affect display.
//! * Enums are lowercase strings.
//! * Every command that takes input takes a single `params` object, so each
//!   transport can pass one value per call.

use serde::{Deserialize, Serialize};

/// Zone value that marks a map cell with no evaluation (no sunset or sunrise
/// that day, Moon below the horizon, or before conjunction).
pub const NO_DATA: u8 = 255;

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
/// year numbering), as used for "the evening of" a map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CivilDate {
    pub year: i32,
    /// 1..=12.
    pub month: u32,
    /// 1..=31.
    pub day: u32,
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
}

/// Which crescent to evaluate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Crescent {
    /// The young crescent after sunset.
    Evening,
    /// The old crescent before sunrise.
    Morning,
}

/// An error returned by any command. Serialized as `{ code, message }`, and
/// surfaced to TypeScript as a rejected promise with that value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    /// Stable machine-readable code, e.g. `"invalid-params"`.
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
// app_info
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
}

/// One ephemeris backend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EphemerisBackendInfo {
    /// Stable identifier: `"meeus"` or `"de440"`.
    pub id: String,
    /// Display name.
    pub name: String,
    /// One-line description, including any data-file requirement.
    pub description: String,
    /// Whether this build can use it (DE440 needs the `de440` feature).
    pub available: bool,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CriterionInfo {
    /// Stable identifier, e.g. `"yallop"`.
    pub id: String,
    /// Display name, e.g. "Yallop (1997)".
    pub name: String,
    /// Bibliographic reference.
    pub reference: String,
    /// Legend, most visible first. A map cell's zone value indexes this list.
    pub zones: Vec<ZoneLegendEntry>,
}

// ---------------------------------------------------------------------------
// compute_visibility_map
// ---------------------------------------------------------------------------

/// Input of `compute_visibility_map`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapParams {
    /// Local date of the evening (or morning) the map is for.
    pub date: CivilDate,
    /// `CriterionInfo::id`.
    pub criterion_id: String,
    pub crescent: Crescent,
    /// Grid spacing in degrees. 0.25 gives the full 1440 × 720 world grid.
    /// Allowed range `0.1..=10`.
    pub resolution_deg: f64,
    /// `EphemerisBackendInfo::id`; `None` selects `AppInfo::default_ephemeris`.
    #[serde(default)]
    pub ephemeris_id: Option<String>,
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

/// A computed visibility map.
///
/// # Payload format
///
/// The map travels as **raw zone indices plus a legend**, not as an image.
/// `cells` holds `rows × cols` bytes, row-major from the north-west corner.
/// Each byte indexes `legend`, or is [`NO_DATA`] (255). The bytes are
/// base64-encoded (RFC 4648, standard alphabet, padded) so the payload is a
/// plain JSON string on every transport.
///
/// Why not a PNG: one byte per cell is already smaller than PNG's raw RGBA
/// input (a 1440 × 720 grid is about 1.04 MB of indices, 1.4 MB as base64),
/// costs no encode or decode beyond base64, and leaves colouring to the
/// client. The UI can then show the zone under the cursor, highlight or hide a
/// zone, or switch palettes without asking the backend again. The client
/// paints the indices into an `ImageData` at grid resolution and scales it
/// onto the canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapResult {
    pub criterion_id: String,
    pub date: CivilDate,
    pub crescent: Crescent,
    pub grid: GridSpec,
    /// Base64 of `rows × cols` zone bytes; see the type docs.
    pub cells: String,
    /// The value marking cells without an evaluation (always [`NO_DATA`]).
    pub no_data: u8,
    /// The criterion's legend; zone byte `i` is `legend[i]`.
    pub legend: Vec<ZoneLegendEntry>,
    /// Backend computation time, milliseconds.
    pub compute_ms: f64,
    /// True while the map comes from the placeholder generator rather than
    /// the visibility engine.
    pub synthetic: bool,
}

// ---------------------------------------------------------------------------
// location_report
// ---------------------------------------------------------------------------

/// Input of `location_report`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationReportParams {
    /// The moment of interest, UT. Its date selects the evening or morning.
    pub instant: UtDateTime,
    /// Offset of local time from UT, hours (display only).
    pub tz_offset_hours: f64,
    pub location: Location,
    pub criterion_id: String,
    pub crescent: Crescent,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
}

/// Crescent visibility at one place: the circumstances and the verdict.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationReport {
    pub location: Location,
    pub criterion_id: String,
    pub crescent: Crescent,
    /// Sunset (evening) or sunrise (morning), UT.
    pub sun_event: Option<UtDateTime>,
    /// Moonset (evening) or moonrise (morning), UT.
    pub moon_event: Option<UtDateTime>,
    /// The criterion's evaluation time, UT.
    pub best_time: Option<UtDateTime>,
    /// The preceding (evening) or following (morning) geocentric conjunction, UT.
    pub conjunction: Option<UtDateTime>,
    /// Circumstances at `best_time`.
    pub geometry: Option<CrescentGeometryDto>,
    /// The criterion's classification.
    pub verdict: Option<Verdict>,
    /// Free-text remarks (e.g. "no sunset at this latitude").
    pub notes: Vec<String>,
    /// True while the report is a placeholder.
    pub synthetic: bool,
}

/// Wire form of `mooncalc_visibility::CrescentGeometry` (degrees, as there).
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrescentGeometryDto {
    pub arcl_deg: f64,
    pub arcl_geo_deg: f64,
    pub arcv_deg: f64,
    pub daz_deg: f64,
    pub width_topo_arcmin: f64,
    pub width_geo_arcmin: f64,
    pub semi_diameter_topo_arcmin: f64,
    pub moon_alt_deg: f64,
    pub moon_alt_airless_deg: f64,
    pub sun_alt_deg: f64,
    pub moon_az_deg: f64,
    pub sun_az_deg: f64,
    pub lag_minutes: f64,
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
            daz_deg: g.daz_deg,
            width_topo_arcmin: g.width_topo_arcmin,
            width_geo_arcmin: g.width_geo_arcmin,
            semi_diameter_topo_arcmin: g.semi_diameter_topo_arcmin,
            moon_alt_deg: g.moon_alt_deg,
            moon_alt_airless_deg: g.moon_alt_airless_deg,
            sun_alt_deg: g.sun_alt_deg,
            moon_az_deg: g.moon_az_deg,
            sun_az_deg: g.sun_az_deg,
            lag_minutes: g.lag_minutes,
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
    /// The criterion's figure of merit (Yallop q, Odeh V, ...), if any.
    pub value: Option<f64>,
}

// ---------------------------------------------------------------------------
// search_cities
// ---------------------------------------------------------------------------

/// Input of `search_cities`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitySearchParams {
    /// Free text; matched case-insensitively against city and country names.
    pub query: String,
    /// Maximum number of results (default 20, capped at 100).
    #[serde(default)]
    pub limit: Option<u32>,
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
    pub location: Location,
    /// IANA time-zone name, e.g. `"Asia/Riyadh"`, if known.
    pub time_zone: Option<String>,
    pub population: Option<u64>,
}
