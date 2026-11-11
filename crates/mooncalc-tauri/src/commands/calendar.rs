//! Calendar-tab commands over `mooncalc_app::calendar`.
//!
//! The session's [`CalendarEngine`] lives in the managed [`CalendarState`].
//! View commands (`calendar_month`, `calendar_convert`,
//! `calendar_compare`) compute the computed-calendar years they lack on the
//! blocking pool, one year per worker thread, before answering, so the
//! frontend gets a complete view in one round trip.

use std::sync::Arc;

use mooncalc_app::calendar::{
    self, CalendarCatalog, CalendarCompareParams, CalendarComparison, CalendarConversion,
    CalendarConvertParams, CalendarEngine, CalendarListParams, CalendarMonth, CalendarMonthParams,
    MissingYear, MonthStartsParams, MonthStartsYear, VisibilityCalendarSpec,
};
use mooncalc_app::dto::ApiError;
use mooncalc_app::ephemeris::Ephemeris;
use tauri::State;

use super::tables::parallel_map;
use super::{blocking, trace};
use crate::AppState;

/// The calendar engine of the running app (computed month starts and their
/// providers), managed next to [`AppState`].
#[derive(Debug, Default, Clone)]
pub struct CalendarState(pub Arc<CalendarEngine>);

/// Compute `missing` years in parallel.
fn compute_missing(
    engine: &CalendarEngine,
    eph: &Arc<dyn Ephemeris>,
    ephemeris_id: Option<&str>,
    visibility: Option<&VisibilityCalendarSpec>,
    missing: &[MissingYear],
) -> Result<(), ApiError> {
    parallel_map(missing, |m| {
        engine.compute_year(
            eph,
            &MonthStartsParams {
                calendar: calendar::spec_for(&m.calendar_id, visibility),
                hijri_year: m.hijri_year,
                ephemeris_id: ephemeris_id.map(Into::into),
            },
        )
    })
    .into_iter()
    .try_for_each(|r| r.map(drop))
}

/// Answer `view`, first computing the years it reports missing.
fn complete<T>(
    engine: &CalendarEngine,
    eph: &Arc<dyn Ephemeris>,
    ephemeris_id: Option<&str>,
    visibility: Option<&VisibilityCalendarSpec>,
    view: impl Fn() -> Result<T, ApiError>,
    missing: impl Fn(&T) -> &[MissingYear],
) -> Result<T, ApiError> {
    let first = view()?;
    if missing(&first).is_empty() {
        return Ok(first);
    }
    compute_missing(engine, eph, ephemeris_id, visibility, missing(&first))?;
    view()
}

/// The calendars, region presets and month and weekday names.
#[tauri::command]
pub async fn calendar_list(params: CalendarListParams) -> Result<CalendarCatalog, ApiError> {
    trace("calendar_list");
    blocking(move || Ok(calendar::calendar_list(&params))).await
}

/// Compute one Hijri year of a calendar.
#[tauri::command]
pub async fn calendar_month_starts(
    state: State<'_, Arc<AppState>>,
    calendars: State<'_, CalendarState>,
    params: MonthStartsParams,
) -> Result<MonthStartsYear, ApiError> {
    trace("calendar_month_starts");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    let engine = calendars.0.clone();
    blocking(move || engine.compute_year(&eph, &params)).await
}

/// A month view, computing missing years first.
#[tauri::command]
pub async fn calendar_month(
    state: State<'_, Arc<AppState>>,
    calendars: State<'_, CalendarState>,
    params: CalendarMonthParams,
) -> Result<CalendarMonth, ApiError> {
    trace("calendar_month");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    let engine = calendars.0.clone();
    blocking(move || {
        complete(
            &engine,
            &eph,
            params.ephemeris_id.as_deref(),
            params.calendar.visibility.as_ref(),
            || engine.month(&*eph, &params),
            |r| &r.missing,
        )
    })
    .await
}

/// Civil ↔ Hijri conversion, computing missing years first.
#[tauri::command]
pub async fn calendar_convert(
    state: State<'_, Arc<AppState>>,
    calendars: State<'_, CalendarState>,
    params: CalendarConvertParams,
) -> Result<CalendarConversion, ApiError> {
    trace("calendar_convert");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    let engine = calendars.0.clone();
    blocking(move || {
        complete(
            &engine,
            &eph,
            params.ephemeris_id.as_deref(),
            params.calendar.visibility.as_ref(),
            || engine.convert(&*eph, &params),
            |r| &r.missing,
        )
    })
    .await
}

/// Month starts of a Hijri year under every calendar, computing the
/// computed calendars' year first.
#[tauri::command]
pub async fn calendar_compare(
    state: State<'_, Arc<AppState>>,
    calendars: State<'_, CalendarState>,
    params: CalendarCompareParams,
) -> Result<CalendarComparison, ApiError> {
    trace("calendar_compare");
    let eph = state.ephemeris(params.ephemeris_id.as_deref())?;
    let engine = calendars.0.clone();
    blocking(move || {
        complete(
            &engine,
            &eph,
            params.ephemeris_id.as_deref(),
            params.visibility.as_ref(),
            || engine.compare(&*eph, &params),
            |r| &r.missing,
        )
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_app::calendar::{CalendarSpec, MonthRef, PrimaryCalendar};

    #[test]
    fn complete_fills_missing_years_in_parallel() {
        let engine = CalendarEngine::new();
        let eph = AppState::default().ephemeris(None).unwrap();
        let params = CalendarCompareParams {
            hijri_year: 1446,
            visibility: None,
            builtin_only: false,
            ephemeris_id: None,
        };
        let c = complete(
            &engine,
            &eph,
            None,
            None,
            || engine.compare(&*eph, &params),
            |r| &r.missing,
        )
        .unwrap();
        assert!(c.missing.is_empty());
        assert!(c.rows.iter().all(|r| r.starts.iter().all(Option::is_some)));
        let m = complete(
            &engine,
            &eph,
            None,
            None,
            || {
                engine.month(
                    &*eph,
                    &CalendarMonthParams {
                        calendar: CalendarSpec {
                            id: "rule-conjunction-before-noon-ut".into(),
                            visibility: None,
                        },
                        primary: PrimaryCalendar::Hijri,
                        month: Some(MonthRef {
                            year: 1447,
                            month: 1,
                        }),
                        date: None,
                        week_start: None,
                        ephemeris_id: None,
                    },
                )
            },
            |r| &r.missing,
        )
        .unwrap();
        assert!(m.missing.is_empty() && !m.weeks.is_empty());
    }
}
