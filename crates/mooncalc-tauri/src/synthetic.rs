//! Placeholder data used until the real engines land (wave 2).
//!
//! Nothing here is astronomy. The grid generator draws a smooth, plausible-
//! looking zone pattern so the UI's map rendering, zoom, hover and legend can
//! be built and tested now. Results carry `synthetic: true`. Delete this
//! module once `mooncalc-maps` evaluates real grids and `mooncalc-atlas`
//! provides cities.

use mooncalc_maps::grid::VisibilityGrid;

use crate::dto::{City, CivilDate, Crescent, CriterionInfo, Location, ZoneLegendEntry};

/// Stand-in criteria, used only while `mooncalc_visibility::criteria::all()`
/// is empty. Zone thresholds are quoted from the papers for the legend text.
pub fn fallback_criteria() -> Vec<CriterionInfo> {
    fn zone(code: &str, description: &str, rgb: [u8; 3]) -> ZoneLegendEntry {
        ZoneLegendEntry {
            code: code.into(),
            description: description.into(),
            rgb,
        }
    }
    vec![
        CriterionInfo {
            id: "yallop".into(),
            name: "Yallop (1997)".into(),
            reference: "Yallop, B. D. (1997), A Method for Predicting the First Sighting of \
                        the New Crescent Moon, NAO Technical Note No. 69."
                .into(),
            zones: vec![
                zone("A", "Easily visible (q > +0.216)", [46, 160, 67]),
                zone(
                    "B",
                    "Visible under perfect conditions (q > -0.014)",
                    [154, 209, 75],
                ),
                zone(
                    "C",
                    "May need optical aid to find the crescent (q > -0.160)",
                    [242, 208, 59],
                ),
                zone("D", "Will need optical aid (q > -0.232)", [240, 140, 46]),
                zone(
                    "E",
                    "Not visible with a telescope (q > -0.293)",
                    [214, 60, 60],
                ),
                zone(
                    "F",
                    "Not visible, below the Danjon limit (q <= -0.293)",
                    [120, 120, 130],
                ),
            ],
        },
        CriterionInfo {
            id: "odeh".into(),
            name: "Odeh (2004)".into(),
            reference: "Odeh, M. Sh. (2004), New Criterion for Lunar Crescent Visibility, \
                        Experimental Astronomy 18, 39-64."
                .into(),
            zones: vec![
                zone("A", "Visible by naked eye (V >= 5.65)", [46, 160, 67]),
                zone(
                    "B",
                    "Visible by optical aid, could be seen by naked eye (V >= 2.00)",
                    [154, 209, 75],
                ),
                zone(
                    "C",
                    "Visible by optical aid only (V >= -0.96)",
                    [240, 140, 46],
                ),
                zone(
                    "D",
                    "Not visible even with optical aid (V < -0.96)",
                    [214, 60, 60],
                ),
            ],
        },
    ]
}

/// A smooth synthetic zone pattern over the whole world.
///
/// The best-visibility region sits west of a date-dependent longitude for the
/// evening crescent (east for the morning one) and fades towards the poles,
/// loosely like a real map. Cells beyond ±62° latitude, and those far from the
/// favoured region, get no evaluation (`None`).
pub fn visibility_grid(
    date: CivilDate,
    crescent: Crescent,
    resolution_deg: f64,
    zone_count: usize,
) -> VisibilityGrid {
    let rows = (180.0 / resolution_deg).round().max(1.0) as usize;
    let cols = (360.0 / resolution_deg).round().max(1.0) as usize;
    let mut grid = VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, rows, cols);
    if zone_count == 0 {
        return grid;
    }

    // An arbitrary day counter: only needs to vary from date to date.
    let n = f64::from(date.year) * 372.0 + f64::from(date.month) * 31.0 + f64::from(date.day);
    let lon0 = (n * 47.0).rem_euclid(360.0) - 180.0;
    let lat0 = 15.0 * (n * 0.21).sin();
    let peak_lon = match crescent {
        Crescent::Evening => lon0 - 90.0,
        Crescent::Morning => lon0 + 90.0,
    };
    let max_zone = (zone_count - 1) as f64;

    for row in 0..rows {
        let (lat, _) = grid.cell_center(row, 0);
        if lat.abs() > 62.0 {
            continue;
        }
        let lat_term = ((lat - lat0) / 40.0).powi(2);
        for col in 0..cols {
            let (_, lon) = grid.cell_center(row, col);
            let s = (lon - peak_lon).to_radians().cos() - lat_term;
            if s < -0.9 {
                continue;
            }
            let zone = ((0.8 - s) / 0.3).floor().clamp(0.0, max_zone);
            grid.set(row, col, Some(zone as u8));
        }
    }
    grid
}

