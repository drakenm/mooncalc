//! A synthetic stand-in for the world visibility map, used until the grid
//! engine (`mooncalc_maps::grid`) and renderer (`mooncalc_maps::render`)
//! land. **Not astronomy:** a lat/lon gradient with a fake parabolic zone
//! band that drifts with the date, so the map tab, job system and map widget
//! can be exercised end to end. Delete this module when the real map exists.

use crate::jobs::{JobContext, JobError, parallel_rows};
use crate::settings::{Crescent, MapRequest};
use crate::widgets::map_view::MapImage;

/// Fake zone legend: (code, description, colour).
pub const ZONES: [(&str, &str, [u8; 3]); 4] = [
    ("A", "Easily visible (placeholder)", [46, 160, 67]),
    (
        "B",
        "Visible in perfect conditions (placeholder)",
        [210, 200, 40],
    ),
    (
        "C",
        "Optical aid may be needed (placeholder)",
        [235, 140, 30],
    ),
    ("D", "Optical aid needed (placeholder)", [200, 50, 40]),
];

/// Equirectangular inverse projection: normalised image position →
/// (lat, lon) in degrees.
pub fn equirect_inverse(x: f32, y: f32) -> Option<(f64, f64)> {
    let (x, y) = (f64::from(x), f64::from(y));
    ((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y))
        .then(|| (90.0 - 180.0 * y, -180.0 + 360.0 * x))
}

/// Equirectangular forward projection: (lat, lon) in degrees → normalised
/// image position.
pub fn equirect_forward(lat_deg: f64, lon_deg: f64) -> Option<(f32, f32)> {
    let lon = crate::location::normalize_lon(lon_deg);
    Some((
        ((lon + 180.0) / 360.0) as f32,
        ((90.0 - lat_deg) / 180.0) as f32,
    ))
}

/// Render the placeholder map (`width × height`, equirectangular).
pub fn render(
    req: &MapRequest,
    width: usize,
    height: usize,
    job: &JobContext,
) -> Result<MapImage, JobError> {
    // Mean new Moon epoch (Meeus eq. 49.1, k = 0), only to make the band move.
    let days_since_new =
        (req.jd_ut - 2_451_550.097_66).rem_euclid(crate::datetime::SYNODIC_MONTH_DAYS);
    let vertex_lon = crate::location::normalize_lon(150.0 - 40.0 * days_since_new);
    let criterion_shift = req.criterion_id.bytes().map(f64::from).sum::<f64>() % 20.0;
    let curvature = 0.010 + criterion_shift * 0.0005;
    let mirror = req.crescent == Crescent::Morning;

    let rows = parallel_rows(job, height, |row| {
        let mut out = Vec::with_capacity(width * 4);
        let lat = 90.0 - 180.0 * (row as f64 + 0.5) / height as f64;
        let lat_step = 180.0 / height as f64;
        for col in 0..width {
            let lon = -180.0 + 360.0 * (col as f64 + 0.5) / width as f64;
            let lon_step = 360.0 / width as f64;
            let t = 1.0 - lat.abs() / 90.0;
            let mut rgb = [
                25.0 + 35.0 * t + 10.0 * (lon.to_radians().cos() + 1.0),
                55.0 + 70.0 * t,
                105.0 + 85.0 * t,
            ];
            let curve = vertex_lon - curvature * lat * lat;
            let mut d = crate::location::normalize_lon(curve - lon);
            if mirror {
                d = -d;
            }
            let zone = match d {
                d if (0.0..15.0).contains(&d) => Some(3),
                d if (15.0..30.0).contains(&d) => Some(2),
                d if (30.0..50.0).contains(&d) => Some(1),
                d if (50.0..100.0).contains(&d) && lat.abs() < 60.0 => Some(0),
                _ => None,
            };
            if let Some(z) = zone {
                let c = ZONES[z].2;
                for (v, c) in rgb.iter_mut().zip(c) {
                    *v = *v * 0.35 + f64::from(c) * 0.65;
                }
            }
            // Graticule every 30°, one pixel wide.
            let on_grid = |v: f64, step: f64| (v + 180.0).rem_euclid(30.0) < step;
            if on_grid(lat, lat_step) || on_grid(lon, lon_step) {
                rgb = rgb.map(|v| v * 0.7);
            }
            out.extend(rgb.map(|v| v.round().clamp(0.0, 255.0) as u8));
            out.push(255);
        }
        out
    })?;
    let pixels = rows.concat();
    MapImage::new(width, height, pixels).ok_or_else(|| JobError::Failed("bad image size".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BackendChoice;

    fn request() -> MapRequest {
        MapRequest {
            jd_ut: 2_461_000.5,
            criterion_id: "yallop".into(),
            crescent: Crescent::Evening,
            backend: BackendChoice::Meeus,
            de440_path: String::new(),
        }
    }

    #[test]
    fn renders_an_opaque_image_of_the_right_size() {
        let img = render(&request(), 72, 36, &JobContext::detached()).unwrap();
        assert_eq!(
            (img.width, img.height, img.pixels.len()),
            (72, 36, 72 * 36 * 4)
        );
        assert!(img.pixels.chunks(4).all(|p| p[3] == 255));
        // Changing the date moves the band.
        let mut later = request();
        later.jd_ut += 1.0;
        assert_ne!(
            render(&later, 72, 36, &JobContext::detached()).unwrap(),
            img
        );
    }

    #[test]
    fn equirect_projection_round_trips() {
        let (lat, lon) = equirect_inverse(0.25, 0.75).unwrap();
        assert_eq!((lat, lon), (-45.0, -90.0));
        let (x, y) = equirect_forward(lat, lon).unwrap();
        assert!((x - 0.25).abs() < 1e-6 && (y - 0.75).abs() < 1e-6);
        assert_eq!(equirect_inverse(1.5, 0.5), None);
    }
}
