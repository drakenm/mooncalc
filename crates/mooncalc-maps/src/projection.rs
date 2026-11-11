//! Map projections into a normalised plane.
//!
//! Every projection maps latitude/longitude (degrees, east-positive) to
//! normalised coordinates `(x, y)` where the map's bounding box is the unit
//! square: `x` runs west to east from 0 to 1 and `y` runs **north to south**
//! from 0 to 1 (image convention). A raster of `width` x `height` pixels
//! shows the map undistorted when `width / height == aspect_ratio()`.
//!
//! * [`Equirectangular`]: plate carrée with configurable bounds, matching a
//!   [`VisibilityGrid`](crate::grid::VisibilityGrid).
//! * [`Robinson`]: the Robinson (1974) world projection.
//! * [`Orthographic`]: a globe seen from infinity, centred anywhere.
//!
//! [`MapProjection`] wraps all three in one cloneable value for options
//! structs.

use std::f64::consts::PI;
use std::sync::OnceLock;

use crate::grid::VisibilityGrid;

/// A map projection into the normalised unit square (see the module docs).
pub trait Projection {
    /// Projects a point. `None` if the point is not on the visible part of
    /// the projection (the far side of a globe). Points that are on the
    /// projection plane but outside the map frame may return coordinates
    /// outside [0, 1]².
    fn forward(&self, lat_deg: f64, lon_deg: f64) -> Option<(f64, f64)>;

    /// Unprojects normalised coordinates to `(lat_deg, lon_deg)`, with
    /// longitude in [-180, 180). `None` outside the map.
    fn inverse(&self, x: f64, y: f64) -> Option<(f64, f64)>;

    /// Width / height of the map frame.
    fn aspect_ratio(&self) -> f64;

    /// The longitude at which the map is cut (its left and right edges),
    /// for cylindrical and pseudo-cylindrical projections. Lines crossing it
    /// must be split. `None` for projections without a seam.
    fn seam_lon_deg(&self) -> Option<f64> {
        None
    }

    /// For azimuthal projections showing one hemisphere: the centre of the
    /// visible hemisphere `(lat_deg, lon_deg)`. Used to clip polygons along
    /// the horizon.
    fn hemisphere_center(&self) -> Option<(f64, f64)> {
        None
    }

    /// The outline of the map frame as a closed polygon in normalised
    /// coordinates (the projected image of the bounds or of the globe's
    /// limb).
    fn outline(&self) -> Vec<(f64, f64)>;
}

/// Normalises a longitude into [-180, 180).
pub fn normalize_lon(lon_deg: f64) -> f64 {
    (lon_deg + 180.0).rem_euclid(360.0) - 180.0
}

/// Equirectangular (plate carrée) projection over arbitrary bounds:
/// `x` linear in longitude, `y` linear in latitude (Snyder 1987, *Map
/// Projections: A Working Manual*, USGS PP 1395, ch. 12, eqs. 12-1, 12-2
/// with φ₁ = 0).
///
/// `lon_max_deg` may exceed 180 so that a map can be centred anywhere, e.g.
/// bounds -30..330 centre the map on 150° E. Longitudes are wrapped into
/// `[lon_min, lon_min + 360)` before projection, so the seam is `lon_min`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Equirectangular {
    pub lat_min_deg: f64,
    pub lat_max_deg: f64,
    pub lon_min_deg: f64,
    pub lon_max_deg: f64,
}

impl Equirectangular {
    /// The whole world centred on `central_lon_deg`.
    pub fn world(central_lon_deg: f64) -> Self {
        let lon_min = normalize_lon(central_lon_deg) - 180.0;
        Self {
            lat_min_deg: -90.0,
            lat_max_deg: 90.0,
            lon_min_deg: lon_min,
            lon_max_deg: lon_min + 360.0,
        }
    }

    /// Exactly the bounds of `grid`, so that grid cells map to equal
    /// rectangles filling the frame.
    pub fn from_grid(grid: &VisibilityGrid) -> Self {
        Self {
            lat_min_deg: grid.lat_min_deg,
            lat_max_deg: grid.lat_max_deg,
            lon_min_deg: grid.lon_min_deg,
            lon_max_deg: grid.lon_max_deg,
        }
    }

    pub fn central_lon_deg(&self) -> f64 {
        normalize_lon(0.5 * (self.lon_min_deg + self.lon_max_deg))
    }

    fn lon_span(&self) -> f64 {
        self.lon_max_deg - self.lon_min_deg
    }

