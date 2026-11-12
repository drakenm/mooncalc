//! Map computation on worker threads: the visibility grid for one date
//! ([`compute_map`]) and the earliest-sighting scan ([`compute_earliest`]).
//!
//! Both use the shared services, `mooncalc_app::map::MapJob` and
//! `mooncalc_app::earliest::EarliestJob` (validation, the engine context,
//! assembly, counts, the scan), as the Tauri frontend does. Only the
//! threading is egui's: rows are spread over all cores with
//! [`parallel_rows`], one row at a time so cancellation and progress stay
//! responsive.

use std::time::{Duration, Instant as Clock};

use mooncalc_app::dto::{
    ApiError, CivilDate, EarliestParams, EarliestResult as EarliestDto, MapParams, MapTiming,
    StatusCounts,
};
use mooncalc_app::earliest::{EarliestJob, next_new_moon};
use mooncalc_app::map::MapJob;
use mooncalc_core::Ephemeris;
use mooncalc_core::events::CalendarDate;
use mooncalc_core::time::Instant;
use mooncalc_maps::engine::{RowBlock, ValueField};
use mooncalc_maps::grid::VisibilityGrid;
use mooncalc_visibility::ZoneInfo;
use mooncalc_visibility::circumstances::Status;
use serde::{Deserialize, Serialize};

use crate::jobs::{JobContext, JobError, parallel_rows};
use crate::settings::Crescent;

/// Grid resolution of the world map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Resolution {
    Deg2,
    #[default]
    Deg1,
    Deg05,
    Deg025,
}

impl Resolution {
    pub const ALL: [Resolution; 4] = [
        Resolution::Deg2,
        Resolution::Deg1,
        Resolution::Deg05,
        Resolution::Deg025,
    ];

    pub fn step_deg(self) -> f64 {
        match self {
            Resolution::Deg2 => 2.0,
            Resolution::Deg1 => 1.0,
            Resolution::Deg05 => 0.5,
            Resolution::Deg025 => 0.25,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Resolution::Deg2 => "2°",
            Resolution::Deg1 => "1°",
            Resolution::Deg05 => "0.5°",
            Resolution::Deg025 => "0.25°",
        }
    }
}

/// The wire form of the crescent setting.
pub fn app_crescent(c: Crescent) -> mooncalc_app::dto::Crescent {
    match c {
        Crescent::Evening => mooncalc_app::dto::Crescent::Evening,
        Crescent::Morning => mooncalc_app::dto::Crescent::Morning,
    }
}

/// What to compute. Two equal specs give the same map.
#[derive(Clone, Debug, PartialEq)]
pub struct MapSpec {
    /// Each cell's local calendar date.
    pub date: CalendarDate,
    pub crescent: Crescent,
    pub criterion_id: String,
    pub resolution: Resolution,
    /// The ephemeris' name (identifies the backend instance).
    pub ephemeris: String,
}

impl MapSpec {
    /// The service request: the whole world at the spec's resolution.
    pub fn params(&self) -> MapParams {
        MapParams {
            date: self.date.into(),
            criterion_id: self.criterion_id.clone(),
            crescent: app_crescent(self.crescent),
            resolution_deg: self.resolution.step_deg(),
            bounds: None,
            ephemeris_id: None,
            include_values: false,
        }
    }
}

/// Short description of a cell status without a crescent.
pub fn status_text(s: Status, crescent: Crescent) -> &'static str {
    let evening = crescent == Crescent::Evening;
    match s {
        Status::Crescent => "Crescent",
        Status::BeforeConjunction if evening => "Before conjunction at best time",
        Status::BeforeConjunction => "After conjunction at best time",
        Status::MoonSetsFirst if evening => "Moon sets before the Sun",
        Status::MoonSetsFirst => "Moon rises after the Sun",
        Status::MoonAlwaysUp => "Moon does not set/rise near the Sun event",
        Status::MoonAlwaysDown => "Moon below the horizon all day",
        Status::MidnightSun => "Polar day (no sunset/sunrise)",
        Status::PolarNight => "Polar night (Sun stays down)",
        Status::SunAltitudeNotReached => "Sun never reaches the evaluation altitude",
        Status::OutsideEphemeris => "Outside the ephemeris coverage",
    }
}

/// A computed visibility map.
#[derive(Clone, Debug)]
pub struct ComputedMap {
    pub spec: MapSpec,
    pub criterion_name: String,
    pub zones: &'static [ZoneInfo],
    pub grid: VisibilityGrid,
    pub field: ValueField,
    /// The new Moon nearest the map's date.
    pub conjunction: Instant,
    /// Cells per zone index.
    pub zone_counts: Vec<u32>,
    /// Cells per status; `crescent` counts the cells with a zone.
    pub status_counts: StatusCounts,
    pub elapsed: Duration,
}

/// What the cursor is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellInfo {
    pub row: usize,
    pub col: usize,
    pub zone: Option<u8>,
    pub value: Option<f32>,
    pub status: Status,
}

impl ComputedMap {
    pub fn cells(&self) -> usize {
        self.grid.rows * self.grid.cols
    }

