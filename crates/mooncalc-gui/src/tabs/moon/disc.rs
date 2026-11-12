//! The Moon close-up drawn with egui: the lit disc rasterised into a
//! texture, and the feature overlay. The projection ([`View`], Meeus ch. 53:
//! orthographic around the sub-observer point, rotated by P or P − q) is
//! shared with the Tauri frontend in [`mooncalc_app::moon::disc`].

use std::f64::consts::FRAC_PI_2;

use egui::{Color32, Pos2, Stroke, vec2};

use super::features::{self, CRATERS, MARIA, angular_radius};

// The projection itself is shared with the Tauri frontend.
pub use mooncalc_app::moon::disc::View;

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Render the disc into an `n`×`n` RGBA image (transparent outside).
pub fn rasterize(view: &View, n: usize, maria: bool) -> egui::ColorImage {
    let mask = features::mare_mask();
    let mut pixels = vec![Color32::TRANSPARENT; n * n];
    let half = n as f64 / 2.0;
    for j in 0..n {
        let v = 1.0 - (j as f64 + 0.5) / half;
        for i in 0..n {
            let u = (i as f64 + 0.5) / half - 1.0;
            let r = (u * u + v * v).sqrt();
            if r > 1.0 + 1.0 / half {
                continue;
            }
            let (uu, vv) = if r > 1.0 { (u / r, v / r) } else { (u, v) };
            let Some((eta, theta)) = view.unproject(uu * 0.9999, vv * 0.9999) else {
                continue;
            };
            let albedo = if maria && mask.at(theta, eta) {
                0.60
            } else {
                0.88
            };
            let dot = view.sun_dot(eta, theta);
            let lit = smoothstep(-0.012, 0.03, dot);
            let day = albedo * (0.80 + 0.20 * dot.max(0.0).sqrt());
            let night = albedo * 0.14;
            let value = night + (day - night) * lit;
            let alpha = ((1.0 - r) * half + 0.5).clamp(0.0, 1.0);
            let c = |k: f64| (value * k * alpha * 255.0).round().clamp(0.0, 255.0) as u8;
            pixels[j * n + i] =
                Color32::from_rgba_premultiplied(c(1.0), c(0.98), c(0.93), (alpha * 255.0) as u8);
        }
    }
    egui::ColorImage::new([n, n], pixels)
}

/// Overlay options.
#[derive(Clone, Copy, Debug)]
pub struct Overlay<'a> {
    pub maria: bool,
    pub craters: bool,
    pub labels: bool,
    pub highlight: Option<&'a str>,
}