    fn lat_span(&self) -> f64 {
        self.lat_max_deg - self.lat_min_deg
    }
}

impl Default for Equirectangular {
    fn default() -> Self {
        Self::world(0.0)
    }
}

impl Projection for Equirectangular {
    fn forward(&self, lat_deg: f64, lon_deg: f64) -> Option<(f64, f64)> {
        let lon = self.lon_min_deg + (lon_deg - self.lon_min_deg).rem_euclid(360.0);
        Some((
            (lon - self.lon_min_deg) / self.lon_span(),
            (self.lat_max_deg - lat_deg) / self.lat_span(),
        ))
    }

    fn inverse(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return None;
        }
        Some((
            self.lat_max_deg - y * self.lat_span(),
            normalize_lon(self.lon_min_deg + x * self.lon_span()),
        ))
    }

    fn aspect_ratio(&self) -> f64 {
        self.lon_span() / self.lat_span()
    }

    fn seam_lon_deg(&self) -> Option<f64> {
        Some(normalize_lon(self.lon_min_deg))
    }

    fn outline(&self) -> Vec<(f64, f64)> {
        vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
    }
}

/// Robinson's table (Snyder & Voxland 1989, *An Album of Map Projections*,
/// USGS PP 1453, p. 82-83; Robinson 1974): for latitudes 0°, 5°, ..., 90°,
/// the parallel length `X` and the distance from the equator `Y`, as
/// fractions of their equatorial / polar values.
const ROBINSON_X: [f64; 19] = [
    1.0000, 0.9986, 0.9954, 0.9900, 0.9822, 0.9730, 0.9600, 0.9427, 0.9216, 0.8962, 0.8679, 0.8350,
    0.7986, 0.7597, 0.7186, 0.6732, 0.6213, 0.5722, 0.5322,
];
const ROBINSON_Y: [f64; 19] = [
    0.0000, 0.0620, 0.1240, 0.1860, 0.2480, 0.3100, 0.3720, 0.4340, 0.4958, 0.5571, 0.6176, 0.6769,
    0.7346, 0.7903, 0.8435, 0.8936, 0.9394, 0.9761, 1.0000,
];
/// Scale constants: x = 0.8487 R X Δλ, y = 1.3523 R Y (same source).
const ROBINSON_KX: f64 = 0.8487;
const ROBINSON_KY: f64 = 1.3523;

/// Cubic (Catmull-Rom) interpolation through a 5°-spaced table, with the
/// table mirrored about the equator and extrapolated linearly past the pole,
/// so the curve passes exactly through every tabulated value.
fn robinson_interp(table: &[f64; 19], odd: bool, abs_lat_deg: f64) -> f64 {
    let t = (abs_lat_deg / 5.0).clamp(0.0, 18.0);
    let i = (t.floor() as usize).min(17);
    let f = t - i as f64;
    let at = |k: isize| -> f64 {
        if k < 0 {
            let v = table[(-k) as usize];
            if odd { -v } else { v }
        } else if k > 18 {
            2.0 * table[18] - table[17]
        } else {
            table[k as usize]
        }
    };
    let k = i as isize;
    let (p0, p1, p2, p3) = (at(k - 1), at(k), at(k + 1), at(k + 2));
    let f2 = f * f;
    let f3 = f2 * f;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * f
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * f2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * f3)
}

/// Inverse table φ(Y) for Robinson, sampled at uniform `Y`.
fn robinson_inverse_lut() -> &'static [f64] {
    static LUT: OnceLock<Vec<f64>> = OnceLock::new();
    LUT.get_or_init(|| {
        const N: usize = 1024;
        (0..=N)
            .map(|k| {
                let target = k as f64 / N as f64;
                let (mut lo, mut hi) = (0.0, 90.0);
                for _ in 0..60 {
                    let mid = 0.5 * (lo + hi);
                    if robinson_interp(&ROBINSON_Y, true, mid) < target {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                0.5 * (lo + hi)
            })
            .collect()
    })
}

/// |φ| in degrees for a Robinson `Y` in [0, 1].
fn robinson_lat_from_y(y: f64) -> f64 {
    let lut = robinson_inverse_lut();
    let n = (lut.len() - 1) as f64;
    let t = (y * n).clamp(0.0, n);
    let i = (t.floor() as usize).min(lut.len() - 2);
    let mut lat = lut[i] + (lut[i + 1] - lut[i]) * (t - i as f64);
    // One Newton step on the interpolated curve.
    let h = 1e-4;
    let f = robinson_interp(&ROBINSON_Y, true, lat) - y;
    let d = (robinson_interp(&ROBINSON_Y, true, lat + h)
        - robinson_interp(&ROBINSON_Y, true, lat - h))
        / (2.0 * h);
    if d > 0.0 {
        lat -= f / d;
    }
    lat.clamp(0.0, 90.0)
}

/// The Robinson pseudo-cylindrical world projection, centred on a meridian.
///
/// Defined by Robinson's table of parallel lengths and spacings (Snyder &
/// Voxland 1989, p. 82-83), interpolated here with Catmull-Rom cubics; the
/// result matches the table exactly at every 5° of latitude.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Robinson {
    pub central_lon_deg: f64,
}

