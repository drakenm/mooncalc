//! Tauri command handlers: thin wrappers over the `mooncalc-app` services.
//!
//! Each command takes at most one `params` argument (the JavaScript side calls
//! `invoke("name", { params })`) and returns `Result<_, ApiError>`. Commands
//! are `async` so they run off the main thread and never stall the window;
//! CPU-heavy work additionally moves to the blocking pool, where map rows are
//! spread over worker threads ([`crate::parallel`]).

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use mooncalc_app::dto::{
    ApiError, AppInfo, Basemap, BasemapParams, City, CitySearchParams, CriterionInfo,
    EarliestParams, EarliestResult, EphemerisBackendInfo, ExportPngParams, LoadDe440Params,
    LocationReport, LocationReportParams, MapParams, MapResult, MapTiming, NearestCity,
    NearestCityParams, PngImage,
};
use mooncalc_app::earliest::EarliestJob;
use mooncalc_app::map::{MapJob, MapOutput};
use mooncalc_app::{atlas, basemap, criteria, info, report};
use tauri::State;
use tauri::async_runtime::spawn_blocking;

use crate::AppState;
use crate::parallel::{default_threads, evaluate_parallel};

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

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1e3
}

/// Two requests describe the same map (ignoring whether values are sent).
fn same_map(a: &MapParams, b: &MapParams) -> bool {
    MapParams {
        include_values: true,
        ..a.clone()
    } == MapParams {
        include_values: true,
        ..b.clone()
    }
}

/// Prepare and evaluate a map in parallel. A newer map request makes this
/// one stop early with a `cancelled` error.
fn compute_map(
    state: &AppState,
    params: &MapParams,
) -> Result<(MapResult, Arc<MapOutput>), ApiError> {
    let generation = state.map_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let t0 = Instant::now();
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    let job = MapJob::prepare(&*eph, params)?;
    let prepare_ms = ms(t0);
    let t1 = Instant::now();
    let (out, threads) = evaluate_parallel(&job, default_threads(), || {
        state.map_generation.load(Ordering::Relaxed) == generation
    })?;
    let evaluate_ms = ms(t1);
    let t2 = Instant::now();
    let mut result = job.result(&out, MapTiming::default());
    result.timing = MapTiming {
        prepare_ms,
        evaluate_ms,
        encode_ms: ms(t2),
        total_ms: ms(t0),
        threads: threads as u32,
    };
    let out = Arc::new(out);
    *state.last_map.lock().unwrap_or_else(|e| e.into_inner()) = Some(out.clone());
    Ok((result, out))
}

/// Version, runtime and ephemeris backends.
#[tauri::command]
pub async fn app_info(state: State<'_, Arc<AppState>>) -> Result<AppInfo, ApiError> {
    trace("app_info");
    Ok(info::app_info(
        &format!("tauri {}", tauri::VERSION),
        &state.ephemerides(),
    ))
}

/// Every sighting criterion with its legend.
#[tauri::command]
pub async fn criteria_list() -> Result<Vec<CriterionInfo>, ApiError> {
    trace("criteria_list");
    Ok(criteria::list())
}

/// A visibility map as zone indices, value field and legend (see
/// [`MapResult`]).
#[tauri::command]
pub async fn compute_visibility_map(
    state: State<'_, Arc<AppState>>,
    params: MapParams,
) -> Result<MapResult, ApiError> {
    trace("compute_visibility_map");
    let state = state.inner().clone();
    blocking(move || compute_map(&state, &params).map(|(r, _)| r)).await
}

/// Render a map to PNG (reusing the last computed map when it matches).
#[tauri::command]
pub async fn export_map_png(
    state: State<'_, Arc<AppState>>,
    params: ExportPngParams,
) -> Result<PngImage, ApiError> {
    trace("export_map_png");
    let state = state.inner().clone();
    blocking(move || {
        let cached = state
            .last_map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .filter(|m| same_map(&m.params, &params.map));
        let out = match cached {
            Some(out) => out,
            None => compute_map(&state, &params.map)?.1,
        };
        out.render_png(&params)
    })
    .await
}

/// Where `save_map_png` wrote the image.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFile {
    pub path: String,
    pub width: u32,
    pub height: u32,
}

