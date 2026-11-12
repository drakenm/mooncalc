//! Geometry shared by the raster and SVG back ends: projecting polylines
//! and polygons with seam splitting and horizon clipping, graticule
//! generation and path simplification.

use crate::projection::{Projection, normalize_lon};

/// A normalised map-plane point (see [`crate::projection`]).
pub(crate) type Pt = (f64, f64);

/// Nudge used to place points on either side of a seam or just inside a
/// horizon, degrees.
const EPS_DEG: f64 = 1e-7;

/// Where the segment `a -> b` (taken the short way round) crosses the seam,
/// as `(t, lat at crossing, a is on the eastern edge of the map)`.
fn seam_crossing(seam: f64, a: (f64, f64), b: (f64, f64)) -> Option<(f64, f64, bool)> {
    let da = (a.1 - seam).rem_euclid(360.0);
    let dl = normalize_lon(b.1 - a.1);
    if dl == 0.0 {
        return None;
    }
    let t = if dl > 0.0 && da + dl >= 360.0 {
        (360.0 - da) / dl
    } else if dl < 0.0 && da + dl < 0.0 {
        da / -dl
    } else {
        return None;
    };
    Some((t, a.0 + t * (b.0 - a.0), dl > 0.0))
}

/// The last visible point on `vis -> hid`, by bisection along the
/// lat/lon-linear segment (lon taken the short way).
fn boundary_point(proj: &dyn Projection, vis: (f64, f64), hid: (f64, f64)) -> Option<Pt> {
    let dl = normalize_lon(hid.1 - vis.1);
    let at = |t: f64| (vis.0 + t * (hid.0 - vis.0), vis.1 + t * dl);
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        let p = at(mid);
        if proj.forward(p.0, p.1).is_some() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let p = at(lo);
    proj.forward(p.0, p.1)
}

/// Projects a lat/lon polyline (degrees) into map-plane pieces, splitting
/// it where it crosses the projection's seam and clipping it where it goes
/// out of view. Pieces with fewer than two points are dropped.
pub(crate) fn project_polyline(
    proj: &dyn Projection,
    pts: impl IntoIterator<Item = (f64, f64)>,
    out: &mut Vec<Vec<Pt>>,
) {
    let seam = proj.seam_lon_deg();
    let mut cur: Vec<Pt> = Vec::new();
    let mut prev: Option<((f64, f64), Option<Pt>)> = None;
    let flush = |cur: &mut Vec<Pt>, out: &mut Vec<Vec<Pt>>| {
        if cur.len() >= 2 {
            out.push(std::mem::take(cur));
        } else {
            cur.clear();
        }
    };
    for p in pts {
        let q = proj.forward(p.0, p.1);
        match prev {
            None => {
                if let Some(q) = q {
                    cur.push(q);
                }
            }
            Some((a, aq)) => {
                if let Some((_, lat_s, a_east)) = seam.and_then(|s| seam_crossing(s, a, p)) {
                    let s = seam.unwrap_or(0.0);
                    let (lon_a, lon_b) = if a_east {
                        (s - EPS_DEG, s + EPS_DEG)
                    } else {
                        (s + EPS_DEG, s - EPS_DEG)
                    };
                    if let Some(e) = proj.forward(lat_s, lon_a) {
                        cur.push(e);
                    }
                    flush(&mut cur, out);
                    if let Some(e) = proj.forward(lat_s, lon_b) {
                        cur.push(e);
                    }
                    if let Some(q) = q {
                        cur.push(q);
                    }
                } else {
                    match (aq, q) {
                        (Some(_), Some(q)) => cur.push(q),
                        (Some(_), None) => {
                            if let Some(e) = boundary_point(proj, a, p) {
                                cur.push(e);
                            }
                            flush(&mut cur, out);
                        }
                        (None, Some(q)) => {
                            if let Some(e) = boundary_point(proj, p, a) {
                                cur.push(e);
                            }
                            cur.push(q);
                        }
                        (None, None) => {}
                    }
                }
            }
        }
        prev = Some((p, q));
    }
    flush(&mut cur, out);
}

