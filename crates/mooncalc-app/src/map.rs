//! World visibility maps: request → prepared job → row blocks → result.
//!
//! [`MapJob::prepare`] validates a [`MapParams`] and builds the engine
//! context (`mooncalc_maps::engine::EngineContext`: the tabulated ephemeris
//! and conjunctions). Cells are then evaluated by row ranges with
//! [`MapJob::evaluate_rows`], which is portable and single-threaded: native
//! frontends spread row chunks over threads, the WASM frontend over web
//! workers, and [`MapJob::assemble`] merges the blocks into a [`MapOutput`].
//! [`MapJob::result`] turns that into the wire [`MapResult`]; the output can
//! also be rendered to PNG ([`MapOutput::render_png`]).
//!
//! [`compute_map`] does everything on the calling thread.
//!
//! Cell semantics (each cell on its own local evening, the 180° meridian as
//! the date line, sea-level observers) are those of the engine; see
//! `mooncalc_maps::engine`. [`crate::dateline`] relates a map's date to an
//! observer's civil date.

use std::ops::Range;

use mooncalc_core::Ephemeris;
use mooncalc_core::events::CalendarDate;
use mooncalc_core::time::Instant;
use mooncalc_maps::basemap::Scale;
use mooncalc_maps::engine::{EngineContext, RowBlock, ValueField, evaluate_rows, field_thresholds};
use mooncalc_maps::grid::VisibilityGrid;
use mooncalc_maps::projection::{Equirectangular, MapProjection};
use mooncalc_maps::render::{
    Legend, LegendPosition, MapData, Marker, RenderOptions, encode_png, render_raster,
};
use mooncalc_visibility::Criterion;
use mooncalc_visibility::circumstances::{Sighting, Status};

use crate::codec::base64_encode;
use crate::criteria;
use crate::dto::{
    ApiError, BestCell, Bounds, ExportPngParams, GridSpec, MapParams, MapResult, MapTiming,
    NO_DATA, NO_VALUE, PngImage, StatusCounts, ValueFieldDto,
};
use crate::ephemeris::check_covers;
use crate::validate;

/// Smallest and largest accepted grid spacing, degrees.
pub const RESOLUTION_RANGE_DEG: (f64, f64) = (0.1, 10.0);

/// Largest accepted number of cells (the 0.1° world grid).
pub const MAX_CELLS: usize = 1800 * 3600;

/// Rows per block that keeps per-block overhead negligible while giving
/// threads enough blocks to balance (a 0.25° world grid has 720 rows).
pub const DEFAULT_ROW_CHUNK: usize = 8;

/// Split `0..rows` into consecutive ranges of at most `chunk` rows.
pub fn row_chunks(rows: usize, chunk: usize) -> Vec<Range<usize>> {
    let chunk = chunk.max(1);
    (0..rows)
        .step_by(chunk)
        .map(|r| r..(r + chunk).min(rows))
        .collect()
}

fn validate_bounds(b: &Bounds) -> Result<(), ApiError> {
    let ok = b.lat_min_deg >= -90.0
        && b.lat_max_deg <= 90.0
        && b.lat_min_deg < b.lat_max_deg
        && b.lon_min_deg >= -360.0
        && b.lon_max_deg <= 360.0
        && b.lon_min_deg < b.lon_max_deg
        && b.lon_max_deg - b.lon_min_deg <= 360.0;
    if ok {
        Ok(())
    } else {
        Err(ApiError::invalid_params(
            "bounds must satisfy -90 <= latMin < latMax <= 90 and lonMin < lonMax \
             within -360..360, spanning at most 360°",
        ))
    }
}

/// The grid for a request (validated).
pub fn grid_for(params: &MapParams) -> Result<VisibilityGrid, ApiError> {
    let (lo, hi) = RESOLUTION_RANGE_DEG;
    if !(lo..=hi).contains(&params.resolution_deg) {
        return Err(ApiError::invalid_params(format!(
            "resolutionDeg must be between {lo} and {hi}, got {}",
            params.resolution_deg
        )));
    }
    let b = params.bounds.unwrap_or(Bounds::WORLD);
    validate_bounds(&b)?;
    let rows = ((b.lat_max_deg - b.lat_min_deg) / params.resolution_deg)
        .round()
        .max(1.0) as usize;
    let cols = ((b.lon_max_deg - b.lon_min_deg) / params.resolution_deg)
        .round()
        .max(1.0) as usize;
    if rows * cols > MAX_CELLS {
        return Err(ApiError::invalid_params(format!(
            "{rows} × {cols} cells is more than the limit of {MAX_CELLS}"
        )));
    }
    Ok(VisibilityGrid::new(
        b.lat_min_deg,
        b.lat_max_deg,
        b.lon_min_deg,
        b.lon_max_deg,
        rows,
        cols,
    ))
}