impl Robinson {
    pub fn new(central_lon_deg: f64) -> Self {
        Self { central_lon_deg }
    }
}

impl Projection for Robinson {
    fn forward(&self, lat_deg: f64, lon_deg: f64) -> Option<(f64, f64)> {
        let dlon = normalize_lon(lon_deg - self.central_lon_deg).to_radians();
        let a = lat_deg.abs().min(90.0);
        let x = robinson_interp(&ROBINSON_X, false, a);
        let y = robinson_interp(&ROBINSON_Y, true, a) * lat_deg.signum();
        // x_n = 0.5 + KX X Δλ / (2π KX), y_n = 0.5 - KY Y / (2 KY).
        Some((0.5 + x * dlon / (2.0 * PI), 0.5 - 0.5 * y))
    }

    fn inverse(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let yy = (0.5 - y) * 2.0;
        if yy.abs() > 1.0 {
            return None;
        }
        let lat = robinson_lat_from_y(yy.abs()).copysign(yy);
        let px = robinson_interp(&ROBINSON_X, false, lat.abs());
        let dlon = (x - 0.5) * 2.0 * PI / px;
        if dlon.abs() > PI {
            return None;
        }
        Some((lat, normalize_lon(self.central_lon_deg + dlon.to_degrees())))
    }

    fn aspect_ratio(&self) -> f64 {
        (2.0 * PI * ROBINSON_KX) / (2.0 * ROBINSON_KY)
    }

    fn seam_lon_deg(&self) -> Option<f64> {
        Some(normalize_lon(self.central_lon_deg + 180.0))
    }

    fn outline(&self) -> Vec<(f64, f64)> {
        let mut pts = Vec::with_capacity(362);
        // East edge from north to south, then west edge from south to north.
        for i in 0..=180 {
            let lat = 90.0 - i as f64;
            let x = robinson_interp(&ROBINSON_X, false, lat.abs());
            let y = robinson_interp(&ROBINSON_Y, true, lat.abs()) * lat.signum();
            pts.push((0.5 + 0.5 * x, 0.5 - 0.5 * y));
        }
        for i in 0..=180 {
            let lat = -90.0 + i as f64;
            let x = robinson_interp(&ROBINSON_X, false, lat.abs());
            let y = robinson_interp(&ROBINSON_Y, true, lat.abs()) * lat.signum();
            pts.push((0.5 - 0.5 * x, 0.5 - 0.5 * y));
        }
        pts
    }
}

/// Orthographic azimuthal projection: the globe as seen from infinitely far
/// away above `(center_lat_deg, center_lon_deg)` (Snyder 1987, ch. 20,
/// pp. 145-153, forward and inverse spherical formulas). The
/// visible hemisphere fills the unit disc inscribed in the frame.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Orthographic {
    pub center_lat_deg: f64,
    pub center_lon_deg: f64,
}

impl Orthographic {
    pub fn new(center_lat_deg: f64, center_lon_deg: f64) -> Self {
        Self {
            center_lat_deg,
            center_lon_deg,
        }
    }
}

impl Projection for Orthographic {
    fn forward(&self, lat_deg: f64, lon_deg: f64) -> Option<(f64, f64)> {
        let (sp0, cp0) = self.center_lat_deg.to_radians().sin_cos();
        let (sp, cp) = lat_deg.to_radians().sin_cos();
        let (sl, cl) = (lon_deg - self.center_lon_deg).to_radians().sin_cos();
        let cos_c = sp0 * sp + cp0 * cp * cl;
        if cos_c < 0.0 {
            return None;
        }
        let x = cp * sl;
        let y = cp0 * sp - sp0 * cp * cl;
        Some((0.5 + 0.5 * x, 0.5 - 0.5 * y))
    }

