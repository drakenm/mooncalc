//! A small near-side feature layer for the Moon close-up: named craters
//! and schematic outlines of the major maria.
//!
//! Names and centre coordinates follow the IAU *Gazetteer of Planetary
//! Nomenclature* (IAU Working Group for Planetary System Nomenclature /
//! USGS Astrogeology Science Center, <https://planetarynames.wr.usgs.gov>,
//! public domain): planetocentric latitude, east-positive longitude,
//! rounded to about 0.1°–0.5° here, and the feature diameter in km.
//!
//! The mare **outlines are schematic**: round basins are drawn as circles
//! of the Gazetteer diameter around the Gazetteer centre; the irregular
//! maria (Oceanus Procellarum, Mare Frigoris, Tranquillitatis,
//! Fecunditatis, Nubium) as coarse hand-placed polygons. They are meant to
//! make libration and phase visible, not for mapping.

/// Mean lunar radius, km (IAU).
pub const MOON_RADIUS_KM: f64 = 1737.4;

/// A named crater.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crater {
    pub name: &'static str,
    /// Selenographic latitude, degrees north.
    pub lat: f64,
    /// Selenographic longitude, degrees east (towards Mare Crisium).
    pub lon: f64,
    pub diameter_km: f64,
}

const fn c(name: &'static str, lat: f64, lon: f64, diameter_km: f64) -> Crater {
    Crater {
        name,
        lat,
        lon,
        diameter_km,
    }
}

/// Named near-side craters.
pub const CRATERS: [Crater; 36] = [
    c("Tycho", -43.3, -11.4, 85.0),
    c("Copernicus", 9.6, -20.1, 96.0),
    c("Kepler", 8.1, -38.0, 31.0),
    c("Aristarchus", 23.7, -47.5, 40.0),
    c("Plato", 51.6, -9.4, 101.0),
    c("Clavius", -58.6, -14.7, 231.0),
    c("Ptolemaeus", -9.2, -1.8, 154.0),
    c("Alphonsus", -13.4, -2.9, 111.0),
    c("Arzachel", -18.3, -1.9, 97.0),
    c("Grimaldi", -5.5, -68.3, 173.0),
    c("Langrenus", -8.9, 61.0, 132.0),
    c("Petavius", -25.3, 60.4, 184.0),
    c("Theophilus", -11.4, 26.3, 99.0),
    c("Cyrillus", -13.2, 24.0, 98.0),
    c("Catharina", -18.1, 23.6, 99.0),
    c("Posidonius", 31.9, 30.0, 95.0),
    c("Eratosthenes", 14.5, -11.3, 58.0),
    c("Archimedes", 29.7, -4.0, 81.0),
    c("Aristoteles", 50.2, 17.3, 88.0),
    c("Eudoxus", 44.3, 16.2, 67.0),
    c("Proclus", 16.1, 46.8, 27.0),
    c("Gassendi", -17.5, -40.0, 110.0),
    c("Schickard", -44.3, -54.6, 206.0),
    c("Longomontanus", -49.6, -21.8, 145.0),
    c("Maginus", -50.5, -6.3, 194.0),
    c("Stöfler", -41.1, 6.0, 126.0),
    c("Walther", -33.1, 1.0, 132.0),
    c("Albategnius", -11.2, 4.0, 129.0),
    c("Hipparchus", -5.1, 5.2, 138.0),
    c("Manilius", 14.5, 9.1, 38.0),
    c("Menelaus", 16.3, 16.0, 27.0),
    c("Bullialdus", -20.7, -22.2, 61.0),
    c("Pythagoras", 63.7, -63.0, 142.0),
    c("Endymion", 53.6, 56.5, 123.0),
    c("Atlas", 46.7, 44.4, 87.0),
    c("Furnerius", -36.0, 60.6, 125.0),
];

/// The outline of a mare.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MareShape {
    /// A circle of the given diameter around the label position.
    Circle { diameter_km: f64 },
    /// A closed polygon of (latitude, longitude) vertices, degrees.
    Polygon(&'static [(f64, f64)]),
}

/// A mare: label position (the Gazetteer centre) and outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mare {
    pub name: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub shape: MareShape,
}

const fn circle(name: &'static str, lat: f64, lon: f64, diameter_km: f64) -> Mare {
    Mare {
        name,
        lat,
        lon,
        shape: MareShape::Circle { diameter_km },
    }
}

const fn poly(name: &'static str, lat: f64, lon: f64, v: &'static [(f64, f64)]) -> Mare {
    Mare {
        name,
        lat,
        lon,
        shape: MareShape::Polygon(v),
    }
}