/// A prepared map: everything shared by the cells. `Sync`, so threads can
/// evaluate disjoint row ranges of one job concurrently.
pub struct MapJob {
    params: MapParams,
    criterion: &'static dyn Criterion,
    ephemeris_name: String,
    ctx: EngineContext<'static>,
    template: VisibilityGrid,
}

impl std::fmt::Debug for MapJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapJob")
            .field("params", &self.params)
            .field("rows", &self.template.rows)
            .field("cols", &self.template.cols)
            .finish_non_exhaustive()
    }
}

impl MapJob {
    /// Validate `params` and tabulate the ephemeris for the date. This is the
    /// expensive serial part (tens of milliseconds); cells are cheap.
    pub fn prepare(eph: &dyn Ephemeris, params: &MapParams) -> Result<Self, ApiError> {
        validate::date(params.date)?;
        let criterion = criteria::get(&params.criterion_id)?;
        if criterion.zones().len() >= usize::from(NO_DATA) {
            return Err(ApiError::internal("criterion has too many zones"));
        }
        let template = grid_for(params)?;
        let date = CalendarDate::from(params.date);
        // The engine tabulates from one day before to three days after 0h
        // UT, and phase searches reach a lunation further either side.
        let jd0 = Instant::from_jd_ut(date.jd0()).jd_tt();
        check_covers(eph, jd0 - 32.0, jd0 + 34.0)?;
        let ctx = EngineContext::new(
            eph,
            criterion,
            &template,
            date,
            Sighting::from(params.crescent),
        );
        Ok(Self {
            params: params.clone(),
            criterion,
            ephemeris_name: eph.name().to_owned(),
            ctx,
            template,
        })
    }

    pub fn params(&self) -> &MapParams {
        &self.params
    }

    pub fn criterion(&self) -> &'static dyn Criterion {
        self.criterion
    }

    pub fn rows(&self) -> usize {
        self.template.rows
    }

    pub fn cols(&self) -> usize {
        self.template.cols
    }

    /// Evaluate a range of rows (`rows.end <= self.rows()`).
    pub fn evaluate_rows(&self, rows: Range<usize>) -> RowBlock {
        evaluate_rows(&self.ctx, rows, |_, _| true).expect("never cancelled")
    }

    /// As [`Self::evaluate_rows`], calling `progress(done, total)` after each
    /// row; returning `false` cancels.
    pub fn evaluate_rows_with(
        &self,
        rows: Range<usize>,
        progress: impl FnMut(usize, usize) -> bool,
    ) -> Result<RowBlock, ApiError> {
        evaluate_rows(&self.ctx, rows, progress).map_err(|_| ApiError::cancelled())
    }

    /// Merge row blocks (in any order) into the full map. Rows no block
    /// covers stay empty.
    pub fn assemble(&self, blocks: impl IntoIterator<Item = RowBlock>) -> MapOutput {
        let mut grid = self.template.clone();
        let mut field = ValueField::new(grid.rows, grid.cols, field_thresholds(self.criterion));
        for b in blocks {
            grid.apply_rows(&b);
            field.apply_rows(&b);
        }
        MapOutput {
            params: self.params.clone(),
            criterion: self.criterion,
            conjunction: self.conjunction(),
            grid,
            field,
        }
    }

    /// Evaluate every row on the calling thread.
    pub fn evaluate(&self) -> MapOutput {
        self.assemble([self.evaluate_rows(0..self.rows())])
    }

    /// The geocentric new Moon nearest to noon UT of the map's date.
    pub fn conjunction(&self) -> Instant {
        let noon = self.ctx.date().jd0() + 0.5;
        *self
            .ctx
            .new_moons()
            .iter()
            .min_by(|a, b| (a.jd_ut - noon).abs().total_cmp(&(b.jd_ut - noon).abs()))
            .expect("the engine finds three new Moons")
    }

    /// The wire result. `timing` comes from the caller (the services keep no
    /// clocks).
    pub fn result(&self, out: &MapOutput, timing: MapTiming) -> MapResult {
        out.result(&self.ephemeris_name, timing)
    }
}

