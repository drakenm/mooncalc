//! Coastlines and country borders as compact polylines, for web frontends
//! that draw them over their own zone canvas.
//!
//! The geometry is Natural Earth (public domain) at 1:110m or 1:50m, from
//! the blobs embedded in `mooncalc_maps::basemap`, simplified with the
//! Douglas–Peucker algorithm (Douglas & Peucker 1973, *Cartographica* 10,
//! 112) in plain longitude/latitude degrees and rounded to 0.001°.

use mooncalc_maps::basemap::{BaseMap, GeoPoint, Scale};

use crate::dto::{ApiError, Basemap, BasemapParams, BasemapScale, PolylineSet};

/// Credit line for the base map.
pub const ATTRIBUTION: &str = "Base map: Natural Earth (naturalearthdata.com), public domain.";

/// Default simplification tolerance, degrees.
pub fn default_tolerance_deg(scale: BasemapScale) -> f64 {
    match scale {
        BasemapScale::Ne110m => 0.02,
        BasemapScale::Ne50m => 0.01,
    }
}

/// Indices of the points Douglas–Peucker keeps (always the end points).
fn simplify(points: &[GeoPoint], tolerance: f64) -> Vec<usize> {
    let n = points.len();
    if n <= 2 || tolerance <= 0.0 {
        return (0..n).collect();
    }
    let xy = |i: usize| (f64::from(points[i].lon_deg), f64::from(points[i].lat_deg));
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    let tol2 = tolerance * tolerance;
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (ax, ay) = xy(a);
        let (bx, by) = xy(b);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        let (mut far, mut far_d2) = (a, -1.0);
        for i in a + 1..b {
            let (px, py) = xy(i);
            let d2 = if len2 == 0.0 {
                (px - ax).powi(2) + (py - ay).powi(2)
            } else {
                let t = (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0);
                (px - ax - t * dx).powi(2) + (py - ay - t * dy).powi(2)
            };
            if d2 > far_d2 {
                far = i;
                far_d2 = d2;
            }
        }
        if far_d2 > tol2 {
            keep[far] = true;
            stack.push((a, far));
            stack.push((far, b));
        }
    }
    (0..n).filter(|&i| keep[i]).collect()
}

fn round3(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

fn polylines(lines: &[Vec<GeoPoint>], tolerance: f64) -> PolylineSet {
    let mut set = PolylineSet::default();
    for line in lines {
        let kept = simplify(line, tolerance);
        if kept.len() < 2 {
            continue;
        }
        let mut flat = Vec::with_capacity(kept.len() * 2);
        for i in kept {
            flat.push(round3(line[i].lon_deg));
            flat.push(round3(line[i].lat_deg));
        }
        set.points += (flat.len() / 2) as u32;
        set.lines.push(flat);
    }
    set
}

/// Coastlines and borders at a scale, simplified.
pub fn basemap(p: &BasemapParams) -> Result<Basemap, ApiError> {
    let tolerance = p
        .tolerance_deg
        .unwrap_or_else(|| default_tolerance_deg(p.scale));
    if !(0.0..=5.0).contains(&tolerance) {
        return Err(ApiError::invalid_params(
            "toleranceDeg must be between 0 and 5",
        ));
    }
    let bm = BaseMap::embedded(match p.scale {
        BasemapScale::Ne110m => Scale::Ne110m,
        BasemapScale::Ne50m => Scale::Ne50m,
    });
    Ok(Basemap {
        scale: p.scale,
        coastlines: polylines(&bm.coastlines, tolerance),
        borders: polylines(&bm.borders, tolerance),
        attribution: ATTRIBUTION.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(lon: f32, lat: f32) -> GeoPoint {
        GeoPoint {
            lat_deg: lat,
            lon_deg: lon,
        }
    }

    #[test]
    fn douglas_peucker() {
        let line = [pt(0.0, 0.0), pt(1.0, 0.001), pt(2.0, 0.0), pt(3.0, 1.0)];
        assert_eq!(simplify(&line, 0.01), vec![0, 2, 3]);
        assert_eq!(simplify(&line, 0.0), vec![0, 1, 2, 3]);
        assert_eq!(simplify(&line[..2], 1.0), vec![0, 1]);
    }

    #[test]
    fn embedded_layers_simplify() {
        let full = basemap(&BasemapParams {
            scale: BasemapScale::Ne110m,
            tolerance_deg: Some(0.0),
        })
        .unwrap();
        let simple = basemap(&BasemapParams::default()).unwrap();
        assert_eq!(simple.coastlines.lines.len(), full.coastlines.lines.len());
        assert!(simple.coastlines.points < full.coastlines.points);
        assert!(simple.borders.points > 100);
        for l in simple.coastlines.lines.iter().chain(&simple.borders.lines) {
            assert!(l.len() >= 4 && l.len() % 2 == 0);
            assert!(
                l.chunks(2)
                    .all(|p| p[0].abs() <= 180.0 && p[1].abs() <= 90.0)
            );
        }
        assert!(
            basemap(&BasemapParams {
                tolerance_deg: Some(-1.0),
                ..BasemapParams::default()
            })
            .is_err()
        );
    }
}