/// Graticule lines (lat, lon degrees), densified every degree. Meridians at
/// the seam are skipped; the frame outline draws them.
pub(crate) fn graticule_lines(spacing_deg: f64, seam: Option<f64>) -> Vec<Vec<(f64, f64)>> {
    let mut lines = Vec::new();
    if spacing_deg.is_nan() || spacing_deg <= 0.0 {
        return lines;
    }
    let n = (360.0 / spacing_deg).ceil() as i64;
    for k in 0..n {
        let lon = -180.0 + k as f64 * spacing_deg;
        if lon >= 180.0 {
            break;
        }
        if seam.is_some_and(|s| normalize_lon(lon - s).abs() < 1e-9) {
            continue;
        }
        lines.push((0..=180).map(|i| (-90.0 + i as f64, lon)).collect());
    }
    let m = (90.0 / spacing_deg).floor() as i64;
    for k in -m..=m {
        let lat = k as f64 * spacing_deg;
        if lat.abs() >= 90.0 - 1e-9 {
            continue;
        }
        lines.push((0..=360).map(|i| (lat, -180.0 + i as f64)).collect());
    }
    lines
}

/// Inserts points so that no edge of the closed ring is longer than
/// `step` degrees in latitude or longitude (edges taken literally, without
/// wrapping).
fn densify_ring(ring: &[(f64, f64)], step: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::with_capacity(ring.len());
    for (i, &a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        out.push(a);
        let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) / step).ceil() as usize;
        for k in 1..n {
            let t = k as f64 / n as f64;
            out.push((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)));
        }
    }
    out
}

/// Sutherland-Hodgman clip of a closed ring against `coord(p) >= limit`
/// (`keep_above`) or `<= limit`, on the second (longitude-like) coordinate.
fn clip_lon(ring: &[(f64, f64)], limit: f64, keep_above: bool) -> Vec<(f64, f64)> {
    let inside = |p: &(f64, f64)| {
        if keep_above {
            p.1 >= limit
        } else {
            p.1 <= limit
        }
    };
    let mut out = Vec::with_capacity(ring.len() + 4);
    for (i, a) in ring.iter().enumerate() {
        let b = &ring[(i + 1) % ring.len()];
        let (ia, ib) = (inside(a), inside(b));
        if ia {
            out.push(*a);
        }
        if ia != ib {
            let t = (limit - a.1) / (b.1 - a.1);
            out.push((a.0 + t * (b.0 - a.0), limit));
        }
    }
    out
}

fn ring_area(ring: &[Pt]) -> f64 {
    let mut s = 0.0;
    for (i, a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        s += a.0 * b.1 - b.0 * a.1;
    }
    0.5 * s
}

fn to_vec3(lat_deg: f64, lon_deg: f64) -> [f64; 3] {
    let (sp, cp) = lat_deg.to_radians().sin_cos();
    let (sl, cl) = lon_deg.to_radians().sin_cos();
    [cp * cl, cp * sl, sp]
}