/// A computed map: zones and the continuous value field.
#[derive(Clone)]
pub struct MapOutput {
    pub params: MapParams,
    pub criterion: &'static dyn Criterion,
    pub conjunction: Instant,
    pub grid: VisibilityGrid,
    pub field: ValueField,
}

impl std::fmt::Debug for MapOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapOutput")
            .field("params", &self.params)
            .field("criterion", &self.criterion.id())
            .field("rows", &self.grid.rows)
            .field("cols", &self.grid.cols)
            .finish_non_exhaustive()
    }
}

fn status_counts(status: &[Status], zones: &[Option<u8>]) -> StatusCounts {
    let mut s = StatusCounts::default();
    for (st, z) in status.iter().zip(zones) {
        let slot = match st {
            // A crescent the criterion was applied to; MoonAlwaysUp cells
            // with a zone are counted as crescents too.
            Status::Crescent => &mut s.crescent,
            Status::MoonAlwaysUp if z.is_some() => &mut s.crescent,
            Status::BeforeConjunction => &mut s.before_conjunction,
            Status::MoonSetsFirst => &mut s.moon_sets_first,
            Status::MoonAlwaysUp => &mut s.moon_always_up,
            Status::MoonAlwaysDown => &mut s.moon_always_down,
            Status::MidnightSun => &mut s.midnight_sun,
            Status::PolarNight => &mut s.polar_night,
            Status::SunAltitudeNotReached => &mut s.sun_altitude_not_reached,
        };
        *slot += 1;
    }
    s
}

/// Quantise the value field to 16 bits over its own range.
fn quantise(values: &[f32]) -> Option<ValueFieldDto> {
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for &v in values.iter().filter(|v| v.is_finite()) {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if lo > hi {
        return None;
    }
    let offset = 0.5 * (f64::from(lo) + f64::from(hi));
    let span = f64::from(hi) - f64::from(lo);
    let scale = if span > 0.0 { span / 65_000.0 } else { 1.0 };
    let mut bytes = Vec::with_capacity(values.len() * 2);
    for &v in values {
        let q = if v.is_finite() {
            ((f64::from(v) - offset) / scale)
                .round()
                .clamp(-32_767.0, 32_767.0) as i16
        } else {
            NO_VALUE
        };
        bytes.extend_from_slice(&q.to_le_bytes());
    }
    Some(ValueFieldDto {
        scale,
        offset,
        no_value: NO_VALUE,
        data: base64_encode(&bytes),
    })
}

fn best_cell(grid: &VisibilityGrid, field: &ValueField) -> Option<BestCell> {
    let mut best: Option<(usize, u8, f32)> = None;
    for (i, z) in grid.cells.iter().enumerate() {
        let Some(z) = *z else { continue };
        let v = field.values[i];
        let better = match best {
            None => true,
            Some((_, bz, bv)) => {
                if v.is_finite() && bv.is_finite() {
                    v > bv
                } else if v.is_finite() != bv.is_finite() {
                    v.is_finite()
                } else {
                    z < bz
                }
            }
        };
        if better {
            best = Some((i, z, v));
        }
    }
    best.map(|(i, zone, v)| {
        let (lat_deg, lon_deg) = grid.cell_center(i / grid.cols, i % grid.cols);
        BestCell {
            lat_deg,
            lon_deg,
            zone,
            value: v.is_finite().then_some(f64::from(v)),
        }
    })
}

/// Replace characters the built-in 5×7 font lacks.
fn ascii_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '≥' => out.push_str(">="),
            '≤' => out.push_str("<="),
            '′' | '’' => out.push('\''),
            '″' => out.push('"'),
            '−' | '–' | '—' => out.push('-'),
            '×' => out.push('x'),
            '°' => out.push('°'),
            c if c.is_ascii() => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}