    fn inverse(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let px = (x - 0.5) * 2.0;
        let py = (0.5 - y) * 2.0;
        let rho = px.hypot(py);
        if rho > 1.0 {
            return None;
        }
        let (sp0, cp0) = self.center_lat_deg.to_radians().sin_cos();
        if rho < 1e-15 {
            return Some((self.center_lat_deg, normalize_lon(self.center_lon_deg)));
        }
        let c = rho.asin();
        let (sc, cc) = c.sin_cos();
        let lat = (cc * sp0 + py * sc * cp0 / rho).clamp(-1.0, 1.0).asin();
        let dlon = (px * sc).atan2(rho * cp0 * cc - py * sp0 * sc);
        Some((
            lat.to_degrees(),
            normalize_lon(self.center_lon_deg + dlon.to_degrees()),
        ))
    }

    fn aspect_ratio(&self) -> f64 {
        1.0
    }

    fn hemisphere_center(&self) -> Option<(f64, f64)> {
        Some((self.center_lat_deg, self.center_lon_deg))
    }

    fn outline(&self) -> Vec<(f64, f64)> {
        (0..360)
            .map(|i| {
                let a = (i as f64).to_radians();
                (0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin())
            })
            .collect()
    }
}

/// Any of the built-in projections, as one cloneable value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MapProjection {
    Equirectangular(Equirectangular),
    Robinson(Robinson),
    Orthographic(Orthographic),
}

impl Default for MapProjection {
    fn default() -> Self {
        Self::Equirectangular(Equirectangular::default())
    }
}

impl MapProjection {
    fn inner(&self) -> &dyn Projection {
        match self {
            Self::Equirectangular(p) => p,
            Self::Robinson(p) => p,
            Self::Orthographic(p) => p,
        }
    }
}

impl Projection for MapProjection {
    fn forward(&self, lat_deg: f64, lon_deg: f64) -> Option<(f64, f64)> {
        self.inner().forward(lat_deg, lon_deg)
    }
    fn inverse(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        self.inner().inverse(x, y)
    }
    fn aspect_ratio(&self) -> f64 {
        self.inner().aspect_ratio()
    }
    fn seam_lon_deg(&self) -> Option<f64> {
        self.inner().seam_lon_deg()
    }
    fn hemisphere_center(&self) -> Option<(f64, f64)> {
        self.inner().hemisphere_center()
    }
    fn outline(&self) -> Vec<(f64, f64)> {
        self.inner().outline()
    }
}

impl From<Equirectangular> for MapProjection {
    fn from(p: Equirectangular) -> Self {
        Self::Equirectangular(p)
    }
}

impl From<Robinson> for MapProjection {
    fn from(p: Robinson) -> Self {
        Self::Robinson(p)
    }
}

