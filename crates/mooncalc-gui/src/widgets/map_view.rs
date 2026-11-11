//! Zoomable, pannable raster map.
//!
//! [`MapView`] shows an RGBA8 image ([`MapImage`]) as a texture, zooms with
//! the mouse wheel (or pinch) about the cursor, pans by dragging, and resets
//! on double-click. The caller supplies the projection as callbacks on
//! normalised image coordinates (`x_norm`, `y_norm` in `[0, 1]`, origin at
//! the top-left):
//!
//! * `inverse(x_norm, y_norm) -> Option<(lat_deg, lon_deg)>` (required):
//!   turns the cursor into a geographic position (`None` off the globe).
//! * `forward(lat_deg, lon_deg) -> Option<(x_norm, y_norm)>` (optional):
//!   places the observer marker. Without it, the marker is drawn where the
//!   user last clicked.
//!
//! The widget knows nothing about visibility: the map tab renders the grid to
//! pixels and passes them in with [`MapView::set_image`].

use egui::{Color32, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2};

/// An RGBA8 image (unmultiplied alpha, row-major, top row first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapImage {
    pub width: usize,
    pub height: usize,
    /// `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

impl MapImage {
    /// Wrap a pixel buffer; `None` if its length doesn't match.
    pub fn new(width: usize, height: usize, pixels: Vec<u8>) -> Option<Self> {
        (width > 0 && height > 0 && pixels.len() == width * height * 4).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn aspect(&self) -> f32 {
        self.width as f32 / self.height as f32
    }
}

/// Inverse projection: normalised image position → (lat, lon) in degrees.
pub type InverseProjection<'a> = &'a dyn Fn(f32, f32) -> Option<(f64, f64)>;
/// Forward projection: (lat, lon) in degrees → normalised image position.
pub type ForwardProjection<'a> = &'a dyn Fn(f64, f64) -> Option<(f32, f32)>;

/// What happened in the map this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MapResponse {
    /// Geographic position under the cursor.
    pub hover: Option<(f64, f64)>,
    /// Geographic position clicked (not dragged) this frame.
    pub clicked: Option<(f64, f64)>,
}

/// Zoom and pan state, independent of egui so it can be unit-tested.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewTransform {
    /// Magnification relative to the image fitted in the widget (≥ 1).
    pub zoom: f32,
    /// Offset of the image centre from the widget centre, in points.
    pub pan: Vec2,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

impl ViewTransform {
    pub const MAX_ZOOM: f32 = 64.0;

    /// The largest rect of the given aspect ratio centred in `area`.
    pub fn fit(area: Rect, aspect: f32) -> Rect {
        let size = if area.width() / area.height() > aspect {
            Vec2::new(area.height() * aspect, area.height())
        } else {
            Vec2::new(area.width(), area.width() / aspect)
        };
        Rect::from_center_size(area.center(), size)
    }

    /// Where the image is drawn, given the fitted rect.
    pub fn image_rect(&self, fitted: Rect) -> Rect {
        Rect::from_center_size(fitted.center() + self.pan, fitted.size() * self.zoom)
    }

    /// Multiply the zoom by `factor`, keeping the image point under `anchor`
    /// fixed on screen.
    pub fn zoom_about(&mut self, fitted: Rect, anchor: Pos2, factor: f32) {
        let new_zoom = (self.zoom * factor).clamp(1.0, Self::MAX_ZOOM);
        let ratio = new_zoom / self.zoom;
        let centre = fitted.center() + self.pan;
        let new_centre = anchor - (anchor - centre) * ratio;
        self.zoom = new_zoom;
        self.pan = new_centre - fitted.center();
        self.clamp(fitted);
    }

    /// Keep the fitted area covered by the image (no panning past an edge).
    pub fn clamp(&mut self, fitted: Rect) {
        let max = (fitted.size() * self.zoom - fitted.size()) * 0.5;
        self.pan.x = self.pan.x.clamp(-max.x, max.x);
        self.pan.y = self.pan.y.clamp(-max.y, max.y);
    }

    /// Normalised image position of a screen point (may lie outside [0, 1]).
    pub fn screen_to_norm(&self, fitted: Rect, p: Pos2) -> (f32, f32) {
        let r = self.image_rect(fitted);
        ((p.x - r.min.x) / r.width(), (p.y - r.min.y) / r.height())
    }

