//! Per-pixel zone sampling (see the parent module docs for the modes).

use std::cell::Cell;

use super::{MapData, ZoneField, ZoneSmoothing};
use crate::grid::VisibilityGrid;
use crate::projection::Projection;

/// Premultiplied colour: rgb in 0..255 already multiplied by alpha (0..1).
pub(crate) type Premul = [f32; 4];

const TRANSPARENT: Premul = [0.0; 4];
const NONE_KEY: u16 = 256;

fn lerp(a: Premul, b: Premul, t: f32) -> Premul {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

/// Coverage of the winning side for a signed pixel distance to the edge.
#[inline]
fn coverage(dist_px: f32) -> f32 {
    (0.5 + dist_px).clamp(0.5, 1.0)
}

pub(crate) struct ZoneSampler<'a> {
    grid: &'a VisibilityGrid,
    field: Option<ZoneField<'a>>,
    colors: Vec<Premul>,
    none: Premul,
    inv_dlat: f64,
    inv_dlon: f64,
    lon_span: f64,
    wrap: bool,
    smoothing: ZoneSmoothing,
    antialias: bool,
    /// Output pixels per grid cell, horizontally and vertically.
    cell_px: (f32, f32),
    /// The last cell patch looked up (lower-left corner indices) and, if
    /// every pixel inside it gets one colour, that colour.
    patch: Cell<((i64, i64), Option<Premul>)>,
}

impl<'a> ZoneSampler<'a> {
    pub(crate) fn new(
        data: &MapData<'a>,
        proj: &dyn Projection,
        size: (u32, u32),
        opacity: f32,
        smoothing: ZoneSmoothing,
        antialias: bool,
    ) -> Self {
        let g = data.grid;
        let a = opacity.clamp(0.0, 1.0);
        let pm = |c: [u8; 3]| {
            [
                f32::from(c[0]) * a,
                f32::from(c[1]) * a,
                f32::from(c[2]) * a,
                a,
            ]
        };
        let rows = g.rows.max(1) as f64;
        let cols = g.cols.max(1) as f64;
        let dlat = (g.lat_max_deg - g.lat_min_deg) / rows;
        let dlon = (g.lon_max_deg - g.lon_min_deg) / cols;
        let lon_span = g.lon_max_deg - g.lon_min_deg;
        let field = data.field.filter(|f| f.values.len() == g.cells.len());
        Self {
            grid: g,
            field,
            colors: data.palette.iter().map(|&c| pm(c)).collect(),
            none: data.none_color.map_or(TRANSPARENT, pm),
            inv_dlat: 1.0 / dlat,
            inv_dlon: 1.0 / dlon,
            lon_span,
            wrap: lon_span >= 360.0 - 1e-9,
            smoothing,
            antialias,
            cell_px: estimate_cell_px(g, proj, size, dlat, dlon),
            patch: Cell::new(((i64::MIN, i64::MIN), None)),
        }
    }

    fn color(&self, key: u16) -> Premul {
        if key == NONE_KEY {
            self.none
        } else {
            self.colors.get(key as usize).copied().unwrap_or(self.none)
        }
    }

    #[inline]
    fn col(&self, i: i64) -> usize {
        let cols = self.grid.cols as i64;
        if self.wrap {
            if (0..cols).contains(&i) {
                i as usize
            } else {
                i.rem_euclid(cols) as usize
            }
        } else {
            i.clamp(0, cols - 1) as usize
        }
    }

    #[inline]
    fn row(&self, j: i64) -> usize {
        j.clamp(0, self.grid.rows as i64 - 1) as usize
    }

    #[inline]
    fn key(&self, row: usize, col: usize) -> u16 {
        match self.grid.cells[row * self.grid.cols + col] {
            Some(z) => u16::from(z),
            None => NONE_KEY,
        }
    }

