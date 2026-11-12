//! The grid engine: evaluates a criterion over a [`VisibilityGrid`] for one
//! date, plus the earliest-sighting scan.
//!
//! # Cell semantics
//!
//! Each cell centre is evaluated as a sea-level observer on **its own local
//! evening (or morning) of the given calendar date**, the HMNAO/MoonCalc
//! convention:
//!
//! * The day is anchored at 12h **local mean time**,
//!   `UT = date 0h + 12h − λ/15°`, with the longitude first reduced to
//!   `[−180°, 180°)`. The calendar date line is therefore the 180° meridian:
//!   a cell at 179° E has its evening around 06h UT on the date, one at
//!   179° W around 06h UT on the next day.
//! * The Sun's upper transit nearest that anchor starts the evening; sunset
//!   is the first one in the following 24 h, moonset the one nearest it,
//!   and the criterion's evaluation time follows (see
//!   [`mooncalc_visibility::circumstances`]).
//!
//! The UT span covered by a map is therefore about 0h UT on the date to 0h
//! UT two days later; the engine tabulates the ephemeris from one day
//! before to three days after 0h UT on the date.
//!
//! # Map date versus civil date
//!
//! A local observer's civil date agrees with this rule wherever their zone
//! offset is within 12 h of mean solar time (`λ/15°`). East of 180° in
//! UTC+13/+14 (Samoa, Tonga, the Line Islands) the civil date runs a day
//! ahead: the map for 10 March shows their civil evening of 11 March, and
//! their civil 10 March is on the map for 9 March.
//! [`CalendarDate::mean_time_date`] gives the map date for a civil date and
//! zone, [`CalendarDate::civil_date_of_mean_time`] the inverse;
//! `mooncalc_app::dateline` words the notes the frontends show.
//!
//! # Performance
//!
//! [`EngineContext::new`] evaluates the ephemeris once on a 30-minute table
//! ([`ApparentTable`], interpolation error below 0.001″) and finds the
//! conjunctions. Each cell then only does topocentric and horizontal vector
//! algebra on interpolated positions: about a dozen evaluations for the
//! Sun's transit, sunset, moonset and the geometry, using the fast local
//! solver with the robust bracketing search as fallback (polar edge cases).
//!
//! The engine is single-threaded and portable. Callers parallelise over row
//! ranges with [`evaluate_rows`] (the egui GUI's `jobs::parallel_rows`, web
//! workers on WASM) and merge the blocks with [`VisibilityGrid::apply_rows`]
//! and [`ValueField::apply_rows`]. [`evaluate_map`] does the whole grid on
//! the calling thread.

use std::fmt;
use std::ops::Range;

use mooncalc_core::GeoLocation;
use mooncalc_core::ephemeris::Ephemeris;
use mooncalc_core::events::phases::{Phase, phase_near};
use mooncalc_core::events::{ApparentTable, CalendarDate};
use mooncalc_core::time::Instant;
use mooncalc_visibility::Criterion;
use mooncalc_visibility::circumstances::{
    Request, Settings, Sighting, Status, compute_with, new_moons_around,
};
use mooncalc_visibility::criteria::{EvaluationTime, odeh, yallop};

use crate::grid::VisibilityGrid;
use crate::render::ZoneField;

/// Evaluation was cancelled by the progress callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("grid evaluation cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// Everything shared by the cells of one map: tabulated positions,
/// conjunctions, the criterion and the grid geometry. `Sync`, so worker
/// threads can share one context.
pub struct EngineContext<'a> {
    criterion: &'a dyn Criterion,
    evaluation: EvaluationTime,
    date: CalendarDate,
    sighting: Sighting,
    settings: Settings,
    table: ApparentTable,
    new_moons: Vec<Instant>,
    grid: VisibilityGrid,
}

/// The result for one cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellResult {
    /// Zone index, `None` where there is no crescent to look for.
    pub zone: Option<u8>,
    /// The criterion's continuous value (Yallop q, Odeh V, ...) or NaN.
    pub value: f32,
    pub status: Status,
}