fn to_latlon(v: [f64; 3]) -> (f64, f64) {
    let r = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (
        (v[2] / r).clamp(-1.0, 1.0).asin().to_degrees(),
        v[1].atan2(v[0]).to_degrees(),
    )
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize3(v: [f64; 3]) -> [f64; 3] {
    let r = dot(v, v).sqrt();
    if r == 0.0 {
        v
    } else {
        [v[0] / r, v[1] / r, v[2] / r]
    }
}

/// Clips a ring of unit vectors to the hemisphere `dot(v, n) >= 0`,
/// replacing each stretch hidden behind the horizon by an arc of the
/// horizon circle (the shorter way round).
fn clip_hemisphere(ring: &[[f64; 3]], n: [f64; 3]) -> Vec<[f64; 3]> {
    // (point, is an exit point onto the horizon)
    let mut pts: Vec<([f64; 3], bool)> = Vec::with_capacity(ring.len() + 8);
    for (i, &a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        let (da, db) = (dot(a, n), dot(b, n));
        if da >= 0.0 {
            pts.push((a, false));
        }
        if (da >= 0.0) != (db >= 0.0) {
            let t = da / (da - db);
            let p = normalize3([
                a[0] + t * (b[0] - a[0]),
                a[1] + t * (b[1] - a[1]),
                a[2] + t * (b[2] - a[2]),
            ]);
            pts.push((p, da >= 0.0));
        }
    }
    let mut out = Vec::with_capacity(pts.len() * 2);
    for (i, &(p, exit)) in pts.iter().enumerate() {
        out.push(p);
        if exit {
            let q = pts[(i + 1) % pts.len()].0;
            // Arc from p to q along the horizon great circle.
            let ang = dot(p, q).clamp(-1.0, 1.0).acos();
            let steps = (ang.to_degrees() / 2.0).ceil() as usize;
            if steps > 1 {
                // Orthonormal basis (p, w) of the horizon plane, w towards q.
                let c = dot(p, q);
                let w = normalize3([q[0] - c * p[0], q[1] - c * p[1], q[2] - c * p[2]]);
                for k in 1..steps {
                    let a = ang * k as f64 / steps as f64;
                    let (s, co) = a.sin_cos();
                    out.push([
                        co * p[0] + s * w[0],
                        co * p[1] + s * w[1],
                        co * p[2] + s * w[2],
                    ]);
                }
            }
        }
    }
    // Nudge horizon points just inside so that they project.
    let nudge = EPS_DEG.to_radians();
    out.iter()
        .map(|v| {
            normalize3([
                v[0] + nudge * n[0],
                v[1] + nudge * n[1],
                v[2] + nudge * n[2],
            ])
        })
        .collect()
}

/// Projects a closed lat/lon ring (degrees) into one or more closed
/// map-plane rings, cutting it along the seam of (pseudo-)cylindrical
/// projections and along the horizon of hemispheric ones. Rings are
/// densified to 1° first so that long edges follow the projection's
/// curvature. Edges are taken literally in lat/lon (no wrapping).
pub(crate) fn project_ring(proj: &dyn Projection, ring: &[(f64, f64)], out: &mut Vec<Vec<Pt>>) {
    if ring.len() < 3 {
        return;
    }
    let dense = densify_ring(ring, 1.0);
    let mut push = |r: Vec<Pt>| {
        if r.len() >= 3 && ring_area(&r).abs() > 1e-12 {
            out.push(r);
        }
    };
    if let Some((clat, clon)) = proj.hemisphere_center() {
        let n = to_vec3(clat, clon);
        let v: Vec<[f64; 3]> = dense.iter().map(|&(la, lo)| to_vec3(la, lo)).collect();
        let clipped = clip_hemisphere(&v, n);
        let r: Vec<Pt> = clipped
            .into_iter()
            .filter_map(|v| {
                let (la, lo) = to_latlon(v);
                proj.forward(la, lo)
            })
            .collect();
        push(r);
    } else if let Some(seam) = proj.seam_lon_deg() {
        // Longitude relative to the seam, unwrapped continuously, so the
        // map spans [0, 360) in this coordinate.
        let mut rel = Vec::with_capacity(dense.len());
        let mut r = (dense[0].1 - seam).rem_euclid(360.0);
        rel.push((dense[0].0, r));
        for w in dense.windows(2) {
            r += normalize_lon(w[1].1 - w[0].1);
            rel.push((w[1].0, r));
        }
        let (lo, hi) = rel.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p.1), hi.max(p.1))
        });
        let k0 = (lo / 360.0).floor() as i64;
        let k1 = ((hi / 360.0).ceil() as i64).max(k0 + 1);
        for k in k0..k1 {
            let base = k as f64 * 360.0;
            let band = clip_lon(&clip_lon(&rel, base, true), base + 360.0, false);
            if band.len() < 3 {
                continue;
            }
            let r: Vec<Pt> = band
                .iter()
                .filter_map(|&(la, rr)| {
                    let rr = (rr - base).clamp(EPS_DEG, 360.0 - EPS_DEG);
                    proj.forward(la, seam + rr)
                })
                .collect();
            push(r);
        }
    } else {
        push(
            dense
                .iter()
                .filter_map(|&(la, lo)| proj.forward(la, lo))
                .collect(),
        );
    }
}

