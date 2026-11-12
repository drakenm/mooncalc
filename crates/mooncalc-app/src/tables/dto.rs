//! Wire types of the [`crate::tables`] service. Same conventions as
//! [`crate::dto`]: camelCase JSON, angles in degrees (`_deg`), times in UT
//! ([`UtDateTime`]). The TypeScript mirror is
//! `frontend/tauri/src/backend/types-tables.ts`.

use serde::{Deserialize, Serialize};

use crate::dto::{CivilDate, Location, UtDateTime};

// ---------------------------------------------------------------------------
// ephemeris_table
// ---------------------------------------------------------------------------

/// Input of `ephemeris_table`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EphemerisTableParams {
    /// First row, UT.
    pub start: UtDateTime,
    /// Interval between rows, minutes (`0.1..=525600`, i.e. up to a year).
    pub step_minutes: f64,
    /// Number of rows, `1..=MAX_TABLE_ROWS`.
    pub count: u32,
    /// Observer for the topocentric columns; `None` for geocentric only.
    #[serde(default)]
    pub location: Option<Location>,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
    /// Offset of local civil time from UT, hours, used when `time_zone` is
    /// absent: only sets the results' `offset_hours`.
    #[serde(default)]
    pub tz_offset_hours: f64,
    /// IANA time zone for the results' `offset_hours`.
    #[serde(default)]
    pub time_zone: Option<String>,
}

/// The topocentric columns of one body.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopocentricDto {
    /// Altitude with standard refraction.
    pub alt_deg: f64,
    /// Airless altitude.
    pub alt_airless_deg: f64,
    /// Azimuth, north through east.
    pub az_deg: f64,
    pub ra_deg: f64,
    pub dec_deg: f64,
    pub distance_km: f64,
    pub semidiameter_arcmin: f64,
}

/// The columns of one body (geocentric apparent, true equinox of date).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyRowDto {
    pub ra_deg: f64,
    pub dec_deg: f64,
    /// Ecliptic longitude.
    pub lon_deg: f64,
    /// Ecliptic latitude.
    pub lat_deg: f64,
    pub distance_km: f64,
    pub semidiameter_arcmin: f64,
    /// Equatorial horizontal parallax.
    pub parallax_arcmin: f64,
    pub topocentric: Option<TopocentricDto>,
}

/// Librations and the Sun's selenographic position.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrationDto {
    /// Total libration in longitude (optical + physical).
    pub lon_deg: f64,
    /// Total libration in latitude.
    pub lat_deg: f64,
    /// Position angle of the Moon's axis.
    pub axis_position_angle_deg: f64,
    /// Selenographic colongitude of the Sun.
    pub colongitude_deg: f64,
    /// Selenographic latitude of the Sun.
    pub sun_lat_deg: f64,
}

/// One table row.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EphemerisRowDto {
    pub time: UtDateTime,
    pub jd_ut: f64,
    pub jd_tt: f64,
    pub sun: BodyRowDto,
    pub moon: BodyRowDto,
    /// Geocentric elongation of the Moon from the Sun.
    pub elongation_deg: f64,
    /// Sun–Moon–Earth angle.
    pub phase_angle_deg: f64,
    /// Illuminated fraction, 0..1.
    pub illuminated_fraction: f64,
    /// Position angle of the bright limb, from north through east.
    pub bright_limb_deg: f64,
    /// Elongation seen from the observer, when a location is given.
    pub topocentric_elongation_deg: Option<f64>,
    /// Days since the last geocentric conjunction.
    pub moon_age_days: f64,
    /// That conjunction.
    pub conjunction: UtDateTime,
    /// Geocentric librations.
    pub libration: LibrationDto,
}

/// Result of `ephemeris_table`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EphemerisTable {
    pub rows: Vec<EphemerisRowDto>,
    pub ephemeris: String,
}

// ---------------------------------------------------------------------------
// rise_set_month
// ---------------------------------------------------------------------------

/// Input of `rise_set_month`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiseSetParams {
    pub year: i32,
    /// 1..=12.
    pub month: u32,
    /// Fixed offset of local civil time from UT, hours, used when
    /// `time_zone` is absent: the local days the events are grouped by.
    #[serde(default)]
    pub tz_offset_hours: f64,
    /// IANA time zone whose midnights bound the days (a day across a DST
    /// change lasts 23 or 25 hours). Takes precedence over `tz_offset_hours`.
    #[serde(default)]
    pub time_zone: Option<String>,
    pub location: Location,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
}