    /// The grid cell containing a geographic position.
    pub fn cell_at(&self, lat_deg: f64, lon_deg: f64) -> Option<CellInfo> {
        let g = &self.grid;
        let dlat = (g.lat_max_deg - g.lat_min_deg) / g.rows as f64;
        let dlon = (g.lon_max_deg - g.lon_min_deg) / g.cols as f64;
        let lon = g.lon_min_deg + (lon_deg - g.lon_min_deg).rem_euclid(360.0);
        let row = ((g.lat_max_deg - lat_deg) / dlat).floor();
        let col = ((lon - g.lon_min_deg) / dlon).floor();
        if !(row >= 0.0 && col >= 0.0) {
            return None;
        }
        let (row, col) = (
            (row as usize).min(g.rows - 1),
            (col as usize).min(g.cols - 1),
        );
        Some(CellInfo {
            row,
            col,
            zone: g.get(row, col),
            value: self.field.get(row, col),
            status: self.field.status(row, col),
        })
    }

    /// The `(code, description)` of a zone index.
    pub fn zone_info(&self, zone: u8) -> Option<&'static ZoneInfo> {
        self.zones.get(zone as usize)
    }

    /// Cells without a zone, per status, where there are any.
    pub fn unzoned_counts(&self) -> impl Iterator<Item = (Status, u32)> + '_ {
        self.status_counts
            .by_status()
            .into_iter()
            .filter(|&(s, n)| s != Status::Crescent && n > 0)
    }
}

fn api_error(e: ApiError) -> JobError {
    if e.code == "cancelled" {
        JobError::Cancelled
    } else {
        JobError::Failed(e.message)
    }
}

/// Evaluate rows `0..rows` of the given jobs on all cores, one row per work
/// item: item `i` is row `i % rows` of `jobs[i / rows]`. Returns each job's
/// blocks in order.
fn evaluate_parallel(jobs: &[MapJob], job: &JobContext) -> Result<Vec<Vec<RowBlock>>, JobError> {
    let Some(rows) = jobs.first().map(MapJob::rows) else {
        return Ok(Vec::new());
    };
    let blocks = parallel_rows(job, rows * jobs.len(), |i| {
        let row = i % rows;
        jobs[i / rows]
            .evaluate_rows_with(row..row + 1, |_, _| !job.is_cancelled())
            .map_err(api_error)
    })?;
    let mut blocks = blocks.into_iter();
    let mut out = Vec::with_capacity(jobs.len());
    for _ in jobs {
        out.push(blocks.by_ref().take(rows).collect::<Result<Vec<_>, _>>()?);
    }
    Ok(out)
}

/// Compute the map for `spec` on all cores.
pub fn compute_map(
    spec: MapSpec,
    eph: &dyn Ephemeris,
    job: &JobContext,
) -> Result<ComputedMap, JobError> {
    let started = Clock::now();
    let map_job = MapJob::prepare(eph, &spec.params()).map_err(api_error)?;
    job.checkpoint()?;
    let blocks = evaluate_parallel(std::slice::from_ref(&map_job), job)?;
    let out = map_job.assemble(blocks.into_iter().flatten());
    let criterion = map_job.criterion();
    Ok(ComputedMap {
        criterion_name: criterion.name().to_owned(),
        zones: criterion.zones(),
        zone_counts: out.zone_counts(),
        status_counts: out.status_counts(),
        conjunction: out.conjunction,
        grid: out.grid,
        field: out.field,
        elapsed: started.elapsed(),
        spec,
    })
}

/// What the earliest-sighting scan depends on.
#[derive(Clone, Debug, PartialEq)]
pub struct EarliestSpec {
    /// The scan covers the first new Moon after this date minus 3 days.
    pub date: CalendarDate,
    pub criterion_id: String,
    pub step_deg: f64,
    pub ephemeris: String,
}

/// Evenings scanned after the conjunction (the scan starts the day before).
pub const EARLIEST_EVENINGS: u32 = 4;

/// Days before the spec's date from which the next new Moon is scanned.
pub const EARLIEST_LOOK_BACK_DAYS: f64 = 3.0;

/// Result of the earliest-sighting scan.
#[derive(Clone, Debug)]
pub struct EarliestResult {
    pub spec: EarliestSpec,
    /// The service result: the conjunction, the dates scanned and, per zone
    /// (except the last, "not visible"), the first sighting.
    pub scan: EarliestDto,
}

impl EarliestResult {
    pub fn conjunction(&self) -> Instant {
        self.scan.conjunction.to_instant()
    }

    pub fn dates(&self) -> impl Iterator<Item = CalendarDate> + '_ {
        self.scan.dates_scanned.iter().map(|&d| d.into())
    }
}