/// The major maria.
pub const MARIA: [Mare; 19] = [
    poly(
        "Oceanus Procellarum",
        18.4,
        -57.4,
        &[
            (56.0, -60.0),
            (48.0, -48.0),
            (40.0, -42.0),
            (30.0, -40.0),
            (20.0, -34.0),
            (10.0, -30.0),
            (2.0, -30.0),
            (-6.0, -34.0),
            (-12.0, -40.0),
            (-16.0, -46.0),
            (-10.0, -58.0),
            (0.0, -66.0),
            (12.0, -72.0),
            (25.0, -78.0),
            (38.0, -72.0),
            (48.0, -68.0),
        ],
    ),
    circle("Mare Imbrium", 32.8, -15.6, 1146.0),
    circle("Mare Serenitatis", 28.0, 17.5, 707.0),
    poly(
        "Mare Tranquillitatis",
        8.5,
        31.4,
        &[
            (19.0, 22.0),
            (16.0, 30.0),
            (13.0, 38.0),
            (6.0, 45.0),
            (-2.0, 42.0),
            (-5.0, 34.0),
            (-2.0, 25.0),
            (4.0, 19.0),
            (12.0, 18.0),
        ],
    ),
    circle("Mare Crisium", 17.0, 59.1, 556.0),
    poly(
        "Mare Fecunditatis",
        -7.8,
        51.3,
        &[
            (4.0, 50.0),
            (2.0, 57.0),
            (-8.0, 60.0),
            (-18.0, 58.0),
            (-22.0, 52.0),
            (-15.0, 47.0),
            (-6.0, 45.0),
            (0.0, 46.0),
        ],
    ),
    circle("Mare Nectaris", -15.2, 35.5, 333.0),
    poly(
        "Mare Nubium",
        -21.3,
        -16.6,
        &[
            (-12.0, -20.0),
            (-15.0, -10.0),
            (-22.0, -6.0),
            (-30.0, -12.0),
            (-30.0, -22.0),
            (-24.0, -28.0),
            (-17.0, -26.0),
        ],
    ),
    circle("Mare Humorum", -24.4, -38.6, 389.0),
    poly(
        "Mare Frigoris",
        56.0,
        1.4,
        &[
            (50.0, -40.0),
            (56.0, -28.0),
            (58.0, -10.0),
            (57.0, 8.0),
            (56.0, 25.0),
            (58.0, 40.0),
            (54.0, 48.0),
            (50.0, 40.0),
            (52.0, 25.0),
            (53.0, 8.0),
            (53.0, -8.0),
            (51.0, -25.0),
            (47.0, -38.0),
        ],
    ),
    circle("Mare Vaporum", 13.3, 3.6, 245.0),
    circle("Sinus Medii", 2.4, 1.7, 335.0),
    circle("Mare Insularum", 7.5, -30.9, 513.0),
    circle("Mare Cognitum", -10.0, -23.1, 376.0),
    circle("Mare Smythii", 1.3, 87.5, 374.0),
    circle("Mare Marginis", 13.3, 86.1, 420.0),
    circle("Mare Humboldtianum", 56.8, 81.5, 273.0),
    circle("Mare Australe", -38.9, 93.0, 603.0),
    circle("Mare Orientale", -19.4, -92.8, 294.0),
];

/// Angular radius (radians) of a circle of `diameter_km` on the Moon.
pub fn angular_radius(diameter_km: f64) -> f64 {
    diameter_km / 2.0 / MOON_RADIUS_KM
}

/// Points (latitude, longitude in radians) around a small circle of
/// angular radius `r` centred at (`lat`, `lon`), radians.
pub fn small_circle(lat: f64, lon: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    let (sl, cl) = lat.sin_cos();
    let (sr, cr) = r.sin_cos();
    (0..=n)
        .map(|i| {
            let az = std::f64::consts::TAU * i as f64 / n as f64;
            let lat2 = (sl * cr + cl * sr * az.cos()).asin();
            let lon2 = lon + (az.sin() * sr * cl).atan2(cr - sl * lat2.sin());
            (lat2, lon2)
        })
        .collect()
}

/// The outline of a mare as a closed path of (latitude, longitude),
/// radians, densified so it projects smoothly.
pub fn outline(m: &Mare) -> Vec<(f64, f64)> {
    let d = std::f64::consts::PI / 180.0;
    match m.shape {
        MareShape::Circle { diameter_km } => {
            small_circle(m.lat * d, m.lon * d, angular_radius(diameter_km), 72)
        }
        MareShape::Polygon(v) => {
            let mut out = Vec::new();
            for i in 0..v.len() {
                let (a, b) = (v[i], v[(i + 1) % v.len()]);
                for s in 0..6 {
                    let f = s as f64 / 6.0;
                    out.push(((a.0 + (b.0 - a.0) * f) * d, (a.1 + (b.1 - a.1) * f) * d));
                }
            }
            out.push((v[0].0 * d, v[0].1 * d));
            out
        }
    }
}