/// A rise, set or transit with an angle.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AngledEventDto {
    pub time: UtDateTime,
    /// Azimuth for a rise or set, refracted altitude for a transit.
    pub angle_deg: f64,
}

/// A body's events on one local day.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyDayDto {
    pub rise: Option<AngledEventDto>,
    pub transit: Option<AngledEventDto>,
    pub set: Option<AngledEventDto>,
    /// `always-up` or `always-down` when the body neither rises nor sets.
    pub circumpolar: Option<String>,
}

/// Morning start and evening end of a twilight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwilightDto {
    pub dawn: Option<UtDateTime>,
    pub dusk: Option<UtDateTime>,
}

/// One local day.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiseSetDay {
    pub date: CivilDate,
    pub sun: BodyDayDto,
    pub moon: BodyDayDto,
    /// Sun's centre at −6°.
    pub civil: TwilightDto,
    /// −12°.
    pub nautical: TwilightDto,
    /// −18°.
    pub astronomical: TwilightDto,
}

/// Result of `rise_set_month`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiseSetMonth {
    pub year: i32,
    pub month: u32,
    /// The offset at local noon on the 1st, hours (every time also carries
    /// its own `offset_hours`).
    pub tz_offset_hours: f64,
    /// The IANA zone used, if any.
    pub time_zone: Option<String>,
    pub location: Location,
    pub days: Vec<RiseSetDay>,
    pub ephemeris: String,
}

// ---------------------------------------------------------------------------
// Year lists: phases, apsides and nodes, seasons
// ---------------------------------------------------------------------------

/// Input of the per-year lists (`lunar_phases`, `apsides_nodes`,
/// `seasons`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YearParams {
    pub year: i32,
    /// The year runs from local midnight on 1 January in this zone.
    #[serde(default)]
    pub tz_offset_hours: f64,
    /// IANA time zone: the year runs from local midnight on 1 January under
    /// its rules. Takes precedence over `tz_offset_hours`.
    #[serde(default)]
    pub time_zone: Option<String>,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
}

/// One lunation's phases that fall in the year.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LunationDto {
    /// Meeus/NASA lunation number (0 at the new Moon of 2000 January 6).
    pub k: i64,
    /// The new Moon starting the lunation (it may precede the year).
    pub new_moon: UtDateTime,
    /// Length of the lunation to the next new Moon, days.
    pub length_days: f64,
    /// New Moon, first quarter, full Moon, last quarter, where inside the
    /// year.
    pub phases: [Option<UtDateTime>; 4],
    /// The Moon's age at each phase, days.
    pub ages_days: [Option<f64>; 4],
}

/// Result of `lunar_phases`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LunarPhasesYear {
    pub year: i32,
    pub lunations: Vec<LunationDto>,
    pub ephemeris: String,
}

/// A lunar perigee or apogee.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApsisDto {
    /// `perigee` or `apogee`.
    pub kind: String,
    pub time: UtDateTime,
    pub distance_km: f64,
    /// Equatorial horizontal parallax.
    pub parallax_arcsec: f64,
    /// Geocentric apparent diameter.
    pub diameter_arcmin: f64,
}

/// A passage through a node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDto {
    /// `ascending` or `descending`.
    pub kind: String,
    pub time: UtDateTime,
}

/// A northern or southern extreme of the Moon's declination.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclinationDto {
    /// `north` or `south`.
    pub kind: String,
    pub time: UtDateTime,
    pub dec_deg: f64,
}

/// Result of `apsides_nodes`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApsidesNodesYear {
    pub year: i32,
    pub apsides: Vec<ApsisDto>,
    pub nodes: Vec<NodeDto>,
    pub declinations: Vec<DeclinationDto>,
    pub ephemeris: String,
}

/// An equinox or solstice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonDto {
    /// `march-equinox`, `june-solstice`, `september-equinox`,
    /// `december-solstice`.
    pub kind: String,
    pub name: String,
    pub time: UtDateTime,
    /// Equation of time then, minutes.
    pub equation_of_time_min: f64,
}

/// The equation of time at local noon of one day.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EquationOfTimePoint {
    pub date: CivilDate,
    /// Apparent minus mean solar time, minutes.
    pub minutes: f64,
}

/// Result of `seasons`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonsYear {
    pub year: i32,
    pub seasons: Vec<SeasonDto>,
    /// One point per day of the year.
    pub equation_of_time: Vec<EquationOfTimePoint>,
    pub ephemeris: String,
}

// ---------------------------------------------------------------------------
// eclipses
// ---------------------------------------------------------------------------