/// Draw the feature overlay for a disc of `radius` at `center`.
pub fn draw_overlay(
    painter: &egui::Painter,
    view: &View,
    center: Pos2,
    radius: f32,
    overlay: &Overlay<'_>,
) {
    let to_screen = |u: f64, v: f64| center + vec2(u as f32 * radius, -v as f32 * radius);
    let font = egui::FontId::proportional(11.0);
    let path = |pts: &[(f64, f64)], stroke: Stroke| {
        let mut run: Vec<Pos2> = Vec::new();
        for &(lat, lon) in pts {
            let (u, v, z) = view.project(lon, lat);
            if z > 0.0 {
                run.push(to_screen(u, v));
            } else if !run.is_empty() {
                if run.len() > 1 {
                    painter.add(egui::Shape::line(std::mem::take(&mut run), stroke));
                }
                run.clear();
            }
        }
        if run.len() > 1 {
            painter.add(egui::Shape::line(run, stroke));
        }
    };

    if overlay.maria {
        let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(110, 170, 255, 150));
        for m in &MARIA {
            path(&features::outline(m), stroke);
            if overlay.labels {
                let (u, v, z) = view.project(m.lon.to_radians(), m.lat.to_radians());
                if z > 0.25 {
                    painter.text(
                        to_screen(u, v),
                        egui::Align2::CENTER_CENTER,
                        m.name
                            .trim_start_matches("Mare ")
                            .trim_start_matches("Oceanus "),
                        egui::FontId::proportional(10.0),
                        Color32::from_rgba_unmultiplied(150, 200, 255, 200),
                    );
                }
            }
        }
    }
    if overlay.craters {
        for c in &CRATERS {
            let selected = overlay.highlight == Some(c.name);
            let color = if selected {
                Color32::from_rgb(255, 170, 40)
            } else {
                Color32::from_rgba_unmultiplied(255, 120, 90, 190)
            };
            let (lat, lon) = (c.lat.to_radians(), c.lon.to_radians());
            let r = angular_radius(c.diameter_km).max(0.012);
            let ring = features::small_circle(lat, lon, r, 24);
            path(&ring, Stroke::new(if selected { 2.0 } else { 1.0 }, color));
            let (u, v, z) = view.project(lon, lat);
            if overlay.labels && z > 0.08 {
                painter.text(
                    to_screen(u, v) + vec2(5.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    c.name,
                    font.clone(),
                    color,
                );
            }
        }
    }

    // The mean centre of the disc (0°, 0°): libration moves it.
    let (u, v, z) = view.project(0.0, 0.0);
    if z > 0.0 {
        let p = to_screen(u, v);
        let s = Stroke::new(1.2, Color32::from_rgb(120, 230, 140));
        painter.line_segment([p - vec2(5.0, 0.0), p + vec2(5.0, 0.0)], s);
        painter.line_segment([p - vec2(0.0, 5.0), p + vec2(0.0, 5.0)], s);
    }
    // The lunar north pole: a tick outside the limb at position angle P.
    let (su, sv) = (-view.rotation.sin(), view.rotation.cos());
    let a = to_screen(su * 1.02, sv * 1.02);
    let b = to_screen(su * 1.12, sv * 1.12);
    let pole = Color32::from_rgb(120, 230, 140);
    painter.line_segment([a, b], Stroke::new(1.5, pole));
    painter.text(
        to_screen(su * 1.19, sv * 1.19),
        egui::Align2::CENTER_CENTER,
        "Np",
        font.clone(),
        pole,
    );
    let (u, v, z) = view.project(0.0, FRAC_PI_2);
    if z > 0.0 {
        painter.circle_filled(to_screen(u, v), 2.5, pole);
    }
}

/// Draw a direction marker (an arrow outside the limb) at screen angle
/// `angle` (anticlockwise from up) with a label.
pub fn draw_direction(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    angle: f64,
    label: &str,
    color: Color32,
) {
    let dir = vec2(-(angle.sin() as f32), -(angle.cos() as f32));
    let a = center + dir * (radius * 1.25);
    let b = center + dir * (radius * 1.38);
    painter.arrow(a, b - a, Stroke::new(1.5, color));
    painter.text(
        center + dir * (radius * 1.47),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(12.0),
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::libration::physical_ephemeris;
    use mooncalc_core::{Instant, MeeusEphemeris};

    #[test]
    fn crescent_image_is_lit_on_the_right() {
        // 2024 March 14, 16:00 UT: a waxing crescent about 20 % lit.
        let eph = MeeusEphemeris::new();
        let jd = Instant::from_calendar(2024, 3, 14, 16, 0, 0.0).jd_tt();
        let p = physical_ephemeris(&eph, jd);
        let view = View {
            l: p.libration.lon,
            b: p.libration.lat,
            sun_lon: p.sun_lon,
            sun_lat: p.sun_lat,
            rotation: p.libration.position_angle,
        };
        let n = 64;
        let img = rasterize(&view, n, false);
        // Mean brightness of the right and left halves of the disc.
        let half = |right: bool| {
            let (mut sum, mut count) = (0.0, 0.0);
            for j in 0..n {
                for i in 0..n {
                    let px = img.pixels[j * n + i];
                    if px.a() == 255 && (i >= n / 2) == right {
                        sum += f64::from(px.r());
                        count += 1.0;
                    }
                }
            }
            sum / count
        };
        assert!(
            half(true) > 2.0 * half(false),
            "{} {}",
            half(true),
            half(false)
        );
        // The rightmost pixel of the middle row is lit, the leftmost dark.
        assert!(img.pixels[32 * n + 62].r() > 150);
        assert!(img.pixels[32 * n + 1].r() < 60);
        assert_eq!(img.pixels[0].a(), 0); // outside the disc
        assert!((view.illuminated_fraction() - 0.2).abs() < 0.08);
    }
}
