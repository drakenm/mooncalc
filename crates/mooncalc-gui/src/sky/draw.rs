//! Painter helpers shared by the sky and trajectory views.

use egui::epaint::{Mesh, PathShape, PathStroke};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};

use super::geometry::{crescent_rows, moon_frame_to_screen, star_radius, star_tint};

/// Colour of the Moon's lit limb.
pub const MOON_LIT: Color32 = Color32::from_rgb(250, 244, 214);
/// The unlit disc (a faint earthshine).
pub const MOON_DARK: Color32 = Color32::from_rgba_premultiplied(22, 25, 34, 110);
/// The Sun.
pub const SUN: Color32 = Color32::from_rgb(255, 196, 64);
/// The Sun's path.
pub const SUN_PATH: Color32 = Color32::from_rgb(255, 170, 60);
/// The Moon's path.
pub const MOON_PATH: Color32 = Color32::from_rgb(190, 205, 235);

/// An sRGB colour from 0–1 components.
pub fn rgb(c: [f32; 3]) -> Color32 {
    let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(b(c[0]), b(c[1]), b(c[2]))
}

/// Fill `rect` with a grid of `nx × ny` cells coloured by `color(point)` at
/// each vertex (Gouraud-shaded gradient).
pub fn gradient_rect(
    painter: &Painter,
    rect: Rect,
    nx: usize,
    ny: usize,
    color: impl Fn(Pos2) -> Color32,
) {
    let mut mesh = Mesh::default();
    for j in 0..=ny {
        for i in 0..=nx {
            let p = pos2(
                rect.left() + rect.width() * i as f32 / nx as f32,
                rect.top() + rect.height() * j as f32 / ny as f32,
            );
            mesh.colored_vertex(p, color(p));
        }
    }
    let w = (nx + 1) as u32;
    for j in 0..ny as u32 {
        for i in 0..nx as u32 {
            let a = j * w + i;
            mesh.add_triangle(a, a + 1, a + w);
            mesh.add_triangle(a + 1, a + w + 1, a + w);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Fill a disc in `rings × sectors` cells coloured by `color(point)`.
pub fn gradient_disc(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    rings: usize,
    sectors: usize,
    color: impl Fn(Pos2) -> Color32,
) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color(center));
    for r in 1..=rings {
        let rr = radius * r as f32 / rings as f32;
        for s in 0..sectors {
            let a = std::f32::consts::TAU * s as f32 / sectors as f32;
            let p = center + rr * vec2(a.cos(), a.sin());
            mesh.colored_vertex(p, color(p));
        }
    }
    let sec = sectors as u32;
    let idx = |r: u32, s: u32| 1 + (r - 1) * sec + (s % sec);
    for s in 0..sec {
        mesh.add_triangle(0, idx(1, s), idx(1, s + 1));
    }
    for r in 1..rings as u32 {
        for s in 0..sec {
            mesh.add_triangle(idx(r, s), idx(r + 1, s), idx(r, s + 1));
            mesh.add_triangle(idx(r, s + 1), idx(r + 1, s), idx(r + 1, s + 1));
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Thinnest lit width drawn, pixels: a 1–2 % crescent on a 20-pixel disc
/// would otherwise be sub-pixel and vanish.
const MIN_LIT_WIDTH_PX: f32 = 1.6;

/// Draw the Moon as a phased, oriented disc: the dark disc, then the lit
/// part with illuminated fraction `fraction`, its bright limb toward the
/// screen direction `limb_dir`. The lit width is drawn at least
/// [`MIN_LIT_WIDTH_PX`] wide (when the fraction is positive), and the lit
/// limb is outlined, so thin crescents stay visible.
pub fn moon(painter: &Painter, center: Pos2, radius: f32, fraction: f64, limb_dir: Vec2) {
    painter.circle_filled(center, radius, MOON_DARK);
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(0.6, Color32::from_white_alpha(28)),
    );
    if fraction <= 0.0 {
        return;
    }
    let min_k = f64::from(MIN_LIT_WIDTH_PX / (2.0 * radius.max(1.0)));
    let rows = crescent_rows(fraction.max(min_k), 48);
    let mut mesh = Mesh::default();
    for (limb, term) in &rows {
        mesh.colored_vertex(
            moon_frame_to_screen(center, radius, limb_dir, *limb),
            MOON_LIT,
        );
        mesh.colored_vertex(
            moon_frame_to_screen(center, radius, limb_dir, *term),
            MOON_LIT,
        );
    }
    for i in 0..(rows.len() as u32 - 1) {
        let a = 2 * i;
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 3, a + 2);
    }
    painter.add(Shape::mesh(mesh));
    let limb: Vec<Pos2> = rows
        .iter()
        .map(|(l, _)| moon_frame_to_screen(center, radius, limb_dir, *l))
        .collect();
    painter.add(Shape::line(limb, Stroke::new(1.0, MOON_LIT)));
}

/// Draw the Sun; dimmed with a dashed-looking outline when below the
/// horizon.
pub fn sun(painter: &Painter, center: Pos2, radius: f32, below_horizon: bool) {
    if below_horizon {
        painter.circle_filled(center, radius, SUN.gamma_multiply(0.35));
        painter.circle_stroke(center, radius, Stroke::new(1.5, SUN));
    } else {
        painter.circle_filled(center, radius * 1.8, SUN.gamma_multiply(0.25));
        painter.circle_filled(center, radius, SUN);
    }
}

/// Draw one star.
pub fn star(painter: &Painter, p: Pos2, vmag: f64, bv: f64, alpha: f32) {
    let c = rgb(star_tint(bv)).gamma_multiply(alpha);
    painter.circle_filled(p, star_radius(vmag), c);
}

/// A polyline, split wherever `keep` is false or consecutive points jump
/// further than `max_jump` pixels (wrap-around in a panorama).
pub fn polyline(painter: &Painter, points: &[(Pos2, bool)], stroke: Stroke, max_jump: f32) {
    let mut run: Vec<Pos2> = Vec::new();
    let flush = |run: &mut Vec<Pos2>| {
        if run.len() >= 2 {
            painter.add(Shape::Path(PathShape::line(
                std::mem::take(run),
                PathStroke::from(stroke),
            )));
        }
        run.clear();
    };
    for &(p, keep) in points {
        if !keep {
            flush(&mut run);
            continue;
        }
        if let Some(last) = run.last()
            && (p - *last).length() > max_jump
        {
            flush(&mut run);
        }
        run.push(p);
    }
    flush(&mut run);
}

/// Text with a dark halo for legibility on any sky.
pub fn label(
    painter: &Painter,
    pos: Pos2,
    anchor: Align2,
    text: impl ToString,
    size: f32,
    color: Color32,
) {
    let font = FontId::proportional(size);
    let text = text.to_string();
    for d in [
        vec2(1.0, 1.0),
        vec2(-1.0, 1.0),
        vec2(1.0, -1.0),
        vec2(-1.0, -1.0),
    ] {
        painter.text(
            pos + d,
            anchor,
            &text,
            font.clone(),
            Color32::from_black_alpha(160),
        );
    }
    painter.text(pos, anchor, text, font, color);
}

/// A translucent box of text lines in a corner of `rect`.
pub fn info_box(painter: &Painter, rect: Rect, anchor: Align2, lines: &[(String, Color32)]) {
    if lines.is_empty() {
        return;
    }
    let font = FontId::monospace(12.0);
    let galleys: Vec<_> = lines
        .iter()
        .map(|(t, c)| painter.layout_no_wrap(t.clone(), font.clone(), *c))
        .collect();
    let w = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max);
    let h: f32 = galleys.iter().map(|g| g.size().y).sum();
    let pad = 6.0;
    let size = vec2(w + 2.0 * pad, h + 2.0 * pad);
    let margin = 8.0;
    let min = pos2(
        match anchor.x() {
            egui::Align::Min => rect.left() + margin,
            egui::Align::Center => rect.center().x - size.x / 2.0,
            egui::Align::Max => rect.right() - margin - size.x,
        },
        match anchor.y() {
            egui::Align::Min => rect.top() + margin,
            egui::Align::Center => rect.center().y - size.y / 2.0,
            egui::Align::Max => rect.bottom() - margin - size.y,
        },
    );
    let bg = Rect::from_min_size(min, size);
    painter.rect_filled(bg, 4.0, Color32::from_black_alpha(150));
    let mut y = min.y + pad;
    for g in galleys {
        let gh = g.size().y;
        painter.galley(pos2(min.x + pad, y), g, Color32::WHITE);
        y += gh;
    }
}
