//! Sky, Trajectory and Moon commands: thin wrappers over
//! [`mooncalc_app::sky`], [`mooncalc_app::trajectory`] and
//! [`mooncalc_app::moon`], each on the blocking pool. Every result is small
//! (a few hundred samples), so none needs worker threads.

use std::sync::Arc;

use mooncalc_app::dto::ApiError;
use mooncalc_app::moon::{self, MoonFeatures, MoonView, MoonViewParams};
use mooncalc_app::sky::{self, SkyScene, SkySceneParams, SkyStars, SkyStarsParams};
use mooncalc_app::trajectory::{self, TrajectoryDay, TrajectoryParams};
use tauri::State;

use super::{blocking, trace};
use crate::AppState;

/// The evening (or morning) sky of a crescent sighting.
#[tauri::command]
pub async fn sky_scene(
    state: State<'_, Arc<AppState>>,
    params: SkySceneParams,
) -> Result<SkyScene, ApiError> {
    trace("sky_scene");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    blocking(move || sky::sky_scene(&*eph, &params)).await
}

/// Bright stars of date, with altitude and azimuth for a location.
#[tauri::command]
pub async fn sky_stars(params: SkyStarsParams) -> Result<SkyStars, ApiError> {
    trace("sky_stars");
    blocking(move || sky::sky_stars(&params)).await
}

/// The Sun's and Moon's local day.
#[tauri::command]
pub async fn trajectory_day(
    state: State<'_, Arc<AppState>>,
    params: TrajectoryParams,
) -> Result<TrajectoryDay, ApiError> {
    trace("trajectory_day");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    blocking(move || trajectory::trajectory_day(&*eph, &params)).await
}

/// The Moon's physical ephemeris, phase and libration curve at an instant.
#[tauri::command]
pub async fn moon_view(
    state: State<'_, Arc<AppState>>,
    params: MoonViewParams,
) -> Result<MoonView, ApiError> {
    trace("moon_view");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    blocking(move || moon::moon_view(&*eph, &params)).await
}

/// The near-side craters and maria.
#[tauri::command]
pub async fn moon_features() -> Result<MoonFeatures, ApiError> {
    trace("moon_features");
    Ok(moon::moon_features())
}