impl From<Orthographic> for MapProjection {
    fn from(p: Orthographic) -> Self {
        Self::Orthographic(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lon_diff(a: f64, b: f64) -> f64 {
        normalize_lon(a - b).abs()
    }

    fn round_trip(p: &dyn Projection, lat: f64, lon: f64) {
        let (x, y) = p
            .forward(lat, lon)
            .unwrap_or_else(|| panic!("({lat}, {lon}) not visible"));
        let (lat2, lon2) = p
            .inverse(x, y)
            .unwrap_or_else(|| panic!("({x}, {y}) not invertible"));
        assert!((lat - lat2).abs() < 1e-7, "lat {lat} -> {lat2}");
        if lat.abs() < 89.999 {
            assert!(
                lon_diff(lon, lon2) < 1e-7,
                "lon {lon} -> {lon2} at lat {lat}"
            );
        }
    }

    #[test]
    fn equirectangular_round_trips_and_maps_bounds() {
        let p = Equirectangular::world(0.0);
        assert_eq!(p.forward(90.0, -180.0), Some((0.0, 0.0)));
        assert_eq!(p.forward(0.0, 0.0), Some((0.5, 0.5)));
        assert!((p.aspect_ratio() - 2.0).abs() < 1e-12);
        for p in [
            Equirectangular::world(0.0),
            Equirectangular::world(150.0),
            Equirectangular::world(-75.5),
        ] {
            for lat in (-89..=89).step_by(7) {
                for lon in (-179..180).step_by(11) {
                    round_trip(&p, lat as f64, lon as f64 + 0.25);
                }
            }
        }
        // Centred on 150° E: 150° E sits in the middle, 30° W at the seam.
        let p = Equirectangular::world(150.0);
        let (x, _) = p.forward(0.0, 150.0).unwrap();
        assert!((x - 0.5).abs() < 1e-12);
        assert_eq!(p.seam_lon_deg(), Some(-30.0));
        // A regional grid.
        let g = VisibilityGrid::new(-60.0, 60.0, -30.0, 90.0, 10, 10);
        let p = Equirectangular::from_grid(&g);
        assert!((p.aspect_ratio() - 1.0).abs() < 1e-12);
        let (x, y) = p.forward(0.0, 30.0).unwrap();
        assert!((x - 0.5).abs() < 1e-12 && (y - 0.5).abs() < 1e-12);
        assert!(p.inverse(1.2, 0.5).is_none());
    }

    #[test]
    fn robinson_matches_table_and_round_trips() {
        let p = Robinson::new(0.0);
        // At tabulated latitudes, x = KX X Δλ and y = KY Y exactly; in
        // normalised units x_n = 0.5 + X Δλ / 2π, y_n = 0.5 - Y / 2.
        for (i, (&tx, &ty)) in ROBINSON_X.iter().zip(&ROBINSON_Y).enumerate() {
            let lat = 5.0 * i as f64;
            let (x, y) = p.forward(lat, 90.0).unwrap();
            assert!((x - (0.5 + tx * 0.25)).abs() < 1e-12, "lat {lat}");
            assert!((y - (0.5 - ty * 0.5)).abs() < 1e-12, "lat {lat}");
            let (_, ys) = p.forward(-lat, 90.0).unwrap();
            assert!((ys - (0.5 + ty * 0.5)).abs() < 1e-12);
        }
        // Snyder & Voxland formula at a tabulated latitude, R = 1:
        // (lat 45, lon 90) -> x = 0.8487 * 0.8962 * π/2 R, y = 1.3523 * 0.5571 R.
        let (x, y) = p.forward(45.0, 90.0).unwrap();
        let w = 2.0 * PI * ROBINSON_KX;
        let h = 2.0 * ROBINSON_KY;
        assert!(((x - 0.5) * w - 1.194_758).abs() < 1e-5);
        assert!(((0.5 - y) * h - 0.753_366).abs() < 1e-5);
        assert!((p.aspect_ratio() - 1.971_6).abs() < 1e-4);
        for p in [Robinson::new(0.0), Robinson::new(-100.0)] {
            for lat in (-89..=89).step_by(3) {
                for lon in (-179..180).step_by(13) {
                    round_trip(&p, lat as f64 + 0.3, lon as f64 + 0.25);
                }
            }
        }
        // Corners of the frame lie outside the Robinson outline.
        assert!(p.inverse(0.01, 0.01).is_none());
        assert!(p.inverse(0.5, 1.01).is_none());
    }

    #[test]
    fn orthographic_round_trips_and_hides_far_side() {
        for p in [
            Orthographic::new(0.0, 0.0),
            Orthographic::new(51.5, -0.1),
            Orthographic::new(-90.0, 0.0),
        ] {
            for lat in (-89..=89).step_by(4) {
                for lon in (-179..180).step_by(9) {
                    let (lat, lon) = (lat as f64 + 0.1, lon as f64 + 0.2);
                    if let Some((x, y)) = p.forward(lat, lon) {
                        // Points well inside the disc invert accurately.
                        if (x - 0.5).hypot(y - 0.5) < 0.49 {
                            round_trip(&p, lat, lon);
                        }
                    }
                }
            }
        }
        let p = Orthographic::new(30.0, 60.0);
        assert_eq!(p.forward(30.0, 60.0), Some((0.5, 0.5)));
        assert!(p.forward(-30.0, -120.0).is_none(), "antipode is hidden");
        assert!(p.inverse(0.0, 0.0).is_none(), "corner is outside the disc");
        let (x, y) = Orthographic::new(0.0, 0.0).forward(0.0, 90.0).unwrap();
        assert!((x - 1.0).abs() < 1e-12 && (y - 0.5).abs() < 1e-12);
        let (x, y) = Orthographic::new(0.0, 0.0).forward(90.0, 0.0).unwrap();
        assert!((x - 0.5).abs() < 1e-12 && y.abs() < 1e-12);
    }

    #[test]
    fn map_projection_delegates() {
        let p: MapProjection = Robinson::new(10.0).into();
        assert_eq!(
            p.forward(20.0, 30.0),
            Robinson::new(10.0).forward(20.0, 30.0)
        );
        assert_eq!(p.seam_lon_deg(), Some(-170.0));
        let o: MapProjection = Orthographic::new(1.0, 2.0).into();
        assert_eq!(o.hemisphere_center(), Some((1.0, 2.0)));
        assert_eq!(o.aspect_ratio(), 1.0);
    }
}