impl<'a> EngineContext<'a> {
    /// Prepare a map of `criterion` on `date` over `grid`'s bounds and
    /// resolution (its cells are not read), with default [`Settings`].
    pub fn new(
        eph: &dyn Ephemeris,
        criterion: &'a dyn Criterion,
        grid: &VisibilityGrid,
        date: CalendarDate,
        sighting: Sighting,
    ) -> Self {
        Self::with_settings(eph, criterion, grid, date, sighting, Settings::default())
    }

    /// As [`Self::new`], with explicit settings (horizon, atmosphere).
    pub fn with_settings(
        eph: &dyn Ephemeris,
        criterion: &'a dyn Criterion,
        grid: &VisibilityGrid,
        date: CalendarDate,
        sighting: Sighting,
        settings: Settings,
    ) -> Self {
        let jd0 = date.jd0();
        let table = ApparentTable::with_default_step(
            eph,
            Instant::from_jd_ut(jd0 - 1.0),
            Instant::from_jd_ut(jd0 + 3.0),
        );
        let new_moons = new_moons_around(eph, Instant::from_jd_ut(jd0 + 1.0));
        let mut shape = grid.clone();
        shape.cells = Vec::new();
        Self {
            criterion,
            evaluation: criterion.evaluation_time(),
            date,
            sighting,
            settings,
            table,
            new_moons,
            grid: shape,
        }
    }

    pub fn rows(&self) -> usize {
        self.grid.rows
    }

    pub fn cols(&self) -> usize {
        self.grid.cols
    }

    pub fn date(&self) -> CalendarDate {
        self.date
    }

    pub fn criterion(&self) -> &dyn Criterion {
        self.criterion
    }

    /// The conjunctions used for the Moon's age (nearest one per cell).
    pub fn new_moons(&self) -> &[Instant] {
        &self.new_moons
    }

    /// Evaluate one cell.
    pub fn evaluate_cell(&self, row: usize, col: usize) -> CellResult {
        let (lat, lon) = self.grid.cell_center(row, col);
        let req = Request::local_date(
            GeoLocation::new(lat, lon, 0.0),
            self.date,
            self.sighting,
            self.evaluation,
        );
        let o = compute_with(&self.table, &self.new_moons, &req, &self.settings);
        let r = o.evaluate(self.criterion);
        CellResult {
            zone: r.map(|r| r.zone as u8),
            value: r.and_then(|r| r.value).map_or(f32::NAN, |v| v as f32),
            status: o.status(),
        }
    }
}

/// Results for a contiguous range of rows, row-major.
#[derive(Clone, Debug, PartialEq)]
pub struct RowBlock {
    pub rows: Range<usize>,
    pub cols: usize,
    pub zones: Vec<Option<u8>>,
    pub values: Vec<f32>,
    pub status: Vec<Status>,
}

/// Evaluate rows `rows` of the context's grid. `progress(done, total)` is
/// called after each row with the rows finished in this call; returning
/// `false` cancels.
pub fn evaluate_rows(
    ctx: &EngineContext<'_>,
    rows: Range<usize>,
    mut progress: impl FnMut(usize, usize) -> bool,
) -> Result<RowBlock, Cancelled> {
    assert!(rows.end <= ctx.rows());
    let cols = ctx.cols();
    let n = rows.len() * cols;
    let mut block = RowBlock {
        rows: rows.clone(),
        cols,
        zones: Vec::with_capacity(n),
        values: Vec::with_capacity(n),
        status: Vec::with_capacity(n),
    };
    for (done, row) in rows.clone().enumerate() {
        for col in 0..cols {
            let c = ctx.evaluate_cell(row, col);
            block.zones.push(c.zone);
            block.values.push(c.value);
            block.status.push(c.status);
        }
        if !progress(done + 1, rows.len()) {
            return Err(Cancelled);
        }
    }
    Ok(block)
}

/// The continuous companion of a [`VisibilityGrid`]: the criterion's value
/// and the circumstance status per cell, same layout as
/// [`VisibilityGrid::cells`].
#[derive(Clone, Debug, PartialEq)]
pub struct ValueField {
    pub rows: usize,
    pub cols: usize,
    /// Criterion value per cell; NaN where there is none (no crescent, or a
    /// criterion without a value). This is the layout
    /// [`ZoneField::values`] expects.
    pub values: Vec<f32>,
    pub status: Vec<Status>,
    /// Descending zone thresholds of the value, when the criterion's zones
    /// are thresholds of its value (see [`field_thresholds`]).
    pub thresholds: Option<Vec<f32>>,
}