/// Whether (lat, lon) in degrees lies inside mare `m`.
pub fn contains(m: &Mare, lat: f64, lon: f64) -> bool {
    match m.shape {
        MareShape::Circle { diameter_km } => {
            let d = std::f64::consts::PI / 180.0;
            mooncalc_core::angle::separation(lon * d, lat * d, m.lon * d, m.lat * d)
                < angular_radius(diameter_km)
        }
        MareShape::Polygon(v) => {
            // Even-odd rule in the (lon, lat) plane; the polygons are small
            // and do not straddle ±180°.
            let mut inside = false;
            let mut j = v.len() - 1;
            for i in 0..v.len() {
                let (yi, xi) = v[i];
                let (yj, xj) = v[j];
                if (yi > lat) != (yj > lat) && lon < (xj - xi) * (lat - yi) / (yj - yi) + xi {
                    inside = !inside;
                }
                j = i;
            }
            inside
        }
    }
}

/// Mare mask on a 0.5° equirectangular grid: row 0 is latitude −90°,
/// column 0 longitude −180°.
pub struct MareMask {
    cells: Vec<bool>,
}

impl MareMask {
    pub const COLS: usize = 720;
    pub const ROWS: usize = 360;

    pub fn build() -> Self {
        let mut cells = vec![false; Self::COLS * Self::ROWS];
        for row in 0..Self::ROWS {
            let lat = -90.0 + (row as f64 + 0.5) * 0.5;
            for col in 0..Self::COLS {
                let lon = -180.0 + (col as f64 + 0.5) * 0.5;
                cells[row * Self::COLS + col] = MARIA.iter().any(|m| contains(m, lat, lon));
            }
        }
        Self { cells }
    }

    /// Whether the point (radians) is in a mare.
    pub fn at(&self, lat: f64, lon: f64) -> bool {
        let lat_deg = lat.to_degrees();
        let lon_deg = (lon.to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
        let row = (((lat_deg + 90.0) * 2.0) as usize).min(Self::ROWS - 1);
        let col = (((lon_deg + 180.0) * 2.0) as usize).min(Self::COLS - 1);
        self.cells[row * Self::COLS + col]
    }
}

/// The shared mask, built on first use.
pub fn mare_mask() -> &'static MareMask {
    static MASK: std::sync::OnceLock<MareMask> = std::sync::OnceLock::new();
    MASK.get_or_init(MareMask::build)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mare(name: &str) -> &'static Mare {
        MARIA.iter().find(|m| m.name == name).unwrap()
    }

    #[test]
    fn craters_sit_where_expected() {
        // Archimedes lies in Mare Imbrium; Plato (on its northern shore)
        // and Copernicus (south of it) lie outside.
        let imbrium = mare("Mare Imbrium");
        let at = |n: &str| CRATERS.iter().find(|c| c.name == n).unwrap();
        assert!(contains(
            imbrium,
            at("Archimedes").lat,
            at("Archimedes").lon
        ));
        assert!(!contains(imbrium, at("Plato").lat, at("Plato").lon));
        assert!(!contains(
            imbrium,
            at("Copernicus").lat,
            at("Copernicus").lon
        ));
        assert!(contains(
            mare("Mare Nubium"),
            at("Bullialdus").lat,
            at("Bullialdus").lon
        ));
        assert!(contains(
            mare("Oceanus Procellarum"),
            at("Kepler").lat,
            at("Kepler").lon
        ));
        assert!(!contains(
            mare("Mare Crisium"),
            at("Proclus").lat,
            at("Proclus").lon
        ));
        // Apollo 11 landed in Mare Tranquillitatis (0.67° N, 23.47° E).
        assert!(contains(mare("Mare Tranquillitatis"), 0.67, 23.47));
        // Tycho is in the southern highlands.
        assert!(
            !MARIA
                .iter()
                .any(|m| contains(m, at("Tycho").lat, at("Tycho").lon))
        );
    }

    #[test]
    fn mask_and_outlines() {
        let mask = mare_mask();
        let d = std::f64::consts::PI / 180.0;
        assert!(mask.at(17.0 * d, 59.1 * d)); // Crisium
        assert!(!mask.at(-43.3 * d, -11.4 * d)); // Tycho
        let o = outline(mare("Mare Crisium"));
        assert_eq!(o.first(), o.last());
        // Every outline point is one radius from the centre.
        let r = angular_radius(556.0);
        for (lat, lon) in o {
            let s = mooncalc_core::angle::separation(lon, lat, 59.1 * d, 17.0 * d);
            assert!((s - r).abs() < 1e-9);
        }
    }
}
