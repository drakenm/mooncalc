//! Application services behind the commands.
//!
//! Plain Rust functions from DTO to DTO with no Tauri types, so they can move
//! into the planned `mooncalc-app` crate and be shared with the egui and WASM
//! frontends. They are also kept portable (no clocks, threads or file I/O);
//! timing and threading are the transport layer's job (see `commands.rs`).

use crate::codec::base64_encode;
use crate::dto::{
    ApiError, AppInfo, City, CitySearchParams, CivilDate, CriterionInfo, EphemerisBackendInfo,
    GridSpec, Location, LocationReport, LocationReportParams, MapParams, MapResult, NO_DATA,
    UtDateTime,
};
use crate::synthetic;

/// Product name shown in the UI.
pub const APP_NAME: &str = "MoonCalc";

/// Identifier of the built-in ephemeris.
pub const DEFAULT_EPHEMERIS: &str = "meeus";

/// Smallest and largest accepted map grid spacing, degrees.
pub const RESOLUTION_RANGE_DEG: (f64, f64) = (0.1, 10.0);

/// Static application information. `runtime` names the hosting frontend.
pub fn app_info(runtime: &str) -> AppInfo {
    AppInfo {
        name: APP_NAME.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        runtime: runtime.into(),
        ephemeris_backends: vec![
            EphemerisBackendInfo {
                id: DEFAULT_EPHEMERIS.into(),
                name: "Meeus / ELP-2000/82".into(),
                description: "Built-in analytical theory (Meeus, Astronomical Algorithms, \
                              2nd ed.): VSOP87 Sun, ELP-2000/82 Moon. No data files needed."
                    .into(),
                available: true,
            },
            EphemerisBackendInfo {
                id: "de440".into(),
                name: "JPL DE440".into(),
                description: "Numerical ephemeris read from a user-supplied DE440 or DE440s \
                              .bsp file. Requires a build with the `de440` feature."
                    .into(),
                available: cfg!(feature = "de440"),
            },
        ],
        default_ephemeris: DEFAULT_EPHEMERIS.into(),
    }
}

/// Every criterion, in display order.
///
/// Comes from `mooncalc_visibility::criteria::all()`. While that registry is
/// still empty (wave 1), stand-in Yallop and Odeh entries are returned so the
/// UI has legends to work with.
pub fn criteria_list() -> Vec<CriterionInfo> {
    let real: Vec<CriterionInfo> = mooncalc_visibility::criteria::all()
        .iter()
        .map(|c| CriterionInfo {
            id: c.id().into(),
            name: c.name().into(),
            reference: c.reference().into(),
            zones: c.zones().iter().map(Into::into).collect(),
        })
        .collect();
    if real.is_empty() {
        synthetic::fallback_criteria()
    } else {
        real
    }
}

fn find_criterion(id: &str) -> Result<CriterionInfo, ApiError> {
    criteria_list()
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| ApiError::unknown_criterion(id))
}

/// Compute a world visibility map.
///
/// Currently returns a synthetic zone pattern (`synthetic: true`) with the
/// real payload format; the grid engine replaces it in wave 2.
/// `compute_ms` is left at zero for the caller to fill in.
pub fn compute_visibility_map(params: &MapParams) -> Result<MapResult, ApiError> {
    validate_date(params.date)?;
    validate_ephemeris(params.ephemeris_id.as_deref())?;
    let (lo, hi) = RESOLUTION_RANGE_DEG;
    if !(lo..=hi).contains(&params.resolution_deg) {
        return Err(ApiError::invalid_params(format!(
            "resolutionDeg must be between {lo} and {hi}, got {}",
            params.resolution_deg
        )));
    }
    let criterion = find_criterion(&params.criterion_id)?;
    if criterion.zones.len() >= usize::from(NO_DATA) {
        return Err(ApiError::internal("criterion has too many zones"));
    }

    let grid = synthetic::visibility_grid(
        params.date,
        params.crescent,
        params.resolution_deg,
        criterion.zones.len(),
    );
    let bytes: Vec<u8> = grid.cells.iter().map(|c| c.unwrap_or(NO_DATA)).collect();

    Ok(MapResult {
        criterion_id: criterion.id,
        date: params.date,
        crescent: params.crescent,
        grid: GridSpec {
            lat_min_deg: grid.lat_min_deg,
            lat_max_deg: grid.lat_max_deg,
            lon_min_deg: grid.lon_min_deg,
            lon_max_deg: grid.lon_max_deg,
            rows: u32::try_from(grid.rows).map_err(|e| ApiError::internal(e.to_string()))?,
            cols: u32::try_from(grid.cols).map_err(|e| ApiError::internal(e.to_string()))?,
        },
        cells: base64_encode(&bytes),
        no_data: NO_DATA,
        legend: criterion.zones,
        compute_ms: 0.0,
        synthetic: true,
    })
}