/// Which eclipses to list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EclipseKindFilter {
    #[default]
    Any,
    Solar,
    Lunar,
}

/// Input of `eclipses`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EclipsesParams {
    pub first_year: i32,
    /// At most `MAX_ECLIPSE_YEARS` after `first_year`.
    pub last_year: i32,
    #[serde(default)]
    pub filter: EclipseKindFilter,
    /// Observer for the local circumstances; `None` for none.
    #[serde(default)]
    pub location: Option<Location>,
    #[serde(default)]
    pub ephemeris_id: Option<String>,
    /// Offset of local civil time from UT, hours, used when `time_zone` is
    /// absent: only sets the results' `offset_hours`.
    #[serde(default)]
    pub tz_offset_hours: f64,
    /// IANA time zone for the results' `offset_hours`.
    #[serde(default)]
    pub time_zone: Option<String>,
}

/// A contact of a solar eclipse at the observer.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolarContactDto {
    pub time: UtDateTime,
    /// The Sun's airless altitude and azimuth then.
    pub sun_alt_deg: f64,
    pub sun_az_deg: f64,
}

/// A solar eclipse seen from the observer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSolarDto {
    /// `partial`, `annular` or `total`.
    pub kind: String,
    pub c1: SolarContactDto,
    pub c2: Option<SolarContactDto>,
    pub max: SolarContactDto,
    pub c3: Option<SolarContactDto>,
    pub c4: SolarContactDto,
    /// Fraction of the Sun's diameter covered at maximum.
    pub magnitude: f64,
    /// Fraction of the Sun's disc area covered at maximum.
    pub obscuration: f64,
    pub moon_sun_ratio: f64,
    /// C1 to C4, minutes.
    pub duration_min: f64,
    /// C2 to C3 (totality or annularity), minutes.
    pub central_duration_min: Option<f64>,
    /// Whether the Sun is above the horizon during some of the eclipse.
    pub visible: bool,
}

/// Refined circumstances of a solar eclipse.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolarEclipseDto {
    /// Whether the shadow axis touches the Earth.
    pub central: bool,
    /// Point of greatest eclipse.
    pub greatest_lat_deg: f64,
    pub greatest_lon_deg: f64,
    /// The Sun's altitude there.
    pub sun_alt_deg: f64,
    /// At the observer, if a location was given and any phase reaches it.
    pub local: Option<LocalSolarDto>,
}

/// A contact of a lunar eclipse, and the Moon seen from the observer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LunarContactDto {
    /// `P1`, `U1`, `U2`, `max`, `U3`, `U4`, `P4`.
    pub code: String,
    pub label: String,
    pub time: UtDateTime,
    /// The Moon's airless topocentric altitude, if a location was given.
    pub moon_alt_deg: Option<f64>,
    /// Whether its upper limb is above the horizon (34′ refraction).
    pub above_horizon: Option<bool>,
}

/// Refined circumstances of a lunar eclipse.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LunarEclipseDto {
    pub penumbral_magnitude: f64,
    pub umbral_magnitude: f64,
    pub contacts: Vec<LunarContactDto>,
    pub penumbral_duration_min: Option<f64>,
    pub partial_duration_min: Option<f64>,
    pub total_duration_min: Option<f64>,
    /// Where the Moon is in the zenith at greatest eclipse.
    pub zenith_lat_deg: f64,
    pub zenith_lon_deg: f64,
    /// Whether the Moon is up at some contact, if a location was given.
    pub visible: Option<bool>,
}

/// One eclipse.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EclipseDto {
    /// `solar` or `lunar`.
    pub body: String,
    /// Solar: `partial`, `annular`, `total`, `hybrid`; lunar: `penumbral`,
    /// `partial`, `total`.
    pub kind: String,
    /// E.g. "Solar total".
    pub label: String,
    /// Meeus/NASA lunation number.
    pub lunation: i64,
    pub greatest: UtDateTime,
    /// Least distance of the shadow axis from the Earth's centre (solar) or
    /// of the Moon from the shadow axis (lunar), Earth radii.
    pub gamma: f64,
    /// Magnitude at greatest eclipse (umbral for lunar eclipses).
    pub magnitude: f64,
    pub solar: Option<SolarEclipseDto>,
    pub lunar: Option<LunarEclipseDto>,
}

/// Result of `eclipses`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EclipseList {
    pub first_year: i32,
    pub last_year: i32,
    pub location: Option<Location>,
    pub eclipses: Vec<EclipseDto>,
    pub ephemeris: String,
}