    /// The zone layer colour at a point.
    pub(crate) fn sample(&self, lat_deg: f64, lon_deg: f64) -> Premul {
        let g = self.grid;
        if g.rows == 0 || g.cols == 0 || lat_deg < g.lat_min_deg || lat_deg > g.lat_max_deg {
            return TRANSPARENT;
        }
        let mut lon_off = lon_deg - g.lon_min_deg;
        if !(0.0..360.0).contains(&lon_off) {
            lon_off = lon_off.rem_euclid(360.0);
        }
        if !self.wrap && lon_off > self.lon_span {
            return TRANSPARENT;
        }
        // Fractional cell coordinates with cell centres at integers.
        let u = lon_off * self.inv_dlon - 0.5;
        let v = (g.lat_max_deg - lat_deg) * self.inv_dlat - 0.5;
        if self.field.is_some() || self.smoothing == ZoneSmoothing::Smooth {
            let key = (u.floor() as i64, v.floor() as i64);
            let (k, fast) = self.patch.get();
            let fast = if k == key {
                fast
            } else {
                let f = self.uniform_patch(key);
                self.patch.set((key, f));
                f
            };
            if let Some(c) = fast {
                return c;
            }
        }
        if let Some(field) = self.field {
            return self.sample_field(field, u, v);
        }
        match self.smoothing {
            ZoneSmoothing::Nearest => {
                let (i, j) = ((u + 0.5).floor() as i64, (v + 0.5).floor() as i64);
                self.color(self.key(self.row(j), self.col(i)))
            }
            ZoneSmoothing::Smooth => self.sample_vote(u, v),
        }
    }

    /// The colour of every pixel in the patch between the four cell centres
    /// with lower-left indices `(i0, j0)`, if the full sampling would give
    /// them all the same colour; `None` if they need per-pixel sampling.
    ///
    /// Exact: in field mode the bilinear value stays within the corner
    /// values, and its gradient is bounded by their range, so a patch whose
    /// corners share a zone and lie at least half a pixel (at that gradient
    /// bound) from every threshold gets full coverage everywhere.
    fn uniform_patch(&self, (i0, j0): (i64, i64)) -> Option<Premul> {
        let (c0, c1) = (self.col(i0), self.col(i0 + 1));
        let (r0, r1) = (self.row(j0), self.row(j0 + 1));
        let cells = [(r0, c0), (r0, c1), (r1, c0), (r1, c1)];
        let Some(field) = self.field else {
            let k = self.key(r0, c0);
            return cells
                .iter()
                .all(|&(r, c)| self.key(r, c) == k)
                .then(|| self.color(k));
        };
        let (mut vmin, mut vmax) = (f32::INFINITY, f32::NEG_INFINITY);
        for &(r, c) in &cells {
            let idx = r * self.grid.cols + c;
            let x = field.values[idx];
            if self.grid.cells[idx].is_none() || !x.is_finite() {
                return None;
            }
            vmin = vmin.min(x);
            vmax = vmax.max(x);
        }
        let z = field.classify(vmax);
        if field.classify(vmin) != z {
            return None;
        }
        if self.antialias {
            let zi = z as usize;
            let up = zi
                .checked_sub(1)
                .map_or(f32::INFINITY, |i| field.thresholds[i] - vmax);
            let lo = field
                .thresholds
                .get(zi)
                .map_or(f32::INFINITY, |&t| vmin - t);
            let (cx, cy) = self.cell_px;
            let grad_bound = (vmax - vmin) * (1.0 / (cx * cx) + 1.0 / (cy * cy)).sqrt();
            if up.min(lo) < 0.5 * grad_bound {
                return None;
            }
        }
        Some(self.color(u16::from(z)))
    }

    fn corners(&self, u: f64, v: f64) -> ([(usize, usize); 4], [f32; 4], f32, f32) {
        let (u0, v0) = (u.floor(), v.floor());
        let (tx, ty) = ((u - u0) as f32, (v - v0) as f32);
        let (i0, j0) = (u0 as i64, v0 as i64);
        let (c0, c1) = (self.col(i0), self.col(i0 + 1));
        let (r0, r1) = (self.row(j0), self.row(j0 + 1));
        let cells = [(r0, c0), (r0, c1), (r1, c0), (r1, c1)];
        let w = [
            (1.0 - tx) * (1.0 - ty),
            tx * (1.0 - ty),
            (1.0 - tx) * ty,
            tx * ty,
        ];
        (cells, w, tx, ty)
    }

    fn sample_vote(&self, u: f64, v: f64) -> Premul {
        let (cells, w, _, _) = self.corners(u, v);
        let mut keys = [0u16; 4];
        let mut sums = [0f32; 4];
        let mut n = 0;
        for (k, &(r, c)) in cells.iter().enumerate() {
            let key = self.key(r, c);
            match keys[..n].iter().position(|&x| x == key) {
                Some(i) => sums[i] += w[k],
                None => {
                    keys[n] = key;
                    sums[n] = w[k];
                    n += 1;
                }
            }
        }
        if n == 1 {
            return self.color(keys[0]);
        }
        let (mut best, mut second) = (0usize, usize::MAX);
        for i in 1..n {
            if sums[i] > sums[best] {
                second = best;
                best = i;
            } else if second == usize::MAX || sums[i] > sums[second] {
                second = i;
            }
        }
        let win = self.color(keys[best]);
        if !self.antialias {
            return win;
        }
        // The margin changes by ~2 per cell across an edge.
        let cell = 0.5 * (self.cell_px.0 + self.cell_px.1);
        let cov = coverage((sums[best] - sums[second]) * cell * 0.5);
        lerp(self.color(keys[second]), win, cov)
    }