impl MapOutput {
    /// The wire result.
    pub fn result(&self, ephemeris_name: &str, timing: MapTiming) -> MapResult {
        let c = self.criterion;
        let zones = c.zones();
        let bytes: Vec<u8> = self
            .grid
            .cells
            .iter()
            .map(|z| z.unwrap_or(NO_DATA))
            .collect();
        let mut zone_counts = vec![0u32; zones.len()];
        for z in self.grid.cells.iter().flatten() {
            if let Some(n) = zone_counts.get_mut(usize::from(*z)) {
                *n += 1;
            }
        }
        MapResult {
            criterion_id: c.id().into(),
            criterion_name: c.name().into(),
            date: self.params.date,
            crescent: self.params.crescent,
            grid: GridSpec {
                lat_min_deg: self.grid.lat_min_deg,
                lat_max_deg: self.grid.lat_max_deg,
                lon_min_deg: self.grid.lon_min_deg,
                lon_max_deg: self.grid.lon_max_deg,
                rows: self.grid.rows as u32,
                cols: self.grid.cols as u32,
            },
            cells: base64_encode(&bytes),
            no_data: NO_DATA,
            legend: zones.iter().map(Into::into).collect(),
            values: if self.params.include_values {
                quantise(&self.field.values)
            } else {
                None
            },
            thresholds: self.field.thresholds.clone(),
            zone_counts,
            status_counts: status_counts(&self.field.status, &self.grid.cells),
            conjunction: self.conjunction.into(),
            evaluation_time: criteria::evaluation_time_text(c.evaluation_time()),
            date_convention: crate::dateline::map_convention(
                self.params.date.into(),
                self.params.crescent,
            ),
            best: best_cell(&self.grid, &self.field),
            ephemeris: ephemeris_name.to_owned(),
            timing,
        }
    }

    /// A suggested file name stem, e.g. `mooncalc-yallop-2024-03-10-evening`.
    pub fn file_stem(&self) -> String {
        let d = self.params.date;
        format!(
            "mooncalc-{}-{:04}-{:02}-{:02}-{}",
            self.criterion.id(),
            d.year,
            d.month,
            d.day,
            match self.params.crescent {
                crate::dto::Crescent::Evening => "evening",
                crate::dto::Crescent::Morning => "morning",
            }
        )
    }

    /// Render to PNG with `mooncalc_maps::render`: Natural Earth 1:50m base
    /// map, zones contoured from the value field where the criterion has
    /// thresholds (bilinear vote otherwise), graticule, legend and marker.
    pub fn render_png(&self, opts: &ExportPngParams) -> Result<PngImage, ApiError> {
        let width = opts.width_px.unwrap_or(2048);
        if !(256..=8192).contains(&width) {
            return Err(ApiError::invalid_params(
                "widthPx must be between 256 and 8192",
            ));
        }
        let zones = self.criterion.zones();
        let palette: Vec<[u8; 3]> = zones.iter().map(|z| z.rgb).collect();
        let scale = (width / 1024).clamp(1, 4);
        let legend = opts.legend.then(|| {
            let labels: Vec<String> = zones
                .iter()
                .map(|z| ascii_label(&format!("{}  {}", z.code, z.description)))
                .collect();
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            let d = self.params.date;
            let title = ascii_label(&format!(
                "{}  {:04}-{:02}-{:02} {}",
                self.criterion.name(),
                d.year,
                d.month,
                d.day,
                match self.params.crescent {
                    crate::dto::Crescent::Evening => "evening",
                    crate::dto::Crescent::Morning => "morning",
                }
            ));
            Legend {
                position: LegendPosition::BottomLeft,
                text_scale: scale,
                ..Legend::from_palette(Some(&title), &palette, &refs)
            }
        });
        let markers = opts
            .marker
            .iter()
            .map(|m| Marker {
                label: m.label.as_deref().map(ascii_label),
                radius_px: 3.5 * scale as f32,
                ..Marker::new(m.location.lat_deg, m.location.lon_deg, "")
            })
            .collect();
        let render_opts = RenderOptions {
            projection: MapProjection::from(Equirectangular::from_grid(&self.grid)),
            width,
            basemap: Scale::Ne50m,
            markers,
            label_scale: scale,
            legend,
            ..RenderOptions::default()
        }
        .fit_height();
        let data = MapData {
            grid: &self.grid,
            palette: &palette,
            none_color: None,
            field: self.field.zone_field(),
        };
        let img = render_raster(&data, &render_opts);
        let png = encode_png(&img).map_err(|e| ApiError::internal(e.to_string()))?;
        Ok(PngImage {
            width: img.width,
            height: img.height,
            png: base64_encode(&png),
            file_name: format!("{}.png", self.file_stem()),
        })
    }
}