impl ValueField {
    /// An empty field (all NaN, status [`Status::MoonSetsFirst`]).
    pub fn new(rows: usize, cols: usize, thresholds: Option<Vec<f32>>) -> Self {
        Self {
            rows,
            cols,
            values: vec![f32::NAN; rows * cols],
            status: vec![Status::MoonSetsFirst; rows * cols],
            thresholds,
        }
    }

    /// The value of a cell, if any.
    pub fn get(&self, row: usize, col: usize) -> Option<f32> {
        let v = self.values[row * self.cols + col];
        v.is_finite().then_some(v)
    }

    pub fn status(&self, row: usize, col: usize) -> Status {
        self.status[row * self.cols + col]
    }

    /// Copy a block's values and statuses in.
    pub fn apply_rows(&mut self, block: &RowBlock) {
        assert_eq!(block.cols, self.cols);
        let r = block.rows.start * self.cols..block.rows.end * self.cols;
        self.values[r.clone()].copy_from_slice(&block.values);
        self.status[r].copy_from_slice(&block.status);
    }

    /// The renderer's smooth-contour input, when thresholds are known.
    pub fn zone_field(&self) -> Option<ZoneField<'_>> {
        self.thresholds.as_deref().map(|thresholds| ZoneField {
            values: &self.values,
            thresholds,
        })
    }
}

impl VisibilityGrid {
    /// Copy a block's zones in.
    pub fn apply_rows(&mut self, block: &RowBlock) {
        assert_eq!(block.cols, self.cols);
        let r = block.rows.start * self.cols..block.rows.end * self.cols;
        self.cells[r].copy_from_slice(&block.zones);
    }
}

/// Descending value thresholds that reproduce the criterion's zones, for
/// smooth contouring: Yallop's q bounds (Table 5), Odeh's V bounds, and 0
/// for the two-zone criteria whose value is a margin (visible when ≥ 0).
/// `None` for criteria whose zones are not thresholds of one value.
///
/// Yallop's bounds are exclusive (`q > 0.216`) where the renderer's are
/// inclusive; the difference is a set of measure zero.
pub fn field_thresholds(criterion: &dyn Criterion) -> Option<Vec<f32>> {
    match criterion.id() {
        "yallop" => Some(yallop::Q_BOUNDS.iter().map(|&b| b as f32).collect()),
        "odeh" => Some(odeh::V_BOUNDS.iter().map(|&b| b as f32).collect()),
        "babylonian" => None,
        _ if criterion.zones().len() == 2 => Some(vec![0.0]),
        _ => None,
    }
}

/// A complete map.
#[derive(Clone, Debug, PartialEq)]
pub struct MapResult {
    pub grid: VisibilityGrid,
    pub field: ValueField,
}

/// Evaluate the whole grid on the calling thread. `progress(done, total)`
/// counts rows; return `false` to cancel.
pub fn evaluate_map(
    ctx: &EngineContext<'_>,
    progress: impl FnMut(usize, usize) -> bool,
) -> Result<MapResult, Cancelled> {
    let block = evaluate_rows(ctx, 0..ctx.rows(), progress)?;
    let mut grid = ctx.grid.clone();
    grid.cells = vec![None; ctx.rows() * ctx.cols()];
    grid.apply_rows(&block);
    let mut field = ValueField::new(ctx.rows(), ctx.cols(), field_thresholds(ctx.criterion));
    field.apply_rows(&block);
    Ok(MapResult { grid, field })
}

/// Where and when a zone was first reached.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FirstSighting {
    pub date: CalendarDate,
    /// The cell with the best value among those in the zone or better (the
    /// first such cell when the criterion has no value).
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub value: Option<f32>,
    /// How many cells reach the zone or better on that date.
    pub cells: usize,
}

/// Accumulates the earliest-sighting scan over successive evenings: for
/// every zone except the last ("not visible"), the first date on which any
/// cell reaches that zone or a better one, and where.
#[derive(Clone, Debug, PartialEq)]
pub struct EarliestScan {
    first: Vec<Option<FirstSighting>>,
}