/// `dir/name`, or `dir/stem-2.ext`, `-3` ... if that exists.
fn unique_path(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    let mut path = dir.join(name);
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
    }
    path
}

/// Render a map to PNG and save it in the user's Downloads folder (never
/// overwriting), returning the path. The desktop counterpart of
/// `export_map_png`, since the webview cannot save downloads itself.
#[tauri::command]
pub async fn save_map_png(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    params: ExportPngParams,
) -> Result<SavedFile, ApiError> {
    use tauri::Manager;
    trace("save_map_png");
    let dir = app
        .path()
        .download_dir()
        .map_err(|e| ApiError::internal(format!("no Downloads folder: {e}")))?;
    let png = export_map_png(state, params).await?;
    blocking(move || {
        let bytes = mooncalc_app::codec::base64_decode(&png.png)
            .ok_or_else(|| ApiError::internal("PNG encoding failed"))?;
        std::fs::create_dir_all(&dir)
            .map_err(|e| ApiError::internal(format!("{}: {e}", dir.display())))?;
        let path = unique_path(&dir, &png.file_name);
        std::fs::write(&path, bytes)
            .map_err(|e| ApiError::internal(format!("{}: {e}", path.display())))?;
        Ok(SavedFile {
            path: path.display().to_string(),
            width: png.width,
            height: png.height,
        })
    })
    .await
}

/// Day events and both crescents under every criterion at one location.
#[tauri::command]
pub async fn location_report(
    state: State<'_, Arc<AppState>>,
    params: LocationReportParams,
) -> Result<LocationReport, ApiError> {
    trace("location_report");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    blocking(move || report::location_report(&*eph, &params)).await
}

/// City atlas search.
#[tauri::command]
pub async fn search_cities(params: CitySearchParams) -> Result<Vec<City>, ApiError> {
    trace("search_cities");
    blocking(move || Ok(atlas::search_cities(&params))).await
}

/// The atlas city nearest to a point.
#[tauri::command]
pub async fn nearest_city(params: NearestCityParams) -> Result<Option<NearestCity>, ApiError> {
    trace("nearest_city");
    blocking(move || atlas::nearest_city(&params)).await
}

/// Earliest-sighting scan after the conjunction nearest a date, each
/// evening's grid evaluated in parallel.
#[tauri::command]
pub async fn earliest_sighting(
    state: State<'_, Arc<AppState>>,
    params: EarliestParams,
) -> Result<EarliestResult, ApiError> {
    trace("earliest_sighting");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    blocking(move || {
        let t0 = Instant::now();
        let mut job = EarliestJob::prepare(&*eph, &params)?;
        let (mut prepare_ms, mut evaluate_ms, mut threads) = (ms(t0), 0.0, 1);
        for date in job.dates().to_vec() {
            let t = Instant::now();
            let map = MapJob::prepare(&*eph, &job.map_params(date))?;
            prepare_ms += ms(t);
            let t = Instant::now();
            let (out, used) = evaluate_parallel(&map, default_threads(), || true)?;
            evaluate_ms += ms(t);
            threads = threads.max(used);
            job.accumulate(&out);
            if job.is_complete() {
                break;
            }
        }
        Ok(job.result(MapTiming {
            prepare_ms,
            evaluate_ms,
            encode_ms: 0.0,
            total_ms: ms(t0),
            threads: threads as u32,
        }))
    })
    .await
}

/// Simplified Natural Earth coastlines and borders.
#[tauri::command]
pub async fn basemap(params: BasemapParams) -> Result<Basemap, ApiError> {
    trace("basemap");
    blocking(move || basemap::basemap(&params)).await
}

/// Load a DE440/DE440s `.bsp` file by path and make it the `de440` backend.
#[tauri::command]
pub async fn load_de440(
    state: State<'_, Arc<AppState>>,
    params: LoadDe440Params,
) -> Result<EphemerisBackendInfo, ApiError> {
    trace("load_de440");
    let state = state.inner().clone();
    blocking(move || state.load_de440(&params.path)).await
}

/// Sky, Trajectory and Moon commands.
pub mod sky;