/// Prepare and evaluate a map on the calling thread (no timing).
pub fn compute_map(eph: &dyn Ephemeris, params: &MapParams) -> Result<MapResult, ApiError> {
    let job = MapJob::prepare(eph, params)?;
    let out = job.evaluate();
    Ok(job.result(
        &out,
        MapTiming {
            threads: 1,
            ..MapTiming::default()
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{CivilDate, Crescent};
    use crate::ephemeris::meeus;

    pub(crate) fn params(step: f64) -> MapParams {
        MapParams {
            date: CivilDate {
                year: 2024,
                month: 3,
                day: 10,
            },
            criterion_id: "yallop".into(),
            crescent: Crescent::Evening,
            resolution_deg: step,
            bounds: None,
            ephemeris_id: None,
            include_values: true,
        }
    }

    fn zone_at(out: &MapOutput, lat: f64, lon: f64) -> Option<u8> {
        let g = &out.grid;
        let row =
            ((g.lat_max_deg - lat) / (g.lat_max_deg - g.lat_min_deg) * g.rows as f64) as usize;
        let col =
            ((lon - g.lon_min_deg) / (g.lon_max_deg - g.lon_min_deg) * g.cols as f64) as usize;
        g.get(row, col)
    }

    #[test]
    fn yallop_2024_03_10_evening_americas_not_asia() {
        // New Moon 2024 March 10 09:00 UT. On the evening of the 10th the
        // crescent is easily visible (A) only over the central Pacific,
        // visible under perfect conditions (B) across the eastern Pacific,
        // and needs optical aid (C) over Mexico and Central America. From
        // Africa to Japan it is below the Danjon limit (F) or absent (Moon
        // sets first, or the evening is before conjunction).
        let eph = meeus(false);
        let job = MapJob::prepare(&*eph, &params(3.0)).unwrap();
        assert_eq!((job.rows(), job.cols()), (60, 120));
        let out = job.evaluate();
        let count = |max_zone: u8, lon_from: f64, lon_to: f64| -> usize {
            let g = &out.grid;
            (0..g.rows)
                .flat_map(|r| (0..g.cols).map(move |c| (r, c)))
                .filter(|&(r, c)| (lon_from..=lon_to).contains(&g.cell_center(r, c).1))
                .filter(|&(r, c)| g.get(r, c).is_some_and(|z| z <= max_zone))
                .count()
        };
        // Naked-eye cells (A/B) in the Pacific and the Americas.
        assert!(
            count(1, -180.0, -100.0) > 200,
            "{}",
            count(1, -180.0, -100.0)
        );
        // Nothing even possible with a telescope (A–E) from Africa to Japan.
        assert_eq!(count(4, -20.0, 145.0), 0);
        // Spot checks: Honolulu A, the eastern Pacific B, Mexico City C,
        // Jakarta and Makkah not visible (F) or no crescent.
        assert_eq!(zone_at(&out, 21.3, -157.8), Some(0));
        assert_eq!(zone_at(&out, -5.0, -135.0), Some(1));
        assert_eq!(zone_at(&out, 19.4, -99.1), Some(2));
        assert!(zone_at(&out, -6.2, 106.8).is_none_or(|z| z == 5));
        assert!(zone_at(&out, 21.4, 39.8).is_none_or(|z| z == 5));

        let r = job.result(&out, MapTiming::default());
        assert_eq!(r.grid.rows * r.grid.cols, 7200);
        assert_eq!(r.legend.len(), 6);
        assert_eq!(r.zone_counts.iter().sum::<u32>(), r.status_counts.crescent);
        let total = r.status_counts;
        let sum = total.crescent
            + total.before_conjunction
            + total.moon_sets_first
            + total.moon_always_up
            + total.moon_always_down
            + total.midnight_sun
            + total.polar_night
            + total.sun_altitude_not_reached;
        assert_eq!(sum, 7200);
        // Conjunction 2024-03-10 09:00 UT.
        assert_eq!((r.conjunction.day, r.conjunction.hour), (10, 9));
        let best = r.best.unwrap();
        assert_eq!(best.zone, 0);
        assert!(best.lon_deg < -30.0, "{best:?}");
        assert!(r.values.is_some());
        assert_eq!(r.evaluation_time, "Best time (sunset + 4/9 lag)");
    }

    #[test]
    fn row_blocks_merge_into_the_full_map() {
        let eph = meeus(false);
        let mut p = params(10.0);
        p.criterion_id = "odeh".into();
        let job = MapJob::prepare(&*eph, &p).unwrap();
        let full = job.evaluate();
        let mut blocks: Vec<_> = row_chunks(job.rows(), 4)
            .into_iter()
            .map(|r| job.evaluate_rows(r))
            .collect();
        blocks.reverse();
        let merged = job.assemble(blocks);
        assert_eq!(merged.grid, full.grid);
        assert_eq!(merged.field.status, full.field.status);
        assert_eq!(row_chunks(10, 4), vec![0..4, 4..8, 8..10],);
        assert!(matches!(
            job.evaluate_rows_with(0..job.rows(), |d, _| d < 2),
            Err(ApiError { ref code, .. }) if code == "cancelled"
        ));
    }

    #[test]
    fn value_field_round_trips_through_quantisation() {
        let v = [0.5f32, -1.25, f32::NAN, 0.216, 3.0];
        let q = quantise(&v).unwrap();
        let bytes = crate::codec::base64_decode(&q.data).unwrap();
        for (i, want) in v.iter().enumerate() {
            let raw = i16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]);
            if want.is_nan() {
                assert_eq!(raw, NO_VALUE);
            } else {
                let got = q.offset + q.scale * f64::from(raw);
                assert!((got - f64::from(*want)).abs() <= q.scale, "{got} {want}");
            }
        }
        assert!(quantise(&[f32::NAN]).is_none());
        let flat = quantise(&[2.0, 2.0]).unwrap();
        assert_eq!(flat.offset, 2.0);
    }

    #[test]
    fn bad_params_are_rejected() {
        let eph = meeus(false);
        let mut p = params(0.01);
        assert_eq!(
            MapJob::prepare(&*eph, &p).unwrap_err().code,
            "invalid-params"
        );
        p = params(1.0);
        p.criterion_id = "nope".into();
        assert_eq!(
            MapJob::prepare(&*eph, &p).unwrap_err().code,
            "unknown-criterion"
        );
        p = params(1.0);
        p.date.day = 32;
        assert!(MapJob::prepare(&*eph, &p).is_err());
        p = params(1.0);
        p.bounds = Some(Bounds {
            lat_min_deg: 10.0,
            lat_max_deg: 0.0,
            ..Bounds::WORLD
        });
        assert!(MapJob::prepare(&*eph, &p).is_err());
        p = params(1.0);
        p.date.year = 4000; // Beyond the Meeus range.
        assert!(MapJob::prepare(&*eph, &p).is_err());
    }

    #[test]
    fn regional_bounds_and_png_export() {
        let eph = meeus(false);
        let mut p = params(2.0);
        p.bounds = Some(Bounds {
            lat_min_deg: -60.0,
            lat_max_deg: 60.0,
            lon_min_deg: -180.0,
            lon_max_deg: 0.0,
        });
        let job = MapJob::prepare(&*eph, &p).unwrap();
        assert_eq!((job.rows(), job.cols()), (60, 90));
        let out = job.evaluate();
        let png = out
            .render_png(&ExportPngParams {
                map: p.clone(),
                width_px: Some(512),
                legend: true,
                marker: Some(crate::dto::MapMarker {
                    location: crate::dto::Location {
                        lat_deg: -12.0,
                        lon_deg: -77.0,
                        elevation_m: 0.0,
                    },
                    label: Some("Lima".into()),
                }),
            })
            .unwrap();
        assert_eq!(png.width, 512);
        assert_eq!(png.height, 341);
        let bytes = crate::codec::base64_decode(&png.png).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(png.file_name, "mooncalc-yallop-2024-03-10-evening.png");
    }

    #[test]
    fn ascii_labels() {
        assert_eq!(ascii_label("ARCL ≥ 12° − W′"), "ARCL >= 12° - W'");
    }
}
