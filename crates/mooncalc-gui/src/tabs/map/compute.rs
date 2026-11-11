//! Map computation on worker threads: the visibility grid for one date
//! ([`compute_map`]) and the earliest-sighting scan ([`compute_earliest`]),
//! both spread over all cores with [`parallel_rows`].

use std::time::{Duration, Instant as Clock};

use mooncalc_core::Ephemeris;
use mooncalc_core::events::CalendarDate;
use mooncalc_core::events::phases::SYNODIC_MONTH_DAYS;
use mooncalc_core::time::Instant;
use mooncalc_maps::engine::{
    EarliestScan, EngineContext, FirstSighting, RowBlock, ValueField, conjunction_near,
    evaluate_rows, field_thresholds, scan_dates,
};
use mooncalc_maps::grid::VisibilityGrid;
use mooncalc_visibility::ZoneInfo;
use mooncalc_visibility::circumstances::{Sighting, Status};
use mooncalc_visibility::criteria::by_id;
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

/// A whole-world grid at `step_deg`.
pub fn world_grid(step_deg: f64) -> VisibilityGrid {
    let rows = (180.0 / step_deg).round() as usize;
    VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, rows, 2 * rows)
}

pub fn sighting(c: Crescent) -> Sighting {
    match c {
        Crescent::Evening => Sighting::Evening,
        Crescent::Morning => Sighting::Morning,
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

/// Number of [`Status`] variants.
pub const STATUS_COUNT: usize = 8;

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
    /// The new Moon nearest the map's evenings (mornings).
    pub conjunction: Instant,
    /// Cells per zone index.
    pub zone_counts: Vec<usize>,
    /// Cells without a zone, per [`Status`] (indexed by `status as usize`).
    pub status_counts: [usize; STATUS_COUNT],
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
}

/// Compute the map for `spec` on all cores.
pub fn compute_map(
    spec: MapSpec,
    eph: &dyn Ephemeris,
    job: &JobContext,
) -> Result<ComputedMap, JobError> {
    let started = Clock::now();
    let criterion = by_id(&spec.criterion_id)
        .ok_or_else(|| JobError::Failed(format!("unknown criterion {:?}", spec.criterion_id)))?;
    let template = world_grid(spec.resolution.step_deg());
    let ctx = EngineContext::new(
        eph,
        criterion.as_ref(),
        &template,
        spec.date,
        sighting(spec.crescent),
    );
    job.checkpoint()?;
    let blocks = parallel_rows(job, ctx.rows(), |row| eval_row(&ctx, row, job))?;
    let mut grid = template;
    let mut field = ValueField::new(grid.rows, grid.cols, field_thresholds(criterion.as_ref()));
    for block in blocks {
        let block = block?;
        grid.apply_rows(&block);
        field.apply_rows(&block);
    }

    let zones = criterion.zones();
    let mut zone_counts = vec![0; zones.len()];
    let mut status_counts = [0; STATUS_COUNT];
    for (z, s) in grid.cells.iter().zip(&field.status) {
        match z {
            Some(z) if (*z as usize) < zones.len() => zone_counts[*z as usize] += 1,
            _ => status_counts[*s as usize] += 1,
        }
    }
    let mid = spec.date.jd0() + 1.0;
    let conjunction = ctx
        .new_moons()
        .iter()
        .copied()
        .min_by(|a, b| (a.jd_ut - mid).abs().total_cmp(&(b.jd_ut - mid).abs()))
        .unwrap_or_else(|| Instant::from_jd_ut(mid));
    Ok(ComputedMap {
        criterion_name: criterion.name().to_owned(),
        zones,
        grid,
        field,
        conjunction,
        zone_counts,
        status_counts,
        elapsed: started.elapsed(),
        spec,
    })
}

fn eval_row(ctx: &EngineContext<'_>, row: usize, job: &JobContext) -> Result<RowBlock, JobError> {
    evaluate_rows(ctx, row..row + 1, |_, _| !job.is_cancelled()).map_err(|_| JobError::Cancelled)
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

/// Result of the earliest-sighting scan.
#[derive(Clone, Debug)]
pub struct EarliestResult {
    pub spec: EarliestSpec,
    pub conjunction: Instant,
    pub zones: &'static [ZoneInfo],
    pub dates: Vec<CalendarDate>,
    /// Per zone (except the last, "not visible"), the first sighting.
    pub first: Vec<Option<FirstSighting>>,
}

/// The first new Moon at or after `jd_ut`.
pub fn next_new_moon(eph: &dyn Ephemeris, jd_ut: f64) -> Instant {
    let c = conjunction_near(eph, Instant::from_jd_ut(jd_ut + 0.5 * SYNODIC_MONTH_DAYS));
    if c.jd_ut < jd_ut {
        conjunction_near(eph, c.add_days(SYNODIC_MONTH_DAYS))
    } else {
        c
    }
}

/// Run the earliest-sighting scan (evening crescent) for the lunation
/// starting near `spec.date`, evaluating every evening's rows in parallel.
pub fn compute_earliest(
    spec: EarliestSpec,
    eph: &dyn Ephemeris,
    job: &JobContext,
) -> Result<EarliestResult, JobError> {
    let criterion = by_id(&spec.criterion_id)
        .ok_or_else(|| JobError::Failed(format!("unknown criterion {:?}", spec.criterion_id)))?;
    let conjunction = next_new_moon(eph, spec.date.jd0() - 3.0);
    let dates = scan_dates(conjunction, EARLIEST_EVENINGS);
    let template = world_grid(spec.step_deg);
    let contexts: Vec<EngineContext<'_>> = dates
        .iter()
        .map(|&d| EngineContext::new(eph, criterion.as_ref(), &template, d, Sighting::Evening))
        .collect();
    let rows = template.rows;
    let blocks = parallel_rows(job, rows * dates.len(), |i| {
        eval_row(&contexts[i / rows], i % rows, job)
    })?;
    let mut scan = EarliestScan::new(criterion.as_ref());
    let mut blocks = blocks.into_iter();
    for &date in &dates {
        let mut grid = template.clone();
        let mut field = ValueField::new(grid.rows, grid.cols, None);
        for block in blocks.by_ref().take(rows) {
            let block = block?;
            grid.apply_rows(&block);
            field.apply_rows(&block);
        }
        scan.accumulate(date, &grid, &field);
    }
    Ok(EarliestResult {
        spec,
        conjunction,
        zones: criterion.zones(),
        dates,
        first: scan.results().to_vec(),
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
            map.zone_counts.iter().sum::<usize>() + map.status_counts.iter().sum::<usize>(),
            map.cells()
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
        assert_eq!(r.first.len(), 5);
        let a = r.first[0].unwrap();
        assert_eq!(a.date, CalendarDate::new(2024, 3, 10));
        assert!(a.lon_deg < -30.0, "{a:?}");
    }

    #[test]
    fn next_new_moon_is_at_or_after() {
        let eph = MeeusEphemeris::new();
        let t = CalendarDate::new(2024, 3, 11).jd0();
        let c = next_new_moon(&eph, t);
        assert!(c.jd_ut >= t && c.jd_ut < t + SYNODIC_MONTH_DAYS);
        assert_eq!(c.to_calendar().month, 4); // 2024-04-08
    }
}