/// Run the earliest-sighting scan (evening crescent) for the first new
/// Moon after `spec.date` minus [`EARLIEST_LOOK_BACK_DAYS`], evaluating
/// every evening's rows in parallel.
pub fn compute_earliest(
    spec: EarliestSpec,
    eph: &dyn Ephemeris,
    job: &JobContext,
) -> Result<EarliestResult, JobError> {
    let conjunction = next_new_moon(eph, spec.date.jd0() - EARLIEST_LOOK_BACK_DAYS);
    let params = EarliestParams {
        date: CivilDate::from(spec.date),
        criterion_id: spec.criterion_id.clone(),
        resolution_deg: Some(spec.step_deg),
        evenings: Some(EARLIEST_EVENINGS),
        ephemeris_id: None,
    };
    let mut scan = EarliestJob::with_conjunction(&params, conjunction).map_err(api_error)?;
    let maps = scan
        .dates()
        .iter()
        .map(|&d| MapJob::prepare(eph, &scan.map_params(d)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(api_error)?;
    job.checkpoint()?;
    let blocks = evaluate_parallel(&maps, job)?;
    for (map, blocks) in maps.iter().zip(blocks) {
        scan.accumulate(&map.assemble(blocks));
    }
    Ok(EarliestResult {
        spec,
        scan: scan.result(MapTiming::default()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::MeeusEphemeris;

    fn spec(res: Resolution) -> MapSpec {
        MapSpec {
            date: CalendarDate::new(2024, 3, 10),
            crescent: Crescent::Evening,
            criterion_id: "yallop".into(),
            resolution: res,
            ephemeris: "meeus".into(),
        }
    }

    #[test]
    fn march_2024_evening_is_visible_in_the_americas_not_asia() {
        let eph = MeeusEphemeris::new();
        let map = compute_map(spec(Resolution::Deg2), &eph, &JobContext::detached()).unwrap();
        assert_eq!(map.cells(), 90 * 180);
        assert_eq!(
            map.zone_counts.iter().sum::<u32>() + map.unzoned_counts().map(|(_, n)| n).sum::<u32>(),
            map.cells() as u32
        );
        assert_eq!(
            map.zone_counts.iter().sum::<u32>(),
            map.status_counts.crescent
        );
        // Conjunction 2024-03-10 09:00 UT.
        let c = map.conjunction.to_calendar();
        assert_eq!((c.year, c.month, c.day, c.hour), (2024, 3, 10, 9));
        // Easily visible (A) or visible in perfect conditions (B) in the
        // central Pacific, worse towards the Americas, F or nothing in
        // South-East Asia (as on HMNAO's map for that evening).
        let pacific = map.cell_at(15.0, -165.0).unwrap();
        assert!(pacific.zone.is_some_and(|z| z <= 1), "{pacific:?}");
        let americas = map.cell_at(10.0, -80.0).unwrap();
        assert!(
            americas.zone.is_some_and(|z| (2..=4).contains(&z)),
            "{americas:?}"
        );
        let asia = map.cell_at(15.0, 100.0).unwrap();
        assert!(asia.zone.is_none_or(|z| z >= 4), "{asia:?}");
    }

    /// The threaded rows give the same map as the service on one thread.
    #[test]
    fn parallel_rows_match_the_service() {
        let eph = MeeusEphemeris::new();
        let s = spec(Resolution::Deg2);
        let map = compute_map(s.clone(), &eph, &JobContext::detached()).unwrap();
        let serial = MapJob::prepare(&eph, &s.params()).unwrap().evaluate();
        assert_eq!(map.grid, serial.grid);
        assert_eq!(map.field.status, serial.field.status);
        assert_eq!(map.status_counts, serial.status_counts());
    }

    #[test]
    fn cell_lookup_wraps_longitude() {
        let eph = MeeusEphemeris::new();
        let map = compute_map(spec(Resolution::Deg2), &eph, &JobContext::detached()).unwrap();
        let a = map.cell_at(0.5, 179.5).unwrap();
        let b = map.cell_at(0.5, -180.5).unwrap();
        assert_eq!((a.row, a.col), (b.row, b.col));
        assert_eq!(map.cell_at(90.0, 0.0).unwrap().row, 0);
        assert_eq!(map.cell_at(-90.0, 0.0).unwrap().row, 89);
    }

    #[test]
    fn earliest_sighting_finds_zone_a_on_march_10_in_the_americas() {
        let eph = MeeusEphemeris::new();
        let r = compute_earliest(
            EarliestSpec {
                date: CalendarDate::new(2024, 3, 9),
                criterion_id: "yallop".into(),
                step_deg: 5.0,
                ephemeris: "meeus".into(),
            },
            &eph,
            &JobContext::detached(),
        )
        .unwrap();
        assert_eq!(r.scan.zones.len(), 5);
        let a = r.scan.zones[0].first.as_ref().unwrap();
        assert_eq!(CalendarDate::from(a.date), CalendarDate::new(2024, 3, 10));
        assert!(a.lon_deg < -30.0, "{a:?}");
        let dates: Vec<_> = r.dates().collect();
        assert_eq!(dates.len(), EARLIEST_EVENINGS as usize);
        assert_eq!(dates[0], CalendarDate::new(2024, 3, 9));
        // A few days after the new Moon the scan still covers it.
        let later = compute_earliest(
            EarliestSpec {
                date: CalendarDate::new(2024, 3, 12),
                ..r.spec.clone()
            },
            &eph,
            &JobContext::detached(),
        )
        .unwrap();
        assert_eq!(later.conjunction(), r.conjunction());
    }
}
