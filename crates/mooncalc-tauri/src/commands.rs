//! Tauri command handlers: thin wrappers over [`crate::services`].
//!
//! Each command takes at most one `params` argument (the JavaScript side calls
//! `invoke("name", { params })`) and returns `Result<_, ApiError>`. Commands
//! are `async` so they run off the main thread and never stall the window;
//! CPU-heavy work additionally moves to the blocking pool.

use std::time::Instant;

use tauri::async_runtime::spawn_blocking;

use crate::dto::{
    ApiError, AppInfo, City, CitySearchParams, CriterionInfo, LocationReport, LocationReportParams,
    MapParams, MapResult,
};
use crate::services;

/// Log each command in debug builds, so a developer (or a smoke test) can see
/// frontend round-trips on stderr.
fn trace(command: &str) {
    if cfg!(debug_assertions) {
        eprintln!("[mooncalc-tauri] command {command}");
    }
}

/// Run CPU-bound work on the blocking thread pool.
async fn blocking<T, F>(f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, ApiError> + Send + 'static,
{
    spawn_blocking(f)
        .await
        .map_err(|e| ApiError::internal(format!("worker failed: {e}")))?
}

/// Version, runtime and available ephemeris backends.
#[tauri::command]
pub async fn app_info() -> Result<AppInfo, ApiError> {
    trace("app_info");
    Ok(services::app_info(&format!("tauri {}", tauri::VERSION)))
}

/// Every sighting criterion with its legend.
#[tauri::command]
pub async fn criteria_list() -> Result<Vec<CriterionInfo>, ApiError> {
    trace("criteria_list");
    Ok(services::criteria_list())
}

/// A world visibility map as zone indices plus legend (see [`MapResult`]).
#[tauri::command]
pub async fn compute_visibility_map(params: MapParams) -> Result<MapResult, ApiError> {
    trace("compute_visibility_map");
    blocking(move || {
        let start = Instant::now();
        let mut result = services::compute_visibility_map(&params)?;
        result.compute_ms = start.elapsed().as_secs_f64() * 1e3;
        Ok(result)
    })
    .await
}

/// Crescent circumstances and verdict at one location.
#[tauri::command]
pub async fn location_report(params: LocationReportParams) -> Result<LocationReport, ApiError> {
    trace("location_report");
    blocking(move || services::location_report(&params)).await
}

/// City atlas search.
#[tauri::command]
pub async fn search_cities(params: CitySearchParams) -> Result<Vec<City>, ApiError> {
    trace("search_cities");
    Ok(services::search_cities(&params))
}