/// Crescent circumstances and verdict at one place.
///
/// Validates its input and echoes it back with every computed field empty
/// (`synthetic: true`) until the circumstances engine lands in wave 2.
pub fn location_report(params: &LocationReportParams) -> Result<LocationReport, ApiError> {
    validate_date_time(params.instant)?;
    validate_location(params.location)?;
    validate_ephemeris(params.ephemeris_id.as_deref())?;
    if !(-14.0..=14.0).contains(&params.tz_offset_hours) {
        return Err(ApiError::invalid_params(
            "tzOffsetHours must be between -14 and +14",
        ));
    }
    let criterion = find_criterion(&params.criterion_id)?;
    Ok(LocationReport {
        location: params.location,
        criterion_id: criterion.id,
        crescent: params.crescent,
        sun_event: None,
        moon_event: None,
        best_time: None,
        conjunction: None,
        geometry: None,
        verdict: None,
        notes: vec![
            "Placeholder: rise/set times, crescent geometry and the criterion verdict \
             arrive with the circumstances engine (wave 2)."
                .into(),
        ],
        synthetic: true,
    })
}

/// Default and maximum number of city search results.
pub const CITY_LIMIT: (u32, u32) = (20, 100);

/// Search the city atlas by name or country, case-insensitively.
///
/// Name-prefix matches come first, then other matches, each alphabetically.
/// Searches a small built-in sample until `mooncalc-atlas` lands.
pub fn search_cities(params: &CitySearchParams) -> Vec<City> {
    let query = params.query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let limit = params.limit.unwrap_or(CITY_LIMIT.0).min(CITY_LIMIT.1) as usize;
    let mut hits: Vec<(bool, City)> = synthetic::sample_cities()
        .into_iter()
        .filter_map(|c| {
            let name = c.name.to_lowercase();
            let prefix = name.starts_with(&query);
            let hit = prefix
                || name.contains(&query)
                || c.country.to_lowercase().contains(&query)
                || c.admin1
                    .as_deref()
                    .is_some_and(|a| a.to_lowercase().contains(&query));
            hit.then_some((prefix, c))
        })
        .collect();
    hits.sort_by(|(pa, a), (pb, b)| pb.cmp(pa).then_with(|| a.name.cmp(&b.name)));
    hits.into_iter().take(limit).map(|(_, c)| c).collect()
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn is_leap_year(year: i32) -> bool {
    // Julian calendar before the 1582 Gregorian reform.
    if year < 1583 {
        year.rem_euclid(4) == 0
    } else {
        (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn validate_date(d: CivilDate) -> Result<(), ApiError> {
    if !(-9999..=9999).contains(&d.year) {
        return Err(ApiError::invalid_params(
            "year must be between -9999 and 9999",
        ));
    }
    if !(1..=12).contains(&d.month) {
        return Err(ApiError::invalid_params("month must be between 1 and 12"));
    }
    if d.day < 1 || d.day > days_in_month(d.year, d.month) {
        return Err(ApiError::invalid_params(format!(
            "{:04}-{:02} has no day {}",
            d.year, d.month, d.day
        )));
    }
    Ok(())
}

fn validate_date_time(t: UtDateTime) -> Result<(), ApiError> {
    validate_date(t.date())?;
    if t.hour > 23 || t.minute > 59 || !(0.0..61.0).contains(&t.second) {
        return Err(ApiError::invalid_params("time of day out of range"));
    }
    Ok(())
}

fn validate_location(l: Location) -> Result<(), ApiError> {
    if !(-90.0..=90.0).contains(&l.lat_deg) {
        return Err(ApiError::invalid_params(
            "latDeg must be between -90 and 90",
        ));
    }
    if !(-180.0..=180.0).contains(&l.lon_deg) {
        return Err(ApiError::invalid_params(
            "lonDeg must be between -180 and 180",
        ));
    }
    if !(-500.0..=9000.0).contains(&l.elevation_m) {
        return Err(ApiError::invalid_params(
            "elevationM must be between -500 and 9000",
        ));
    }
    Ok(())
}

fn validate_ephemeris(id: Option<&str>) -> Result<(), ApiError> {
    let Some(id) = id else { return Ok(()) };
    let info = app_info("");
    match info.ephemeris_backends.iter().find(|b| b.id == id) {
        Some(b) if b.available => Ok(()),
        Some(b) => Err(ApiError::invalid_params(format!(
            "ephemeris {:?} is not available in this build",
            b.id
        ))),
        None => Err(ApiError::invalid_params(format!(
            "unknown ephemeris {id:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::Crescent;

    fn map_params() -> MapParams {
        MapParams {
            date: CivilDate {
                year: 2026,
                month: 11,
                day: 10,
            },
            criterion_id: "yallop".into(),
            crescent: Crescent::Evening,
            resolution_deg: 0.25,
            ephemeris_id: None,
        }
    }

    #[test]
    fn full_world_map_payload_is_one_byte_per_cell() {
        let r = compute_visibility_map(&map_params()).unwrap();
        assert_eq!((r.grid.rows, r.grid.cols), (720, 1440));
        // 1440 * 720 bytes, base64 with padding.
        assert_eq!(r.cells.len(), (1440 * 720usize).div_ceil(3) * 4);
        assert_eq!(r.no_data, NO_DATA);
        assert!(r.synthetic);
        assert_eq!(r.legend.len(), criteria_list()[0].zones.len());
    }

    #[test]
    fn map_rejects_bad_params() {
        let mut p = map_params();
        p.resolution_deg = 0.01;
        assert_eq!(
            compute_visibility_map(&p).unwrap_err().code,
            "invalid-params"
        );

        let mut p = map_params();
        p.criterion_id = "nope".into();
        assert_eq!(
            compute_visibility_map(&p).unwrap_err().code,
            "unknown-criterion"
        );

        let mut p = map_params();
        p.date.month = 2;
        p.date.day = 29; // 2026 is not a leap year.
        assert!(compute_visibility_map(&p).is_err());

        let mut p = map_params();
        p.ephemeris_id = Some("vsop2013".into());
        assert!(compute_visibility_map(&p).is_err());
    }

    #[test]
    fn leap_years() {
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(1500)); // Julian.
        assert!(is_leap_year(2024));
    }

    #[test]
    fn report_validates_and_echoes() {
        let p = LocationReportParams {
            instant: UtDateTime {
                year: 2026,
                month: 11,
                day: 10,
                hour: 15,
                minute: 0,
                second: 0.0,
            },
            tz_offset_hours: 3.0,
            location: Location {
                lat_deg: 21.4225,
                lon_deg: 39.8262,
                elevation_m: 277.0,
            },
            criterion_id: "odeh".into(),
            crescent: Crescent::Evening,
            ephemeris_id: Some(DEFAULT_EPHEMERIS.into()),
        };
        let r = location_report(&p).unwrap();
        assert_eq!(r.location, p.location);
        assert!(r.synthetic);

        let mut bad = p.clone();
        bad.location.lat_deg = 91.0;
        assert!(location_report(&bad).is_err());
        let mut bad = p;
        bad.instant.hour = 24;
        assert!(location_report(&bad).is_err());
    }

    #[test]
    fn city_search_ranks_prefix_matches_first() {
        let r = search_cities(&CitySearchParams {
            query: "ma".into(),
            limit: None,
        });
        let names: Vec<_> = r.iter().map(|c| c.name.as_str()).collect();
        // "Madinah" and "Makkah" start with "ma"; "Kuala Lumpur" matches by
        // country (Malaysia).
        assert_eq!(&names[..2], ["Madinah", "Makkah"]);
        assert!(names.contains(&"Kuala Lumpur"));

        let one = search_cities(&CitySearchParams {
            query: "a".into(),
            limit: Some(1),
        });
        assert_eq!(one.len(), 1);
        assert!(
            search_cities(&CitySearchParams {
                query: "  ".into(),
                limit: None
            })
            .is_empty()
        );
    }

    #[test]
    fn app_info_lists_the_default_backend() {
        let info = app_info("test");
        assert!(
            info.ephemeris_backends
                .iter()
                .any(|b| b.id == info.default_ephemeris && b.available)
        );
    }
}
