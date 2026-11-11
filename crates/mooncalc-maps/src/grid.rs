//! Visibility grids.

/// A regular latitude/longitude grid of zone indices.
///
/// Row 0 is the northernmost row (`lat_max_deg`), column 0 the westernmost
/// (`lon_min_deg`). Cell centres sit half a step in from the bounds.
#[derive(Clone, Debug, PartialEq)]
pub struct VisibilityGrid {
    pub lat_min_deg: f64,
    pub lat_max_deg: f64,
    pub lon_min_deg: f64,
    pub lon_max_deg: f64,
    pub rows: usize,
    pub cols: usize,
    /// `rows * cols` entries, row-major. `None` = no evaluation possible
    /// (e.g. no sunset that day, or Moon sets before the Sun / before
    /// conjunction, rendered as "not visible").
    pub cells: Vec<Option<u8>>,
}

impl VisibilityGrid {
    /// An empty grid covering the given bounds.
    pub fn new(lat_min_deg: f64, lat_max_deg: f64, lon_min_deg: f64, lon_max_deg: f64, rows: usize, cols: usize) -> Self {
        Self { lat_min_deg, lat_max_deg, lon_min_deg, lon_max_deg, rows, cols, cells: vec![None; rows * cols] }
    }

    /// Latitude/longitude of a cell centre, degrees.
    pub fn cell_center(&self, row: usize, col: usize) -> (f64, f64) {
        let dlat = (self.lat_max_deg - self.lat_min_deg) / self.rows as f64;
        let dlon = (self.lon_max_deg - self.lon_min_deg) / self.cols as f64;
        (self.lat_max_deg - (row as f64 + 0.5) * dlat, self.lon_min_deg + (col as f64 + 0.5) * dlon)
    }

    pub fn get(&self, row: usize, col: usize) -> Option<u8> {
        self.cells[row * self.cols + col]
    }

    pub fn set(&mut self, row: usize, col: usize, v: Option<u8>) {
        self.cells[row * self.cols + col] = v;
    }
}