impl EarliestScan {
    pub fn new(criterion: &dyn Criterion) -> Self {
        Self {
            first: vec![None; criterion.zones().len().saturating_sub(1)],
        }
    }

    /// Fold in one evaluated date (dates in increasing order).
    pub fn accumulate(&mut self, date: CalendarDate, grid: &VisibilityGrid, field: &ValueField) {
        for (zone, slot) in self.first.iter_mut().enumerate() {
            if slot.is_some() {
                continue;
            }
            let mut best: Option<(usize, f32)> = None;
            let mut count = 0;
            for (i, z) in grid.cells.iter().enumerate() {
                if z.is_some_and(|z| z as usize <= zone) {
                    count += 1;
                    let v = field.values[i];
                    let better = match best {
                        None => true,
                        Some((_, bv)) => v > bv || (bv.is_nan() && !v.is_nan()),
                    };
                    if better {
                        best = Some((i, v));
                    }
                }
            }
            if let Some((i, v)) = best {
                let (lat, lon) = grid.cell_center(i / grid.cols, i % grid.cols);
                *slot = Some(FirstSighting {
                    date,
                    lat_deg: lat,
                    lon_deg: lon,
                    value: v.is_finite().then_some(v),
                    cells: count,
                });
            }
        }
    }

    /// Per zone (index = zone), the first sighting found so far.
    pub fn results(&self) -> &[Option<FirstSighting>] {
        &self.first
    }

    /// Whether every zone has been reached.
    pub fn is_complete(&self) -> bool {
        self.first.iter().all(Option::is_some)
    }
}

/// The evening dates the scan covers for a conjunction: starting the UT day
/// before the conjunction's UT date (whose evening at the 180° meridian
/// falls after 06h UT on the conjunction date), `evenings` dates in all.
pub fn scan_dates(conjunction: Instant, evenings: u32) -> Vec<CalendarDate> {
    let first = CalendarDate::from_jd(conjunction.jd_ut).add_days(-1);
    (0..evenings as i32).map(|k| first.add_days(k)).collect()
}

/// The new Moon nearest to `t`, for [`earliest_sighting`].
pub fn conjunction_near(eph: &dyn Ephemeris, t: Instant) -> Instant {
    phase_near(eph, Phase::New, t)
}