/// Drops consecutive points closer than `min_dist` (same units as the
/// points), always keeping the first and last.
pub(crate) fn simplify(pts: &[(f32, f32)], min_dist: f32, out: &mut Vec<(f32, f32)>) {
    out.clear();
    let Some((&first, rest)) = pts.split_first() else {
        return;
    };
    out.push(first);
    let d2 = min_dist * min_dist;
    for (i, &p) in rest.iter().enumerate() {
        let last = *out.last().unwrap_or(&first);
        let (dx, dy) = (p.0 - last.0, p.1 - last.1);
        if dx * dx + dy * dy >= d2 || i == rest.len() - 1 {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::{Equirectangular, Orthographic, Robinson};

    #[test]
    fn polyline_splits_at_seam() {
        let p = Equirectangular::world(0.0);
        let mut out = Vec::new();
        project_polyline(&p, [(10.0, 170.0), (10.0, -170.0)], &mut out);
        assert_eq!(out.len(), 2);
        assert!((out[0][1].0 - 1.0).abs() < 1e-6, "{:?}", out);
        assert!(out[1][0].0.abs() < 1e-6, "{:?}", out);
        // Not crossing the seam: one piece.
        out.clear();
        project_polyline(
            &Robinson::new(0.0),
            [(0.0, -10.0), (0.0, 10.0), (5.0, 20.0)],
            &mut out,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 3);
    }

    #[test]
    fn polyline_clips_at_horizon() {
        let p = Orthographic::new(0.0, 0.0);
        let mut out = Vec::new();
        project_polyline(&p, [(0.0, 80.0), (0.0, 100.0), (0.0, 120.0)], &mut out);
        assert_eq!(out.len(), 1);
        let end = out[0].last().unwrap();
        assert!((end.0 - 1.0).abs() < 1e-6, "{end:?}");
    }

    #[test]
    fn ring_is_cut_at_seam_and_horizon() {
        let square = [(10.0, 170.0), (10.0, 190.0), (-10.0, 190.0), (-10.0, 170.0)];
        let mut out = Vec::new();
        project_ring(&Equirectangular::world(0.0), &square, &mut out);
        assert_eq!(out.len(), 2);
        let total: f64 = out.iter().map(|r| ring_area(r).abs()).sum();
        let expect = (20.0 / 360.0) * (20.0 / 180.0);
        assert!((total - expect).abs() < 1e-6, "{total} vs {expect}");

        // A cap straddling the limb of a globe centred on (0, 0): half of
        // it is visible, and the cut follows the limb.
        out.clear();
        let cap = [(20.0, 70.0), (20.0, 110.0), (-20.0, 110.0), (-20.0, 70.0)];
        project_ring(&Orthographic::new(0.0, 0.0), &cap, &mut out);
        assert_eq!(out.len(), 1);
        assert!(
            out[0]
                .iter()
                .all(|p| (p.0 - 0.5).hypot(p.1 - 0.5) <= 0.5 + 1e-9)
        );
        let max_x = out[0].iter().map(|p| p.0).fold(0.0, f64::max);
        assert!((max_x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn graticule_skips_seam_meridian() {
        let lines = graticule_lines(30.0, Some(-180.0));
        // 11 meridians (no -180) + 5 parallels (-60..60).
        assert_eq!(lines.len(), 11 + 5);
    }

    #[test]
    fn simplify_keeps_ends() {
        let pts = [(0.0, 0.0), (0.1, 0.0), (0.2, 0.0), (5.0, 0.0), (5.1, 0.0)];
        let mut out = Vec::new();
        simplify(&pts, 1.0, &mut out);
        assert_eq!(out, vec![(0.0, 0.0), (5.0, 0.0), (5.1, 0.0)]);
    }
}