    fn sample_field(&self, field: ZoneField<'_>, u: f64, v: f64) -> Premul {
        let (cells, w, tx, ty) = self.corners(u, v);
        // A cell with a zone but no finite value (e.g. lag +∞ where the Moon
        // does not set) cannot be interpolated; draw its neighbourhood from
        // the zones rather than as "no crescent".
        let zone_without_value = cells.iter().any(|&(r, c)| {
            let idx = r * self.grid.cols + c;
            self.grid.cells[idx].is_some() && !field.values[idx].is_finite()
        });
        if zone_without_value {
            return self.sample_vote(u, v);
        }
        let mut vals = [f32::NAN; 4];
        let (mut wsum, mut vsum) = (0f32, 0f32);
        for (k, &(r, c)) in cells.iter().enumerate() {
            let idx = r * self.grid.cols + c;
            let x = field.values[idx];
            if self.grid.cells[idx].is_some() && x.is_finite() {
                vals[k] = x;
                wsum += w[k];
                vsum += w[k] * x;
            }
        }
        if wsum <= 0.0 {
            return self.none;
        }
        let value = vsum / wsum;
        let w_none = 1.0 - wsum;
        let zone = field.classify(value);
        let mut c = self.color(u16::from(zone));
        if self.antialias {
            for x in &mut vals {
                if x.is_nan() {
                    *x = value;
                }
            }
            // Bilinear gradient per cell, then per pixel.
            let gx = ((1.0 - ty) * (vals[1] - vals[0]) + ty * (vals[3] - vals[2])) / self.cell_px.0;
            let gy = ((1.0 - tx) * (vals[2] - vals[0]) + tx * (vals[3] - vals[1])) / self.cell_px.1;
            let grad = (gx * gx + gy * gy).sqrt();
            if grad > 0.0 {
                let z = zone as usize;
                let upper = z
                    .checked_sub(1)
                    .map(|i| (field.thresholds[i] - value, z - 1));
                let lower = field.thresholds.get(z).map(|&t| (value - t, z + 1));
                let nearest = match (upper, lower) {
                    (Some(a), Some(b)) => Some(if a.0 < b.0 { a } else { b }),
                    (a, b) => a.or(b),
                };
                if let Some((d, other)) = nearest {
                    c = lerp(self.color(other as u16), c, coverage(d / grad));
                }
            }
        }
        if w_none > 0.0 {
            let m = 1.0 - 2.0 * w_none;
            if !self.antialias {
                if m <= 0.0 {
                    c = self.none;
                }
            } else {
                let cell = 0.5 * (self.cell_px.0 + self.cell_px.1);
                let d = m * cell * 0.5;
                c = if d >= 0.0 {
                    lerp(self.none, c, coverage(d))
                } else {
                    lerp(c, self.none, coverage(-d))
                };
            }
        }
        c
    }
}

/// Pixels per grid cell near the middle of the map.
fn estimate_cell_px(
    g: &VisibilityGrid,
    proj: &dyn Projection,
    size: (u32, u32),
    dlat: f64,
    dlon: f64,
) -> (f32, f32) {
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    let centre = proj.inverse(0.5, 0.5).unwrap_or((0.0, 0.0));
    let (lo, hi) = (g.lat_min_deg + dlat, g.lat_max_deg - dlat);
    let lat = if lo <= hi {
        centre.0.clamp(lo, hi)
    } else {
        0.5 * (g.lat_min_deg + g.lat_max_deg)
    };
    let lon = centre.1;
    let dist = |a: Option<(f64, f64)>, b: Option<(f64, f64)>| -> Option<f64> {
        let (a, b) = (a?, b?);
        Some(((b.0 - a.0) * w).hypot((b.1 - a.1) * h))
    };
    let p0 = proj.forward(lat, lon);
    let px = dist(p0, proj.forward(lat, lon + dlon))
        .filter(|d| *d < w * 0.5)
        .unwrap_or(1.0);
    let py = dist(p0, proj.forward(lat - dlat, lon))
        .filter(|d| *d < h * 0.5)
        .unwrap_or(1.0);
    ((px as f32).max(0.25), (py as f32).max(0.25))
}