/// Single-threaded earliest-sighting scan: evaluate `template`'s grid on
/// each evening of [`scan_dates`] and return, per zone, the first sighting.
/// Stops early once every zone is reached. `progress(done, total)` counts
/// evaluated rows over all dates; return `false` to cancel.
pub fn earliest_sighting(
    eph: &dyn Ephemeris,
    criterion: &dyn Criterion,
    template: &VisibilityGrid,
    conjunction: Instant,
    evenings: u32,
    mut progress: impl FnMut(usize, usize) -> bool,
) -> Result<Vec<Option<FirstSighting>>, Cancelled> {
    let dates = scan_dates(conjunction, evenings);
    let total = dates.len() * template.rows;
    let mut scan = EarliestScan::new(criterion);
    for (k, date) in dates.into_iter().enumerate() {
        let ctx = EngineContext::new(eph, criterion, template, date, Sighting::Evening);
        let map = evaluate_map(&ctx, |d, _| progress(k * template.rows + d, total))?;
        scan.accumulate(date, &map.grid, &map.field);
        if scan.is_complete() {
            break;
        }
    }
    Ok(scan.results().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::MeeusEphemeris;
    use mooncalc_core::events::DirectSource;
    use mooncalc_visibility::circumstances::{Outcome, compute};
    use mooncalc_visibility::criteria::{Danjon, Odeh, Saao, Yallop};

    fn world(step_deg: f64) -> VisibilityGrid {
        let rows = (180.0 / step_deg) as usize;
        VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, rows, 2 * rows)
    }

    #[test]
    fn cells_match_direct_circumstances() {
        // The table-driven engine agrees with circumstances::compute (its
        // own table) and with a direct-ephemeris source.
        let eph = MeeusEphemeris::new();
        let grid = world(15.0);
        let date = CalendarDate::new(2024, 3, 11);
        let ctx = EngineContext::new(&eph, &Yallop, &grid, date, Sighting::Evening);
        let direct = DirectSource::new(&eph);
        let mut crescents = 0;
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let cell = ctx.evaluate_cell(row, col);
                let (lat, lon) = grid.cell_center(row, col);
                let req = Request::local_date(
                    GeoLocation::new(lat, lon, 0.0),
                    date,
                    Sighting::Evening,
                    EvaluationTime::YallopBestTime,
                );
                let o = compute(&eph, &req, &Settings::default());
                assert_eq!(cell.status, o.status(), "{lat} {lon}");
                let d = compute_with(&direct, ctx.new_moons(), &req, &Settings::default());
                assert_eq!(d.status(), o.status());
                if let (Outcome::Crescent(a), Outcome::Crescent(b)) = (o, d) {
                    crescents += 1;
                    let dq = yallop::q(a.geometry.arcv_deg, a.geometry.width_topo_arcmin)
                        - yallop::q(b.geometry.arcv_deg, b.geometry.width_topo_arcmin);
                    assert!(dq.abs() < 1e-6, "{dq}");
                    assert!((a.sun_event.jd_ut - b.sun_event.jd_ut).abs() * 86_400.0 < 0.2);
                    assert_eq!(
                        cell.zone,
                        Some(yallop::zone_for_q(yallop::q(
                            a.geometry.arcv_deg,
                            a.geometry.width_topo_arcmin
                        )) as u8)
                    );
                }
            }
        }
        assert!(crescents > 50, "{crescents}");
    }

    #[test]
    fn rows_api_merges_into_the_full_map() {
        let eph = MeeusEphemeris::new();
        let grid = world(10.0);
        let ctx = EngineContext::new(
            &eph,
            &Odeh,
            &grid,
            CalendarDate::new(2024, 3, 11),
            Sighting::Evening,
        );
        let full = evaluate_map(&ctx, |_, _| true).unwrap();
        let mut g = grid.clone();
        let mut f = ValueField::new(grid.rows, grid.cols, field_thresholds(&Odeh));
        for r in [0..5, 5..11, 11..18] {
            let b = evaluate_rows(&ctx, r, |_, _| true).unwrap();
            g.apply_rows(&b);
            f.apply_rows(&b);
        }
        assert_eq!(g, full.grid);
        assert_eq!(f.status, full.field.status);
        assert!(
            f.values
                .iter()
                .zip(&full.field.values)
                .all(|(a, b)| a == b || (a.is_nan() && b.is_nan()))
        );
        // The value field classifies to the same zones as the grid.
        let zf = full.field.zone_field().unwrap();
        for (i, z) in full.grid.cells.iter().enumerate() {
            if let Some(z) = z {
                assert_eq!(zf.classify(full.field.values[i]), *z);
            }
        }
        // Cancellation.
        assert_eq!(
            evaluate_rows(&ctx, 0..18, |done, _| done < 3).unwrap_err(),
            Cancelled
        );
    }

    #[test]
    fn rendered_cell_centres_show_the_evaluated_zone() {
        // End to end: at every cell centre the contoured raster must show
        // the zone the criterion returned there (or the "no crescent"
        // colour for `None`), including Moon-always-up cells whose value is
        // lag = +∞ ("moonset after sunset", "minimum lag").
        use crate::projection::Equirectangular;
        use crate::render::{MapData, RenderOptions, render_raster};
        use mooncalc_visibility::criteria::{MinimumLag, MoonsetAfterSunset};
        let eph = MeeusEphemeris::new();
        let grid = world(6.0);
        const NONE: [u8; 3] = [1, 2, 3];
        let criteria: [&dyn Criterion; 5] = [
            &Yallop,
            &Odeh,
            &Danjon::default(),
            &MoonsetAfterSunset,
            &MinimumLag::default(),
        ];
        for criterion in criteria {
            for date in [
                CalendarDate::new(2024, 3, 11),
                CalendarDate::new(2024, 6, 7),
            ] {
                let ctx = EngineContext::new(&eph, criterion, &grid, date, Sighting::Evening);
                let map = evaluate_map(&ctx, |_, _| true).unwrap();
                let palette: Vec<[u8; 3]> = criterion.zones().iter().map(|z| z.rgb).collect();
                let data = MapData {
                    grid: &map.grid,
                    palette: &palette,
                    none_color: Some(NONE),
                    field: map.field.zone_field(),
                };
                assert!(data.field.is_some(), "{}", criterion.id());
                // An odd pixel count puts a pixel centre exactly on each cell centre.
                let (px_per_cell, w, h) = (9, grid.cols as u32 * 9, grid.rows as u32 * 9);
                let opts = RenderOptions {
                    projection: Equirectangular::from_grid(&grid).into(),
                    width: w,
                    height: h,
                    zone_opacity: 1.0,
                    antialias: false,
                    coastlines: None,
                    borders: None,
                    graticule: None,
                    frame: None,
                    ..RenderOptions::default()
                };
                let img = render_raster(&data, &opts);
                for row in 0..grid.rows {
                    for col in 0..grid.cols {
                        let want = map.grid.get(row, col).map_or(NONE, |z| palette[z as usize]);
                        let p = img.pixel(
                            col as u32 * px_per_cell + px_per_cell / 2,
                            row as u32 * px_per_cell + px_per_cell / 2,
                        );
                        assert_eq!(
                            [p[0], p[1], p[2]],
                            want,
                            "{} {date:?} cell {:?} status {:?} value {:?}",
                            criterion.id(),
                            grid.cell_center(row, col),
                            map.field.status(row, col),
                            map.field.values[row * grid.cols + col],
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn thresholds() {
        assert_eq!(field_thresholds(&Yallop).unwrap().len(), 5);
        assert_eq!(field_thresholds(&Danjon::default()), Some(vec![0.0]));
        assert_eq!(field_thresholds(&Saao), None);
    }

    #[test]
    fn date_line_and_window() {
        // Either side of the 180° meridian the evenings are a day apart.
        let eph = MeeusEphemeris::new();
        let grid = VisibilityGrid::new(-10.0, 10.0, 170.0, 190.0, 1, 2);
        let date = CalendarDate::new(2024, 3, 11);
        let ctx = EngineContext::new(&eph, &Yallop, &grid, date, Sighting::Evening);
        let east = Request::local_date(
            GeoLocation::new(0.0, 175.0, 0.0),
            date,
            Sighting::Evening,
            EvaluationTime::Sunset,
        );
        let west = Request::local_date(
            GeoLocation::new(0.0, 185.0, 0.0),
            date,
            Sighting::Evening,
            EvaluationTime::Sunset,
        );
        let s = |r: &Request| {
            compute_with(&ctx.table, ctx.new_moons(), r, &Settings::default())
                .circumstances()
                .unwrap()
                .sun_event
                .jd_ut
        };
        let (e, w) = (s(&east), s(&west));
        assert!((w - e - (1.0 - 10.0 / 360.0)).abs() < 0.01, "{}", w - e);
        assert!(e > date.jd0() + 0.2 && w < date.jd0() + 1.35);
    }

    #[test]
    fn earliest_sighting_march_2024() {
        // New Moon 2024 March 10 09:00 UT. On the evening of March 10 the
        // crescent is only possible in the Americas/Pacific; by March 11 it
        // is easily visible widely. Zone A (easily visible) is first reached
        // on March 10 in the Americas, per HMNAO's map for that evening.
        let eph = MeeusEphemeris::new();
        let conj = conjunction_near(&eph, Instant::from_calendar(2024, 3, 10, 0, 0, 0.0));
        let dates = scan_dates(conj, 4);
        assert_eq!(dates[0], CalendarDate::new(2024, 3, 9));
        let r = earliest_sighting(&eph, &Yallop, &world(5.0), conj, 4, |_, _| true).unwrap();
        assert_eq!(r.len(), 5);
        let a = r[0].unwrap();
        assert_eq!(a.date, CalendarDate::new(2024, 3, 10));
        assert!(a.lon_deg < -30.0, "{a:?}");
        for z in &r {
            assert!(z.unwrap().date <= CalendarDate::new(2024, 3, 10));
        }
    }
}