    /// Screen point of a normalised image position.
    pub fn norm_to_screen(&self, fitted: Rect, x: f32, y: f32) -> Pos2 {
        let r = self.image_rect(fitted);
        Pos2::new(r.min.x + x * r.width(), r.min.y + y * r.height())
    }
}

/// The map widget's retained state (texture, zoom/pan, click marker).
pub struct MapView {
    id: &'static str,
    texture: Option<TextureHandle>,
    pending: Option<MapImage>,
    aspect: f32,
    transform: ViewTransform,
    /// Last clicked position in normalised coordinates (marker fallback).
    clicked_norm: Option<(f32, f32)>,
}

impl MapView {
    /// `id` names the texture; use a different one per map.
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            texture: None,
            pending: None,
            aspect: 2.0,
            transform: ViewTransform::default(),
            clicked_norm: None,
        }
    }

    /// Replace the image (uploaded on the next [`Self::show`]).
    pub fn set_image(&mut self, image: MapImage) {
        self.pending = Some(image);
    }

    pub fn has_image(&self) -> bool {
        self.texture.is_some() || self.pending.is_some()
    }

    /// Reset zoom and pan.
    pub fn reset_view(&mut self) {
        self.transform = ViewTransform::default();
    }

    pub fn transform(&self) -> ViewTransform {
        self.transform
    }

    /// Draw the map in all available space and handle input.
    ///
    /// `marker` is the observer position to mark (needs `forward`, otherwise
    /// the last click position is marked).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        inverse: InverseProjection<'_>,
        forward: Option<ForwardProjection<'_>>,
        marker: Option<(f64, f64)>,
    ) -> MapResponse {
        self.upload_pending(ui.ctx());

        let size = ui.available_size().max(Vec2::splat(32.0));
        let (area, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let fitted = ViewTransform::fit(area, self.aspect);
        let mut out = MapResponse::default();

        // Zoom (wheel / pinch) about the cursor.
        if let Some(pointer) = response.hover_pos() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = (scroll / 200.0).exp() * pinch;
            if (factor - 1.0).abs() > 1e-4 {
                self.transform.zoom_about(fitted, pointer, factor);
            }
        }
        if response.dragged() {
            self.transform.pan += response.drag_delta();
            self.transform.clamp(fitted);
        }
        if response.double_clicked() {
            self.reset_view();
        }

        let to_geo = |p: Pos2, t: &ViewTransform| {
            let (x, y) = t.screen_to_norm(fitted, p);
            ((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y))
                .then(|| inverse(x, y))
                .flatten()
                .map(|geo| (geo, (x, y)))
        };
        if let Some(p) = response.hover_pos() {
            out.hover = to_geo(p, &self.transform).map(|(geo, _)| geo);
        }
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
            && let Some((geo, norm)) = to_geo(p, &self.transform)
        {
            out.clicked = Some(geo);
            self.clicked_norm = Some(norm);
        }

        // Paint.
        let painter = ui.painter_at(area);
        let visuals = ui.visuals();
        painter.rect_filled(area, 0.0, visuals.extreme_bg_color);
        let image_rect = self.transform.image_rect(fitted);
        match &self.texture {
            Some(tex) => {
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                painter.image(tex.id(), image_rect, uv, Color32::WHITE);
            }
            None => {
                painter.text(
                    area.center(),
                    egui::Align2::CENTER_CENTER,
                    "Computing map…",
                    egui::FontId::proportional(16.0),
                    visuals.weak_text_color(),
                );
            }
        }

        let marker_norm = match (forward, marker) {
            (Some(fwd), Some((lat, lon))) => fwd(lat, lon),
            _ => self.clicked_norm,
        };
        if let Some((x, y)) = marker_norm {
            let p = self.transform.norm_to_screen(fitted, x, y);
            let outline = Stroke::new(3.0, Color32::BLACK);
            let inner = Stroke::new(1.5, Color32::WHITE);
            for stroke in [outline, inner] {
                painter.circle_stroke(p, 6.0, stroke);
                painter.line_segment([p - Vec2::X * 11.0, p - Vec2::X * 7.0], stroke);
                painter.line_segment([p + Vec2::X * 7.0, p + Vec2::X * 11.0], stroke);
                painter.line_segment([p - Vec2::Y * 11.0, p - Vec2::Y * 7.0], stroke);
                painter.line_segment([p + Vec2::Y * 7.0, p + Vec2::Y * 11.0], stroke);
            }
        }

        let hint = format!(
            "Wheel: zoom ({:.1}×) · Drag: pan · Double-click: reset · Click: set location",
            self.transform.zoom
        );
        let hint_pos = area.left_bottom() + Vec2::new(6.0, -6.0);
        let galley = painter.layout_no_wrap(
            hint,
            egui::FontId::proportional(11.0),
            Color32::from_gray(235),
        );
        let bg = Rect::from_min_size(
            hint_pos - Vec2::new(3.0, galley.size().y + 2.0),
            galley.size() + Vec2::new(6.0, 4.0),
        );
        painter.rect_filled(bg, 3.0, Color32::from_black_alpha(140));
        painter.galley(
            hint_pos - Vec2::new(0.0, galley.size().y),
            galley,
            Color32::WHITE,
        );

        if out.hover.is_some() {
            ui.ctx().set_cursor_icon(if response.dragged() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Crosshair
            });
        }
        out
    }

    fn upload_pending(&mut self, ctx: &egui::Context) {
        let Some(image) = self.pending.take() else {
            return;
        };
        let max_side = ctx.input(|i| i.max_texture_side);
        if image.width > max_side || image.height > max_side {
            eprintln!(
                "mooncalc: map image {}×{} exceeds the GPU texture limit {max_side}",
                image.width, image.height
            );
            return;
        }
        self.aspect = image.aspect();
        let color =
            egui::ColorImage::from_rgba_unmultiplied([image.width, image.height], &image.pixels);
        match &mut self.texture {
            Some(tex) => tex.set(color, TextureOptions::LINEAR),
            None => self.texture = Some(ctx.load_texture(self.id, color, TextureOptions::LINEAR)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(400.0, 300.0))
    }

    #[test]
    fn fit_preserves_aspect_and_centres() {
        let f = ViewTransform::fit(area(), 2.0);
        assert_eq!(f.size(), Vec2::new(400.0, 200.0));
        assert_eq!(f.center(), area().center());
        let tall = ViewTransform::fit(area(), 1.0);
        assert_eq!(tall.size(), Vec2::new(300.0, 300.0));
    }

    #[test]
    fn zoom_keeps_the_anchor_point_fixed() {
        let fitted = ViewTransform::fit(area(), 2.0);
        let mut t = ViewTransform::default();
        let anchor = Pos2::new(100.0, 150.0);
        let before = t.screen_to_norm(fitted, anchor);
        t.zoom_about(fitted, anchor, 3.0);
        let after = t.screen_to_norm(fitted, anchor);
        assert!((before.0 - after.0).abs() < 1e-5 && (before.1 - after.1).abs() < 1e-5);
        assert_eq!(t.zoom, 3.0);
    }

    #[test]
    fn zoom_is_clamped_and_pan_cannot_leave_the_image() {
        let fitted = ViewTransform::fit(area(), 2.0);
        let mut t = ViewTransform::default();
        t.zoom_about(fitted, fitted.center(), 0.5);
        assert_eq!(t.zoom, 1.0);
        t.pan = Vec2::new(500.0, -500.0);
        t.clamp(fitted);
        assert_eq!(t.pan, Vec2::ZERO);
        t.zoom_about(fitted, fitted.center(), 1000.0);
        assert_eq!(t.zoom, ViewTransform::MAX_ZOOM);
        t.pan = Vec2::new(1e6, 1e6);
        t.clamp(fitted);
        let r = t.image_rect(fitted);
        assert!(r.min.x <= fitted.min.x + 1e-3 && r.min.y <= fitted.min.y + 1e-3);
    }

    #[test]
    fn norm_and_screen_are_inverse() {
        let fitted = ViewTransform::fit(area(), 2.0);
        let mut t = ViewTransform::default();
        t.zoom_about(fitted, Pos2::new(50.0, 60.0), 2.5);
        let p = t.norm_to_screen(fitted, 0.3, 0.7);
        let (x, y) = t.screen_to_norm(fitted, p);
        assert!((x - 0.3).abs() < 1e-5 && (y - 0.7).abs() < 1e-5);
    }

    #[test]
    fn image_buffer_length_is_checked() {
        assert!(MapImage::new(2, 2, vec![0; 16]).is_some());
        assert!(MapImage::new(2, 2, vec![0; 15]).is_none());
        assert!(MapImage::new(0, 2, vec![]).is_none());
    }
}
