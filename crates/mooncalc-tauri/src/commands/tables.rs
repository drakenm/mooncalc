//! Ephemeris-tab commands over `mooncalc_app::tables`, on the blocking
//! pool. Table rows and eclipse details are spread over worker threads.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use mooncalc_app::dto::ApiError;
use mooncalc_app::tables::{
    self, ApsidesNodesYear, DEFAULT_TABLE_CHUNK, EclipseList, EclipsesParams, EphemerisTable,
    EphemerisTableParams, LunarPhasesYear, RiseSetMonth, RiseSetParams, SeasonsYear, YearParams,
};
use tauri::State;

use super::{blocking, trace};
use crate::AppState;
use crate::parallel::default_threads;

/// `f` over `items` on up to [`default_threads`] scoped threads, which pull
/// items from a shared counter; results keep the input order.
pub(super) fn parallel_map<T, R, F>(items: &[T], f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let threads = default_threads().clamp(1, items.len().max(1));
    if threads == 1 {
        return items.iter().map(&f).collect();
    }
    let next = AtomicUsize::new(0);
    let worker = || {
        let mut out = Vec::new();
        loop {
            let i = next.fetch_add(1, Ordering::Relaxed);
            let Some(item) = items.get(i) else {
                return out;
            };
            out.push((i, f(item)));
        }
    };
    let mut all: Vec<(usize, R)> = thread::scope(|s| {
        let handles: Vec<_> = (0..threads).map(|_| s.spawn(worker)).collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("worker panicked"))
            .collect()
    });
    all.sort_by_key(|(i, _)| *i);
    all.into_iter().map(|(_, r)| r).collect()
}

/// Sun and Moon ephemeris table rows, computed in parallel chunks.
#[tauri::command]
pub async fn ephemeris_table(
    state: State<'_, Arc<AppState>>,
    params: EphemerisTableParams,
) -> Result<EphemerisTable, ApiError> {
    trace("ephemeris_table");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    blocking(move || {
        let n = tables::check_table(&*eph, &params)?;
        let chunks = tables::table_row_chunks(n, DEFAULT_TABLE_CHUNK);
        let rows = parallel_map(&chunks, |r| tables::table_rows(&*eph, &params, r.clone()))
            .into_iter()
            .flatten()
            .collect();
        Ok(EphemerisTable {
            rows,
            ephemeris: eph.name().into(),
        })
    })
    .await
}

/// A month of Sun and Moon rise/transit/set and twilight at one place.
#[tauri::command]
pub async fn rise_set_month(
    state: State<'_, Arc<AppState>>,
    params: RiseSetParams,
) -> Result<RiseSetMonth, ApiError> {
    trace("rise_set_month");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    blocking(move || tables::rise_set_month(&*eph, &params)).await
}

/// The principal lunar phases of a year.
#[tauri::command]
pub async fn lunar_phases(
    state: State<'_, Arc<AppState>>,
    params: YearParams,
) -> Result<LunarPhasesYear, ApiError> {
    trace("lunar_phases");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    blocking(move || tables::lunar_phases(&*eph, &params)).await
}

/// Perigees, apogees, node passages and declination extremes of a year.
#[tauri::command]
pub async fn apsides_nodes(
    state: State<'_, Arc<AppState>>,
    params: YearParams,
) -> Result<ApsidesNodesYear, ApiError> {
    trace("apsides_nodes");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    blocking(move || tables::apsides_nodes(&*eph, &params)).await
}

/// Equinoxes, solstices and the daily equation of time of a year.
#[tauri::command]
pub async fn seasons(
    state: State<'_, Arc<AppState>>,
    params: YearParams,
) -> Result<SeasonsYear, ApiError> {
    trace("seasons");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    blocking(move || tables::seasons(&*eph, &params)).await
}

/// Eclipses of a range of years with refined details and local
/// circumstances, each eclipse on a worker thread.
#[tauri::command]
pub async fn eclipses(
    state: State<'_, Arc<AppState>>,
    params: EclipsesParams,
) -> Result<EclipseList, ApiError> {
    trace("eclipses");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    blocking(move || {
        let list = tables::eclipse_candidates(&*eph, &params)?;
        let eclipses = parallel_map(&list, |e| {
            tables::eclipse_details(&*eph, e, params.location)
        });
        Ok(EclipseList {
            first_year: params.first_year,
            last_year: params.last_year,
            location: params.location,
            eclipses,
            ephemeris: eph.name().into(),
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_map_keeps_order() {
        let items: Vec<u32> = (0..1000).collect();
        let out = parallel_map(&items, |x| x * 2);
        assert_eq!(out, items.iter().map(|x| x * 2).collect::<Vec<_>>());
        assert!(parallel_map(&[] as &[u32], |x| *x).is_empty());
    }
}
