//! A minimal line chart drawn with the egui painter (no plotting crate):
//! a frame, grid lines with labels, one polyline and optional markers.

use egui::{Color32, Pos2, Rect, Stroke, pos2, vec2};

/// A marked point with a label.
pub struct Marker {
    pub x: f64,
    pub y: f64,
    pub color: Color32,
    pub label: String,
}

/// What to draw.
pub struct Chart<'a> {
    pub x_range: (f64, f64),
    pub y_range: (f64, f64),
    pub x_ticks: &'a [(f64, String)],
    pub y_ticks: &'a [(f64, String)],
    pub x_label: &'a str,
    pub y_label: &'a str,
    /// Polyline points (x, y); a NaN breaks the line.
    pub points: &'a [[f64; 2]],
    pub markers: &'a [Marker],
    /// Keep one data unit the same length on both axes.
    pub equal_aspect: bool,
}

/// Maps data coordinates into a screen rectangle.
#[derive(Clone, Copy)]
pub struct Mapping {
    pub rect: Rect,
    x_range: (f64, f64),
    y_range: (f64, f64),
}

impl Mapping {
    pub fn point(self, x: f64, y: f64) -> Pos2 {
        let fx = (x - self.x_range.0) / (self.x_range.1 - self.x_range.0);
        let fy = (y - self.y_range.0) / (self.y_range.1 - self.y_range.0);
        pos2(
            self.rect.left() + fx as f32 * self.rect.width(),
            self.rect.bottom() - fy as f32 * self.rect.height(),
        )
    }
}

/// Draw `chart` into a new area `size` points large. Returns the response
/// (for hover/click) and the data mapping.
pub fn show(ui: &mut egui::Ui, size: egui::Vec2, chart: &Chart<'_>) -> (egui::Response, Mapping) {
    let (outer, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let visuals = ui.visuals();
    let text_color = visuals.text_color();
    let weak = visuals.weak_text_color();
    let grid = visuals.widgets.noninteractive.bg_stroke.color;
    let font = egui::FontId::proportional(11.0);
    let painter = ui.painter_at(outer);

    let mut plot = Rect::from_min_max(outer.min + vec2(52.0, 20.0), outer.max - vec2(12.0, 34.0));
    if chart.equal_aspect {
        let dx = chart.x_range.1 - chart.x_range.0;
        let dy = chart.y_range.1 - chart.y_range.0;
        let scale = (plot.width() as f64 / dx).min(plot.height() as f64 / dy);
        plot = Rect::from_center_size(
            plot.center(),
            vec2((dx * scale) as f32, (dy * scale) as f32),
        );
    }
    let map = Mapping {
        rect: plot,
        x_range: chart.x_range,
        y_range: chart.y_range,
    };

    painter.rect_filled(plot, 2.0, visuals.extreme_bg_color);
    for (x, label) in chart.x_ticks {
        let p = map.point(*x, chart.y_range.0);
        painter.line_segment(
            [pos2(p.x, plot.top()), pos2(p.x, plot.bottom())],
            Stroke::new(1.0, grid),
        );
        painter.text(
            p + vec2(0.0, 3.0),
            egui::Align2::CENTER_TOP,
            label,
            font.clone(),
            weak,
        );
    }
    for (y, label) in chart.y_ticks {
        let p = map.point(chart.x_range.0, *y);
        painter.line_segment(
            [pos2(plot.left(), p.y), pos2(plot.right(), p.y)],
            Stroke::new(1.0, grid),
        );
        painter.text(
            p - vec2(4.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            label,
            font.clone(),
            weak,
        );
    }
    painter.rect_stroke(plot, 2.0, Stroke::new(1.0, weak), egui::StrokeKind::Inside);
    painter.text(
        pos2(plot.center().x, outer.bottom() - 2.0),
        egui::Align2::CENTER_BOTTOM,
        chart.x_label,
        font.clone(),
        text_color,
    );
    painter.text(
        pos2(outer.left() + 2.0, plot.top() - 2.0),
        egui::Align2::LEFT_BOTTOM,
        chart.y_label,
        font.clone(),
        text_color,
    );

    let line = Stroke::new(1.8, visuals.hyperlink_color);
    let plot_painter = painter.with_clip_rect(plot.expand(1.0));
    let mut run: Vec<Pos2> = Vec::new();
    let flush = |run: &mut Vec<Pos2>| {
        if run.len() > 1 {
            plot_painter.add(egui::Shape::line(std::mem::take(run), line));
        }
        run.clear();
    };
    for p in chart.points {
        if p[0].is_finite() && p[1].is_finite() {
            run.push(map.point(p[0], p[1]));
        } else {
            flush(&mut run);
        }
    }
    flush(&mut run);

    for m in chart.markers {
        let p = map.point(m.x, m.y);
        plot_painter.circle_filled(p, 4.5, m.color);
        plot_painter.circle_stroke(p, 4.5, Stroke::new(1.0, text_color));
        if !m.label.is_empty() {
            plot_painter.text(
                p + vec2(7.0, -7.0),
                egui::Align2::LEFT_BOTTOM,
                &m.label,
                font.clone(),
                text_color,
            );
        }
    }
    (response, map)
}

/// Evenly spaced "nice" tick positions covering `[lo, hi]`.
pub fn nice_ticks(lo: f64, hi: f64, target: usize) -> Vec<f64> {
    let span = (hi - lo).abs().max(1e-12);
    let raw = span / target.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|m| m * mag)
        .find(|s| *s >= raw)
        .unwrap_or(10.0 * mag);
    let mut t = (lo / step).ceil() * step;
    let mut out = Vec::new();
    while t <= hi + step * 1e-9 {
        out.push(if t.abs() < step * 1e-9 { 0.0 } else { t });
        t += step;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_are_nice() {
        assert_eq!(
            nice_ticks(-8.0, 8.0, 8),
            vec![-8.0, -6.0, -4.0, -2.0, 0.0, 2.0, 4.0, 6.0, 8.0]
        );
        assert_eq!(
            nice_ticks(-16.0, 17.0, 8),
            vec![-15.0, -10.0, -5.0, 0.0, 5.0, 10.0, 15.0]
        );
    }

    #[test]
    fn mapping_maps_corners() {
        let m = Mapping {
            rect: Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 100.0)),
            x_range: (-5.0, 5.0),
            y_range: (0.0, 10.0),
        };
        assert_eq!(m.point(0.0, 10.0), pos2(110.0, 20.0));
        assert_eq!(m.point(-2.5, 5.0), pos2(60.0, 70.0));
    }
}