/// One sample city: name, admin1, country, latitude, longitude (degrees,
/// east-positive), elevation (m), IANA time zone.
type CityRow = (
    &'static str,
    Option<&'static str>,
    &'static str,
    f64,
    f64,
    f64,
    &'static str,
);

/// A handful of well-known cities, used until `mooncalc-atlas` lands.
/// Coordinates are city-centre approximations; elevations are rounded.
#[rustfmt::skip]
const CITIES: &[CityRow] = &[
    ("Makkah", Some("Makkah"), "Saudi Arabia", 21.4225, 39.8262, 277.0, "Asia/Riyadh"),
    ("Madinah", Some("Madinah"), "Saudi Arabia", 24.4686, 39.6142, 608.0, "Asia/Riyadh"),
    ("Riyadh", Some("Riyadh"), "Saudi Arabia", 24.7136, 46.6753, 612.0, "Asia/Riyadh"),
    ("Cairo", None, "Egypt", 30.0444, 31.2357, 23.0, "Africa/Cairo"),
    ("Istanbul", None, "Turkey", 41.0082, 28.9784, 39.0, "Europe/Istanbul"),
    ("Tehran", None, "Iran", 35.6892, 51.3890, 1190.0, "Asia/Tehran"),
    ("Karachi", Some("Sindh"), "Pakistan", 24.8607, 67.0011, 10.0, "Asia/Karachi"),
    ("Lahore", Some("Punjab"), "Pakistan", 31.5204, 74.3587, 217.0, "Asia/Karachi"),
    ("Dhaka", None, "Bangladesh", 23.8103, 90.4125, 4.0, "Asia/Dhaka"),
    ("Kuala Lumpur", None, "Malaysia", 3.1390, 101.6869, 56.0, "Asia/Kuala_Lumpur"),
    ("Jakarta", None, "Indonesia", -6.2088, 106.8456, 8.0, "Asia/Jakarta"),
    ("Rabat", None, "Morocco", 34.0209, -6.8416, 75.0, "Africa/Casablanca"),
    ("Dakar", None, "Senegal", 14.7167, -17.4677, 22.0, "Africa/Dakar"),
    ("Cape Town", Some("Western Cape"), "South Africa", -33.9249, 18.4241, 15.0, "Africa/Johannesburg"),
    ("London", Some("England"), "United Kingdom", 51.5074, -0.1278, 11.0, "Europe/London"),
    ("New York", Some("New York"), "United States", 40.7128, -74.0060, 10.0, "America/New_York"),
    ("Los Angeles", Some("California"), "United States", 34.0522, -118.2437, 71.0, "America/Los_Angeles"),
    ("Toronto", Some("Ontario"), "Canada", 43.6532, -79.3832, 76.0, "America/Toronto"),
    ("Santiago", None, "Chile", -33.4489, -70.6693, 570.0, "America/Santiago"),
    ("Sydney", Some("New South Wales"), "Australia", -33.8688, 151.2093, 58.0, "Australia/Sydney"),
];

/// The sample cities as DTOs.
pub fn sample_cities() -> Vec<City> {
    CITIES
        .iter()
        .map(
            |&(name, admin1, country, lat_deg, lon_deg, elevation_m, tz)| City {
                name: name.into(),
                admin1: admin1.map(Into::into),
                country: country.into(),
                location: Location {
                    lat_deg,
                    lon_deg,
                    elevation_m,
                },
                time_zone: Some(tz.into()),
                population: None,
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATE: CivilDate = CivilDate {
        year: 2026,
        month: 11,
        day: 10,
    };

    #[test]
    fn grid_has_requested_shape_and_valid_zones() {
        let g = visibility_grid(DATE, Crescent::Evening, 0.25, 6);
        assert_eq!((g.rows, g.cols), (720, 1440));
        assert!(g.cells.iter().flatten().all(|&z| z < 6));
        // Polar rows are empty, and both evaluated and empty cells exist.
        assert!((0..g.cols).all(|c| g.get(0, c).is_none()));
        assert!(g.cells.iter().any(Option::is_some));
        assert!(g.cells.iter().any(Option::is_none));
    }

    #[test]
    fn morning_and_evening_differ() {
        let e = visibility_grid(DATE, Crescent::Evening, 2.0, 4);
        let m = visibility_grid(DATE, Crescent::Morning, 2.0, 4);
        assert_ne!(e.cells, m.cells);
    }

    #[test]
    fn fallback_criteria_have_legends() {
        for c in fallback_criteria() {
            assert!(!c.zones.is_empty(), "{}", c.id);
        }
    }
}
