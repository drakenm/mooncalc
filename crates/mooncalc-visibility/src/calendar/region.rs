//! Observers for a visibility calendar: one place, several places, or a
//! region sampled on a grid.

use mooncalc_core::GeoLocation;

/// A latitude/longitude box, degrees (east-positive longitudes, with
/// `lon_min_deg < lon_max_deg`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatLonBox {
    pub lat_min_deg: f64,
    pub lat_max_deg: f64,
    pub lon_min_deg: f64,
    pub lon_max_deg: f64,
}

impl LatLonBox {
    pub const fn new(
        lat_min_deg: f64,
        lat_max_deg: f64,
        lon_min_deg: f64,
        lon_max_deg: f64,
    ) -> Self {
        Self {
            lat_min_deg,
            lat_max_deg,
            lon_min_deg,
            lon_max_deg,
        }
    }

    /// Cell centres of a grid with cells of about `step_deg` (at least one
    /// cell per axis), like `mooncalc_maps::grid::VisibilityGrid`.
    pub fn grid_points(&self, step_deg: f64) -> Vec<GeoLocation> {
        let cells = |span: f64| ((span / step_deg.max(0.1)).ceil() as usize).max(1);
        let rows = cells(self.lat_max_deg - self.lat_min_deg);
        let cols = cells(self.lon_max_deg - self.lon_min_deg);
        let dlat = (self.lat_max_deg - self.lat_min_deg) / rows as f64;
        let dlon = (self.lon_max_deg - self.lon_min_deg) / cols as f64;
        let mut out = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                out.push(GeoLocation::new(
                    self.lat_min_deg + (r as f64 + 0.5) * dlat,
                    self.lon_min_deg + (c as f64 + 0.5) * dlon,
                    0.0,
                ));
            }
        }
        out
    }
}

/// A region: the union of one or more boxes.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub boxes: Vec<LatLonBox>,
}

impl Region {
    pub fn new(boxes: Vec<LatLonBox>) -> Self {
        Self { boxes }
    }

    /// Sea-level grid points over every box (see [`LatLonBox::grid_points`]).
    pub fn grid_points(&self, step_deg: f64) -> Vec<GeoLocation> {
        self.boxes
            .iter()
            .flat_map(|b| b.grid_points(step_deg))
            .collect()
    }
}

/// Regions for "visible anywhere in …" calendars, in the spirit of
/// MoonCalc 6's world-wide and tri-zonal calendars.
///
/// The boxes are coarse continental outlines of our own choosing (MoonCalc's
/// exact zones are not published). They include sea, so a crescent visible
/// only over the ocean counts as seen; and they stop at ±60° latitude, beyond
/// which there are few observers and long twilight. Each calendar's month
/// starts after the first evening on which the crescent meets the threshold
/// anywhere in the region (each grid point on its own local evening).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegionPreset {
    /// Anywhere between 60° S and 60° N.
    World,
    /// North and South America, 56° S – 60° N, 170° W – 30° W.
    Americas,
    /// Europe, Africa and the Middle East, 35° S – 60° N, 20° W – 60° E.
    EuropeAfricaMiddleEast,
    /// Asia and the Pacific, 47° S – 55° N, 60° E – 180°.
    AsiaPacific,
}

impl RegionPreset {
    pub const ALL: [RegionPreset; 4] = [
        RegionPreset::World,
        RegionPreset::Americas,
        RegionPreset::EuropeAfricaMiddleEast,
        RegionPreset::AsiaPacific,
    ];

    /// Stable identifier, safe to persist.
    pub fn id(self) -> &'static str {
        match self {
            RegionPreset::World => "world",
            RegionPreset::Americas => "americas",
            RegionPreset::EuropeAfricaMiddleEast => "europe-africa-middle-east",
            RegionPreset::AsiaPacific => "asia-pacific",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RegionPreset::World => "Anywhere (60° S – 60° N)",
            RegionPreset::Americas => "The Americas",
            RegionPreset::EuropeAfricaMiddleEast => "Europe, Africa and the Middle East",
            RegionPreset::AsiaPacific => "Asia and the Pacific",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.id() == id)
    }

    pub fn region(self) -> Region {
        let b = match self {
            RegionPreset::World => LatLonBox::new(-60.0, 60.0, -180.0, 180.0),
            RegionPreset::Americas => LatLonBox::new(-56.0, 60.0, -170.0, -30.0),
            RegionPreset::EuropeAfricaMiddleEast => LatLonBox::new(-35.0, 60.0, -20.0, 60.0),
            RegionPreset::AsiaPacific => LatLonBox::new(-47.0, 55.0, 60.0, 180.0),
        };
        Region::new(vec![b])
    }
}

/// Where the crescent is looked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Observers {
    /// One place.
    Place(GeoLocation),
    /// Several places: seen if seen at any of them.
    Places(Vec<GeoLocation>),
    /// A region sampled on a grid of about `step_deg` cells: seen if seen at
    /// any grid point.
    Region { region: Region, step_deg: f64 },
}

impl Observers {
    /// The places to test, ordered west to east (the evening crescent is
    /// usually seen first in the west, so "seen anywhere" stops sooner).
    pub fn points(&self) -> Vec<GeoLocation> {
        let mut p = match self {
            Observers::Place(l) => vec![*l],
            Observers::Places(v) => v.clone(),
            Observers::Region { region, step_deg } => region.grid_points(*step_deg),
        };
        p.sort_by(|a, b| a.lon_deg.total_cmp(&b.lon_deg));
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_points_cover_the_box() {
        let b = LatLonBox::new(-10.0, 10.0, 100.0, 130.0);
        let p = b.grid_points(10.0);
        assert_eq!(p.len(), 2 * 3);
        assert_eq!((p[0].lat_deg, p[0].lon_deg), (-5.0, 105.0));
        assert_eq!((p[5].lat_deg, p[5].lon_deg), (5.0, 125.0));
        // A step larger than the box still yields one cell.
        assert_eq!(b.grid_points(90.0).len(), 1);
        let world = RegionPreset::World.region().grid_points(5.0);
        assert_eq!(world.len(), 24 * 72);
        assert!(world.iter().all(|l| l.lat_deg.abs() < 60.0));
    }

    #[test]
    fn presets_round_trip_and_sort_west_to_east() {
        for r in RegionPreset::ALL {
            assert_eq!(RegionPreset::from_id(r.id()), Some(r));
            assert!(!r.region().grid_points(10.0).is_empty());
        }
        let o = Observers::Places(vec![
            GeoLocation::new(0.0, 50.0, 0.0),
            GeoLocation::new(0.0, -50.0, 0.0),
        ]);
        assert_eq!(o.points()[0].lon_deg, -50.0);
    }
}
