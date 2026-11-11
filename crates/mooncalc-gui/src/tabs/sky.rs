//! **Sky** tab: the evening (or morning) sky of a crescent sighting, in the
//! spirit of MoonCalc 6's local sky view and star chart.
//!
//! Three views share a time slider spanning sunset → moonset (moonrise →
//! sunrise for the morning crescent), with a play button:
//!
//! * **Horizon**: a panorama of the western (eastern) sky with the horizon,
//!   cardinal points, twilight shading by the Sun's altitude, the Sun below
//!   the horizon, the Moon as a phased crescent oriented in the observer's
//!   zenith frame (`χ − q`, see [`crate::sky::geometry`]), the Sun's and
//!   Moon's paths with 10-minute dots, and an ARCV/DAZ annotation.
//!   Wheel zooms, drag pans, double-click resets.
//! * **All-sky**: the whole sky with the zenith at the centre.
//! * **Star chart**: RA/Dec with the ecliptic, the horizon, the Sun, the
//!   Moon and the bright stars of date.
//!
//! The local date and the evening/morning choice come from the global
//! inputs; times are shown in the app's time zone. The night is computed on
//! a background job ([`crate::jobs`], cancel-and-replace on new inputs);
//! the previous night stays on screen until the new one arrives. A date
//! outside the ephemeris coverage shows a notice instead. The view options
//! persist in [`SkySettings`].

use std::sync::Arc;

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, pos2, vec2};
use mooncalc_core::angle::{DEG, norm_2pi, separation};
use mooncalc_core::nutation::NutationObliquity;
use mooncalc_core::refraction::apparent_altitude;
use mooncalc_core::topocentric::{moon_semidiameter, sun_semidiameter};
use mooncalc_core::transform::{
    ecliptic_to_equatorial, equatorial_to_horizontal, horizontal_to_equatorial,
};
use mooncalc_core::{Ecliptic, Equatorial, Horizontal, Instant};
use mooncalc_visibility::circumstances::{Outcome, Polar};
use serde::{Deserialize, Serialize};

use super::ephemeris::cached::CachedJob;
use super::{Tab, TabContext};
use crate::settings::Crescent;
use crate::sky::draw::{self, MOON_PATH, SUN_PATH};
use crate::sky::geometry::{
    AllSkyProjection, HorizonProjection, StarChartProjection, compass_point, limb_direction,
    phase_name, sky_color, star_visibility,
};
use crate::sky::scene::{ATMOSPHERE, Night, NightKey, Snapshot, civil_noon, ephemeris};
use crate::sky::{format_date_hm, format_hm};
use crate::widgets::coverage::coverage_notice;

/// The three views.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkyView {
    #[default]
    Horizon,
    AllSky,
    StarChart,
}

impl SkyView {
    const ALL: [SkyView; 3] = [SkyView::Horizon, SkyView::AllSky, SkyView::StarChart];

    fn label(self) -> &'static str {
        match self {
            SkyView::Horizon => "Horizon",
            SkyView::AllSky => "All-sky",
            SkyView::StarChart => "Star chart",
        }
    }
}

/// Playback speeds, sky minutes per second.
const SPEEDS: [f32; 5] = [1.0, 5.0, 10.0, 30.0, 60.0];
/// Default azimuth span of the horizon view, degrees.
const DEFAULT_SPAN_DEG: f32 = 90.0;
/// Default magnification of the Moon's and Sun's discs.
const DEFAULT_DISC_SCALE: f32 = 6.0;
/// Default star-chart scale, pixels per degree.
const DEFAULT_CHART_PX_PER_DEG: f32 = 6.0;

/// Days around civil noon that a night needs from the ephemeris: its
/// interpolation table spans ±2 d (the conjunction's look-around is checked
/// by the circumstances, which report `OutsideEphemeris`).
const NIGHT_MARGIN_DAYS: f64 = 2.0;

/// Persisted view options of the Sky tab.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkySettings {
    pub view: SkyView,
    /// Horizon view: azimuth span (zoom), degrees.
    pub span_deg: f32,
    /// Magnification of the Sun and Moon discs (1 = true size).
    pub disc_scale: f32,
    /// Star chart scale, pixels per degree.
    pub chart_px_per_deg: f32,
    pub show_paths: bool,
    pub show_stars: bool,
    pub show_labels: bool,
    /// Playback speed, sky minutes per second.
    pub speed_min_per_s: f32,
}

impl Default for SkySettings {
    fn default() -> Self {
        Self {
            view: SkyView::Horizon,
            span_deg: DEFAULT_SPAN_DEG,
            disc_scale: DEFAULT_DISC_SCALE,
            chart_px_per_deg: DEFAULT_CHART_PX_PER_DEG,
            show_paths: true,
            show_stars: true,
            show_labels: true,
            speed_min_per_s: 10.0,
        }
    }
}

impl SkySettings {
    /// Clamp values from an edited or old settings file into the ranges
    /// the controls offer.
    fn sanitized(mut self) -> Self {
        let fix = |v: f32, lo: f32, hi: f32, default: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                default
            }
        };
        self.span_deg = fix(self.span_deg, 10.0, 360.0, DEFAULT_SPAN_DEG);
        self.disc_scale = fix(self.disc_scale, 1.0, 30.0, DEFAULT_DISC_SCALE);
        self.chart_px_per_deg = fix(self.chart_px_per_deg, 1.5, 40.0, DEFAULT_CHART_PX_PER_DEG);
        if !SPEEDS.contains(&self.speed_min_per_s) {
            self.speed_min_per_s = 10.0;
        }
        self
    }
}

/// State of the Sky tab. View options live in [`SkySettings`]; `opts` is
/// this frame's copy.
pub struct SkyTab {
    /// The night on screen (the last one computed).
    night: Option<Arc<Night>>,
    job: CachedJob<NightKey, Arc<Night>>,
    opts: SkySettings,
    /// The instant shown, UT JD.
    time: f64,
    playing: bool,
    last_tick: Option<f64>,
    /// Horizon view: pan from the automatic centre, degrees.
    az_offset_deg: f32,
    alt_offset_deg: f32,
    /// Automatic centre azimuth of the horizon view, radians.
    auto_center_az: f64,
    /// Star chart centre (`None` follows the Moon).
    chart_center: Option<(f64, f64)>,
}

impl Default for SkyTab {
    fn default() -> Self {
        Self {
            night: None,
            job: CachedJob::default(),
            opts: SkySettings::default(),
            time: 0.0,
            playing: false,
            last_tick: None,
            az_offset_deg: 0.0,
            alt_offset_deg: 0.0,
            auto_center_az: 270.0 * DEG,
            chart_center: None,
        }
    }
}

impl std::fmt::Debug for SkyTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkyTab")
            .field("view", &self.opts.view)
            .field("time", &self.time)
            .finish_non_exhaustive()
    }
}

/// The coverage problem of the night for `key`, if any.
fn coverage_problem(key: &NightKey, settings: &crate::settings::Settings) -> Option<String> {
    let noon = civil_noon(key.date, key.zone);
    crate::backend::resolve(settings.backend, &settings.de440_path).span_problem(
        noon,
        noon,
        NIGHT_MARGIN_DAYS,
        NIGHT_MARGIN_DAYS,
    )
}

/// Circular mean of two azimuths, radians.
fn mean_azimuth(a: f64, b: f64) -> f64 {
    norm_2pi((a.sin() + b.sin()).atan2(a.cos() + b.cos()))
}

impl SkyTab {
    /// Start computing the night for the current inputs while the tab is
    /// visible; adopt a finished night.
    pub fn poll(&mut self, ctx: &egui::Context, tc: &mut TabContext<'_>) {
        if tc.settings.view.active_tab == Tab::Sky {
            let key = NightKey::from_settings(tc.settings);
            if coverage_problem(&key, tc.settings).is_none() {
                let eph = ephemeris(tc.settings);
                self.job
                    .request(ctx, key, "Computing the sky", move |key, job| {
                        job.checkpoint()?;
                        Ok(Arc::new(Night::compute(key.clone(), eph.as_ref())))
                    });
            }
        }
        self.job.poll(tc.status);
        if let Some(night) = self.job.get()
            && !self.night.as_ref().is_some_and(|n| Arc::ptr_eq(n, night))
        {
            let night = night.clone();
            self.adopt(night);
        }
    }

    /// Show a newly computed night, from its default instant.
    fn adopt(&mut self, night: Arc<Night>) {
        self.time = night.default_time();
        let s = night.snapshot(self.time);
        self.auto_center_az = if s.moon.alt > -10.0 * DEG {
            mean_azimuth(s.sun.az, s.moon.az)
        } else {
            s.sun.az
        };
        self.az_offset_deg = 0.0;
        self.alt_offset_deg = 0.0;
        self.chart_center = None;
        self.playing = false;
        self.night = Some(night);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        self.opts = tc.settings.sky_tab.clone().sanitized();
        self.ui_inner(ui, tc);
        if tc.settings.sky_tab != self.opts {
            tc.settings.sky_tab = self.opts.clone();
        }
    }

    fn ui_inner(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let tz = tc.settings.time.tz_offset_hours;
        let key = NightKey::from_settings(tc.settings);
        self.controls(ui);
        if let Some(problem) = coverage_problem(&key, tc.settings) {
            coverage_notice(ui, &format!("{}: {problem}", key.date_label()));
            return;
        }
        let Some(night) = self.night.clone() else {
            super::ephemeris::busy(ui, true);
            return;
        };
        if night.key != key {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(format!(
                    "Computing the {} … (showing the {})",
                    key.date_label().to_lowercase(),
                    night.key.date_label().to_lowercase()
                ));
            });
        }
        summary_line(ui, &night, tz, night.key.crescent);

        // Playback.
        if self.playing {
            let now = ui.input(|i| i.time);
            if let Some(last) = self.last_tick {
                self.time += (now - last) * f64::from(self.opts.speed_min_per_s) / 1440.0;
                if self.time > night.end {
                    self.time = night.start;
                }
            }
            self.last_tick = Some(now);
            ui.ctx().request_repaint();
        } else {
            self.last_tick = None;
        }
        self.time = self.time.clamp(night.start, night.end);

        let avail = ui.available_size();
        let size = vec2(avail.x, (avail.y - 40.0).max(220.0));
        let (response, painter) = ui.allocate_painter(size, Sense::click_and_drag());
        let rect = response.rect;
        let snap = night.snapshot(self.time);
        match self.opts.view {
            SkyView::Horizon => self.horizon_view(ui, &painter, &response, &night, &snap, tz),
            SkyView::AllSky => self.all_sky_view(&painter, &response, &night, &snap),
            SkyView::StarChart => self.star_chart_view(ui, &painter, &response, &night, &snap),
        }
        if self.opts.view != SkyView::StarChart {
            self.readout_box(&painter, rect, &night, &snap, tz);
        }

        self.time_controls(ui, &night, tz);
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for v in SkyView::ALL {
                ui.selectable_value(&mut self.opts.view, v, v.label());
            }
            ui.separator();
            if self.opts.view == SkyView::Horizon {
                ui.label("Span");
                ui.add(
                    egui::Slider::new(&mut self.opts.span_deg, 20.0..=360.0)
                        .suffix("°")
                        .logarithmic(true),
                );
            }
            if self.opts.view == SkyView::StarChart {
                ui.label("Scale");
                ui.add(
                    egui::Slider::new(&mut self.opts.chart_px_per_deg, 1.5..=40.0)
                        .suffix(" px/°")
                        .logarithmic(true),
                );
            } else {
                ui.label("Discs ×");
                ui.add(egui::Slider::new(&mut self.opts.disc_scale, 1.0..=30.0).logarithmic(true))
                    .on_hover_text("Magnification of the Sun's and Moon's discs (1 = true size)");
            }
            ui.checkbox(&mut self.opts.show_paths, "Paths");
            ui.checkbox(&mut self.opts.show_stars, "Stars");
            ui.checkbox(&mut self.opts.show_labels, "Labels");
            if ui.button("Reset view").clicked() {
                self.reset_view();
            }
        });
    }

    fn reset_view(&mut self) {
        self.opts.span_deg = DEFAULT_SPAN_DEG;
        self.az_offset_deg = 0.0;
        self.alt_offset_deg = 0.0;
        self.chart_center = None;
        self.opts.chart_px_per_deg = DEFAULT_CHART_PX_PER_DEG;
    }

    fn time_controls(&mut self, ui: &mut egui::Ui, night: &Night, tz: f64) {
        ui.horizontal(|ui| {
            let icon = if self.playing { "⏸" } else { "▶" };
            if ui
                .button(icon)
                .on_hover_text(if self.playing { "Pause" } else { "Play" })
                .clicked()
            {
                self.playing = !self.playing;
            }
            egui::ComboBox::from_id_salt("sky_speed")
                .width(80.0)
                .selected_text(format!("{} min/s", self.opts.speed_min_per_s))
                .show_ui(ui, |ui| {
                    for s in SPEEDS {
                        ui.selectable_value(
                            &mut self.opts.speed_min_per_s,
                            s,
                            format!("{s} min/s"),
                        );
                    }
                });
            ui.monospace(format_hm(self.time, tz));
            let jumps: [(&str, Option<f64>); 3] = match night.key.crescent {
                Crescent::Evening => [
                    ("Sunset", night.sun_event),
                    ("Best", night.evaluation),
                    ("Moonset", night.moon_event),
                ],
                Crescent::Morning => [
                    ("Moonrise", night.moon_event),
                    ("Best", night.evaluation),
                    ("Sunrise", night.sun_event),
                ],
            };
            for (name, t) in jumps {
                if let Some(t) = t
                    && ui
                        .small_button(name)
                        .on_hover_text(format_hm(t, tz))
                        .clicked()
                {
                    self.time = t.clamp(night.start, night.end);
                    self.playing = false;
                }
            }
            let w = (ui.available_width() - 110.0).max(120.0);
            ui.spacing_mut().slider_width = w;
            let r = ui.add(
                egui::Slider::new(&mut self.time, night.start..=night.end)
                    .show_value(false)
                    .smart_aim(false),
            );
            if r.dragged() {
                self.playing = false;
            }
            ui.weak(format!(
                "{}–{}",
                format_hm(night.start, tz),
                format_hm(night.end, tz)
            ));
        });
    }

    // ----------------------------------------------------------------- views

    #[allow(clippy::too_many_arguments)]
    fn horizon_view(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        response: &egui::Response,
        night: &Night,
        snap: &Snapshot,
        tz: f64,
    ) {
        let rect = response.rect;
        // Interaction: wheel zooms the span, drag pans, double-click resets.
        if response.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = (-scroll / 300.0).exp() / pinch;
            if (factor - 1.0).abs() > 1e-4 {
                self.opts.span_deg = (self.opts.span_deg * factor).clamp(10.0, 360.0);
            }
        }
        let px_per_deg = rect.width() / self.opts.span_deg;
        if response.dragged() {
            let d = response.drag_delta();
            self.az_offset_deg -= d.x / px_per_deg;
            self.alt_offset_deg += d.y / px_per_deg;
            self.alt_offset_deg = self.alt_offset_deg.clamp(-80.0, 60.0);
        }
        if response.double_clicked() {
            self.reset_view();
        }
        let px_per_deg = rect.width() / self.opts.span_deg;
        let proj = HorizonProjection {
            center_az: self.auto_center_az + f64::from(self.az_offset_deg) * DEG,
            center_x: rect.center().x,
            horizon_y: rect.top() + rect.height() * 0.78 + self.alt_offset_deg * px_per_deg,
            px_per_rad: px_per_deg / DEG as f32,
        };
        let sun_alt_deg = snap.sun.alt / DEG;

        // Sky and ground.
        let sky_rect = Rect::from_min_max(
            rect.min,
            pos2(rect.right(), proj.horizon_y.min(rect.bottom())),
        );
        if sky_rect.height() > 0.0 {
            draw::gradient_rect(painter, sky_rect, 48, 24, |p| {
                let (alt, az) = proj.unproject(p);
                let sep = separation(az, alt, snap.sun.az, snap.sun.alt) / DEG;
                draw::rgb(sky_color(sun_alt_deg, alt / DEG, sep))
            });
        }
        let ground =
            Rect::from_min_max(pos2(rect.left(), proj.horizon_y.max(rect.top())), rect.max);
        let ground_color = draw::rgb(sky_color(sun_alt_deg, 0.0, 180.0));
        let ground_color = Color32::from_rgb(
            (ground_color.r() as f32 * 0.25 + 12.0) as u8,
            (ground_color.g() as f32 * 0.3 + 16.0) as u8,
            (ground_color.b() as f32 * 0.2 + 10.0) as u8,
        );

        // Altitude grid.
        let grid = Stroke::new(0.5, Color32::from_white_alpha(28));
        for alt in (-80..=80).step_by(10) {
            let y = proj.project(f64::from(alt) * DEG, proj.center_az).y;
            if rect.y_range().contains(y) && alt != 0 {
                painter.hline(rect.x_range(), y, grid);
                draw::label(
                    painter,
                    pos2(rect.right() - 4.0, y - 1.0),
                    Align2::RIGHT_BOTTOM,
                    format!("{alt}°"),
                    10.0,
                    Color32::from_white_alpha(120),
                );
            }
        }

        // Stars.
        if self.opts.show_stars {
            let alpha = star_visibility(sun_alt_deg);
            if alpha > 0.02 {
                for s in &night.stars {
                    let h = star_horizontal(s.ra, s.dec, night.observer.lat, snap.lst);
                    if h.alt < -0.5 * DEG {
                        continue;
                    }
                    let p = proj.project(h.alt, h.az);
                    if rect.contains(p) {
                        draw::star(painter, p, s.star.vmag, s.star.bv, alpha);
                        if self.opts.show_labels && s.star.vmag < 1.6 {
                            draw::label(
                                painter,
                                p + vec2(5.0, 0.0),
                                Align2::LEFT_CENTER,
                                s.star.display_name(),
                                10.0,
                                Color32::from_white_alpha((150.0 * alpha) as u8),
                            );
                        }
                    }
                }
            }
        }

        // Paths and 10-minute dots.
        if self.opts.show_paths {
            let max_jump = rect.width() * 0.5;
            let path = |state: fn(&Snapshot) -> (f64, f64)| -> Vec<(Pos2, bool)> {
                night
                    .samples
                    .iter()
                    .map(|s| {
                        let (alt, az) = state(s);
                        (proj.project(alt, az), true)
                    })
                    .collect()
            };
            draw::polyline(
                painter,
                &path(|s| (s.sun.alt, s.sun.az)),
                Stroke::new(1.2, SUN_PATH.gamma_multiply(0.8)),
                max_jump,
            );
            draw::polyline(
                painter,
                &path(|s| (s.moon.alt, s.moon.az)),
                Stroke::new(1.2, MOON_PATH.gamma_multiply(0.8)),
                max_jump,
            );
            self.horizon_dots(painter, &proj, rect, night, tz);
        }

        // Ground on top of the paths, so what is below the horizon is dimmed.
        if ground.height() > 0.0 {
            painter.rect_filled(ground, 0.0, ground_color.gamma_multiply(0.82));
            painter.hline(
                rect.x_range(),
                proj.horizon_y,
                Stroke::new(1.5, Color32::from_rgb(120, 140, 110)),
            );
        }
        horizon_ticks(painter, &proj, rect);

        // ARCV / DAZ annotation.
        let deg_px = f64::from(px_per_deg);
        let sun_p = proj.project(snap.sun.alt, snap.sun.az);
        let moon_p = proj.project(snap.moon.alt, snap.moon.az);
        let sun_r = ((sun_semidiameter(snap.sun.topo.distance_km) / DEG * deg_px) as f32
            * self.opts.disc_scale)
            .max(5.0);
        if let Some(g) = night.geometry(self.time)
            && rect.contains(sun_p.clamp(rect.min, rect.max))
        {
            let corner = pos2(moon_p.x, sun_p.y);
            let dash = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 150));
            painter.add(egui::Shape::dashed_line(
                &[sun_p, corner, moon_p],
                dash,
                5.0,
                4.0,
            ));
            draw::label(
                painter,
                pos2((sun_p.x + corner.x) / 2.0, sun_p.y + sun_r + 4.0),
                Align2::CENTER_TOP,
                format!("DAZ {:+.1}°", g.daz_deg),
                11.0,
                Color32::WHITE,
            );
            draw::label(
                painter,
                pos2(corner.x + 6.0, (corner.y + moon_p.y) / 2.0),
                Align2::LEFT_CENTER,
                format!("ARCV {:.1}°", g.arcv_deg),
                11.0,
                Color32::WHITE,
            );
        }

        // The Sun and the Moon.
        draw::sun(painter, sun_p, sun_r, snap.sun.alt < 0.0);
        let moon_r = (moon_semidiameter(snap.moon.topo.distance_km) / DEG * deg_px) as f32
            * self.opts.disc_scale;
        let limb = limb_direction(proj.zenith_direction(), snap.limb_zenith);
        draw::moon(
            painter,
            moon_p,
            moon_r.max(4.0),
            snap.illumination.fraction,
            limb,
        );
        if self.opts.show_labels {
            draw::label(
                painter,
                moon_p + vec2(moon_r.max(4.0) + 6.0, -moon_r.max(4.0) - 2.0),
                Align2::LEFT_BOTTOM,
                "Moon",
                11.0,
                Color32::from_rgb(230, 230, 210),
            );
        }

        hover_readout(ui, painter, response, rect, |p| {
            let (alt, az) = proj.unproject(p);
            format!(
                "alt {:+.1}°  az {:.1}° {}",
                alt / DEG,
                az / DEG,
                compass_point(az / DEG)
            )
        });
    }

    fn horizon_dots(
        &self,
        painter: &egui::Painter,
        proj: &HorizonProjection,
        rect: Rect,
        night: &Night,
        tz: f64,
    ) {
        const STEP: f64 = 10.0 / 1440.0;
        let tz_days = tz / 24.0;
        let (p0, p1) = (
            night.samples.first().map_or(night.start, |s| s.jd_ut),
            night.samples.last().map_or(night.end, |s| s.jd_ut),
        );
        // Dots at round local times.
        let first = ((p0 + tz_days) / STEP).ceil() * STEP - tz_days;
        let px_per_10min = 10.0 * proj.px_per_rad * (0.25 * DEG) as f32;
        let label_every = if px_per_10min > 60.0 {
            1
        } else if px_per_10min > 25.0 {
            3
        } else {
            6
        };
        let mut t = first;
        let mut k = (((first + tz_days) / STEP).round() as i64).rem_euclid(144);
        while t <= p1 {
            let s = night.snapshot(t);
            let labelled = self.opts.show_labels && k % label_every == 0;
            for (alt, az, color) in [
                (s.sun.alt, s.sun.az, SUN_PATH),
                (s.moon.alt, s.moon.az, MOON_PATH),
            ] {
                let p = proj.project(alt, az);
                if !rect.expand(4.0).contains(p) {
                    continue;
                }
                painter.circle_filled(p, 2.2, color);
                if labelled {
                    draw::label(
                        painter,
                        p + vec2(5.0, 4.0),
                        Align2::LEFT_TOP,
                        format_hm(t, tz),
                        9.5,
                        color.gamma_multiply(0.9),
                    );
                }
            }
            t += STEP;
            k = (k + 1).rem_euclid(144);
        }
    }

    fn all_sky_view(
        &mut self,
        painter: &egui::Painter,
        response: &egui::Response,
        night: &Night,
        snap: &Snapshot,
    ) {
        let rect = response.rect;
        let radius = (rect.width().min(rect.height()) * 0.5 - 26.0).max(40.0);
        let proj = AllSkyProjection {
            center: rect.center(),
            radius,
        };
        let sun_alt_deg = snap.sun.alt / DEG;
        painter.rect_filled(rect, 0.0, Color32::from_rgb(14, 16, 20));
        let unproject = |p: Pos2| -> (f64, f64) {
            let d = p - proj.center;
            let r = f64::from(d.length() / radius);
            let alt = (1.0 - r) * 90.0 * DEG;
            let az = norm_2pi(f64::from(-d.x).atan2(f64::from(-d.y)));
            (alt, az)
        };
        draw::gradient_disc(painter, proj.center, radius, 18, 72, |p| {
            let (alt, az) = unproject(p);
            let sep = separation(az, alt, snap.sun.az, snap.sun.alt) / DEG;
            draw::rgb(sky_color(sun_alt_deg, alt / DEG, sep))
        });
        let grid = Stroke::new(0.6, Color32::from_white_alpha(40));
        for alt in [30.0, 60.0] {
            painter.circle_stroke(proj.center, radius * (1.0 - alt / 90.0), grid);
        }
        for k in 0..8 {
            let az = f64::from(k) * 45.0 * DEG;
            painter.line_segment([proj.center, proj.project(0.0, az)], grid);
        }
        painter.circle_stroke(
            proj.center,
            radius,
            Stroke::new(1.5, Color32::from_rgb(120, 140, 110)),
        );
        for (name, az) in [("N", 0.0), ("E", 90.0), ("S", 180.0), ("W", 270.0)] {
            let dir = proj.project(0.0, az * DEG) - proj.center;
            draw::label(
                painter,
                proj.center + dir * ((radius + 13.0) / radius),
                Align2::CENTER_CENTER,
                name,
                14.0,
                Color32::from_rgb(220, 220, 200),
            );
        }

        if self.opts.show_stars {
            let alpha = star_visibility(sun_alt_deg);
            if alpha > 0.02 {
                for s in &night.stars {
                    let h = star_horizontal(s.ra, s.dec, night.observer.lat, snap.lst);
                    if h.alt < 0.0 {
                        continue;
                    }
                    let p = proj.project(h.alt, h.az);
                    draw::star(painter, p, s.star.vmag, s.star.bv, alpha);
                    if self.opts.show_labels && s.star.vmag < 1.3 {
                        draw::label(
                            painter,
                            p + vec2(5.0, 0.0),
                            Align2::LEFT_CENTER,
                            s.star.display_name(),
                            10.0,
                            Color32::from_white_alpha((150.0 * alpha) as u8),
                        );
                    }
                }
            }
        }

        // Ecliptic.
        let eps = NutationObliquity::at(Instant::from_jd_ut(snap.jd_ut).jd_tt()).eps;
        let ecl: Vec<(Pos2, bool)> = (0..=180)
            .map(|i| {
                let eq = ecliptic_to_equatorial(
                    &Ecliptic {
                        lon: f64::from(i) * 2.0 * DEG,
                        lat: 0.0,
                        distance_km: 1.0,
                    },
                    eps,
                );
                let h = star_horizontal(eq.ra, eq.dec, night.observer.lat, snap.lst);
                (proj.project(h.alt, h.az), h.alt >= 0.0)
            })
            .collect();
        draw::polyline(
            painter,
            &ecl,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(230, 200, 90, 120)),
            radius,
        );

        if self.opts.show_paths {
            let pts = |f: fn(&Snapshot) -> (f64, f64)| -> Vec<(Pos2, bool)> {
                night
                    .samples
                    .iter()
                    .map(|s| {
                        let (alt, az) = f(s);
                        (proj.project(alt, az), alt >= -DEG)
                    })
                    .collect()
            };
            draw::polyline(
                painter,
                &pts(|s| (s.sun.alt, s.sun.az)),
                Stroke::new(1.2, SUN_PATH),
                radius,
            );
            draw::polyline(
                painter,
                &pts(|s| (s.moon.alt, s.moon.az)),
                Stroke::new(1.2, MOON_PATH),
                radius,
            );
        }

        let px_per_deg = f64::from(radius) / 90.0;
        let sun_p = proj.project(snap.sun.alt, snap.sun.az);
        if rect.contains(sun_p) {
            let r = (sun_semidiameter(snap.sun.topo.distance_km) / DEG * px_per_deg) as f32
                * self.opts.disc_scale;
            draw::sun(painter, sun_p, r.max(5.0), snap.sun.alt < 0.0);
        }
        let moon_p = proj.project(snap.moon.alt, snap.moon.az);
        if rect.contains(moon_p) {
            let r = ((moon_semidiameter(snap.moon.topo.distance_km) / DEG * px_per_deg) as f32
                * self.opts.disc_scale)
                .max(5.0);
            let limb = limb_direction(proj.zenith_direction(moon_p), snap.limb_zenith);
            draw::moon(painter, moon_p, r, snap.illumination.fraction, limb);
        }
    }

    fn star_chart_view(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        response: &egui::Response,
        night: &Night,
        snap: &Snapshot,
    ) {
        let rect = response.rect;
        if response.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = (scroll / 300.0).exp() * pinch;
            if (factor - 1.0).abs() > 1e-4 {
                self.opts.chart_px_per_deg = (self.opts.chart_px_per_deg * factor).clamp(1.5, 40.0);
            }
        }
        let (mut ra0, mut dec0) = self.chart_center.unwrap_or((
            snap.moon.topo.ra,
            snap.moon.topo.dec.clamp(-60.0 * DEG, 60.0 * DEG),
        ));
        let px_per_rad = self.opts.chart_px_per_deg / DEG as f32;
        if response.dragged() {
            let d = response.drag_delta();
            ra0 = norm_2pi(ra0 + f64::from(d.x / px_per_rad));
            dec0 = (dec0 + f64::from(d.y / px_per_rad)).clamp(-90.0 * DEG, 90.0 * DEG);
            self.chart_center = Some((ra0, dec0));
        }
        if response.double_clicked() {
            self.chart_center = None;
        }
        let proj = StarChartProjection {
            center: rect.center(),
            ra0,
            dec0,
            px_per_rad,
        };
        painter.rect_filled(rect, 0.0, Color32::from_rgb(8, 12, 26));
        let painter = painter.with_clip_rect(rect);

        // Grid: RA hours and Dec every 10° (or 30° when zoomed out).
        let grid = Stroke::new(0.5, Color32::from_white_alpha(30));
        let dec_step = if self.opts.chart_px_per_deg < 3.0 {
            30
        } else {
            10
        };
        for h in 0..24 {
            let ra = f64::from(h) * 15.0 * DEG;
            let x = proj.project(ra, 0.0).x;
            painter.vline(x, rect.y_range(), grid);
            draw::label(
                &painter,
                pos2(x, rect.bottom() - 3.0),
                Align2::CENTER_BOTTOM,
                format!("{h}h"),
                10.0,
                Color32::from_white_alpha(140),
            );
        }
        for d in (-90..=90).step_by(dec_step) {
            let y = proj.project(ra0, f64::from(d) * DEG).y;
            let stroke = if d == 0 {
                Stroke::new(0.8, Color32::from_rgba_unmultiplied(120, 170, 255, 90))
            } else {
                grid
            };
            painter.hline(rect.x_range(), y, stroke);
            draw::label(
                &painter,
                pos2(rect.left() + 3.0, y - 1.0),
                Align2::LEFT_BOTTOM,
                format!("{d:+}°"),
                10.0,
                Color32::from_white_alpha(140),
            );
        }
        let max_jump = rect.width() * 0.5;
        let eps = NutationObliquity::at(Instant::from_jd_ut(snap.jd_ut).jd_tt()).eps;
        let ecl: Vec<(Pos2, bool)> = (0..=360)
            .map(|i| {
                let eq = ecliptic_to_equatorial(
                    &Ecliptic {
                        lon: f64::from(i) * DEG,
                        lat: 0.0,
                        distance_km: 1.0,
                    },
                    eps,
                );
                (proj.project(eq.ra, eq.dec), true)
            })
            .collect();
        draw::polyline(
            &painter,
            &ecl,
            Stroke::new(1.2, Color32::from_rgba_unmultiplied(230, 200, 90, 150)),
            max_jump,
        );
        // The horizon at the shown instant.
        let horizon: Vec<(Pos2, bool)> = (0..=180)
            .map(|i| {
                let (ra, dec) = horizontal_to_equatorial(
                    &Horizontal {
                        alt: 0.0,
                        az: f64::from(i) * 2.0 * DEG,
                    },
                    night.observer.lat,
                    snap.lst,
                );
                (proj.project(ra, dec), true)
            })
            .collect();
        draw::polyline(
            &painter,
            &horizon,
            Stroke::new(1.2, Color32::from_rgba_unmultiplied(110, 200, 120, 150)),
            max_jump,
        );

        if self.opts.show_stars {
            for s in &night.stars {
                let p = proj.project(s.ra, s.dec);
                if !rect.contains(p) {
                    continue;
                }
                draw::star(&painter, p, s.star.vmag, s.star.bv, 1.0);
                let limit = if self.opts.chart_px_per_deg > 12.0 {
                    3.5
                } else {
                    1.6
                };
                if self.opts.show_labels && s.star.vmag < limit {
                    draw::label(
                        &painter,
                        p + vec2(5.0, 0.0),
                        Align2::LEFT_CENTER,
                        s.star.display_name(),
                        10.0,
                        Color32::from_white_alpha(150),
                    );
                }
            }
        }
        if self.opts.show_paths {
            let pts = |f: fn(&Snapshot) -> (f64, f64)| -> Vec<(Pos2, bool)> {
                night
                    .samples
                    .iter()
                    .map(|s| {
                        let (ra, dec) = f(s);
                        (proj.project(ra, dec), true)
                    })
                    .collect()
            };
            draw::polyline(
                &painter,
                &pts(|s| (s.sun.topo.ra, s.sun.topo.dec)),
                Stroke::new(2.0, SUN_PATH),
                max_jump,
            );
            draw::polyline(
                &painter,
                &pts(|s| (s.moon.topo.ra, s.moon.topo.dec)),
                Stroke::new(2.0, MOON_PATH),
                max_jump,
            );
        }
        let deg_px = f64::from(self.opts.chart_px_per_deg);
        let sun_p = proj.project(snap.sun.topo.ra, snap.sun.topo.dec);
        let sun_r = (sun_semidiameter(snap.sun.topo.distance_km) / DEG * deg_px) as f32;
        draw::sun(
            &painter,
            sun_p,
            (sun_r * self.opts.disc_scale.min(3.0)).max(6.0),
            false,
        );
        let moon_p = proj.project(snap.moon.topo.ra, snap.moon.topo.dec);
        let moon_r =
            ((moon_semidiameter(snap.moon.topo.distance_km) / DEG * deg_px) as f32 * 3.0).max(7.0);
        let limb = limb_direction(proj.north_direction(), snap.illumination.bright_limb_angle);
        draw::moon(&painter, moon_p, moon_r, snap.illumination.fraction, limb);
        if self.opts.show_labels {
            draw::label(
                &painter,
                sun_p + vec2(9.0, 0.0),
                Align2::LEFT_CENTER,
                "Sun",
                11.0,
                draw::SUN,
            );
            draw::label(
                &painter,
                moon_p + vec2(moon_r + 5.0, 0.0),
                Align2::LEFT_CENTER,
                "Moon",
                11.0,
                draw::MOON_LIT,
            );
        }
        let ra_h = snap.moon.topo.ra / DEG / 15.0;
        draw::info_box(
            &painter,
            rect,
            Align2::LEFT_TOP,
            &[
                (
                    format!(
                        "Moon  RA {:02}h{:04.1}m  Dec {:+.2}°  (topocentric, of date)",
                        ra_h.floor() as i64,
                        ra_h.fract() * 60.0,
                        snap.moon.topo.dec / DEG
                    ),
                    draw::MOON_LIT,
                ),
                (
                    format!(
                        "Sun   RA {:02}h{:04.1}m  Dec {:+.2}°",
                        (snap.sun.topo.ra / DEG / 15.0).floor() as i64,
                        (snap.sun.topo.ra / DEG / 15.0).fract() * 60.0,
                        snap.sun.topo.dec / DEG
                    ),
                    draw::SUN,
                ),
                (
                    "Yellow: ecliptic · Green: horizon · Blue: equator".to_owned(),
                    Color32::from_gray(180),
                ),
            ],
        );
        hover_readout(ui, &painter, response, rect, |p| {
            let ra = norm_2pi(ra0 - f64::from((p.x - proj.center.x) / px_per_rad));
            let dec = dec0 + f64::from((proj.center.y - p.y) / px_per_rad);
            let h = ra / DEG / 15.0;
            format!(
                "RA {:02}h{:02}m  Dec {:+.1}°",
                h.floor() as i64,
                (h.fract() * 60.0).floor() as i64,
                dec / DEG
            )
        });
    }

    fn readout_box(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        night: &Night,
        snap: &Snapshot,
        tz: f64,
    ) {
        let mut lines = vec![(
            format!("{} (UTC{:+})", format_date_hm(self.time, tz), tz),
            Color32::WHITE,
        )];
        lines.push((
            format!(
                "Sun   alt {:+6.2}°  az {:6.2}°",
                snap.sun.alt / DEG,
                snap.sun.az / DEG
            ),
            draw::SUN,
        ));
        lines.push((
            format!(
                "Moon  alt {:+6.2}°  az {:6.2}°",
                snap.moon.alt / DEG,
                snap.moon.az / DEG
            ),
            draw::MOON_LIT,
        ));
        if let Some(g) = night.geometry(self.time) {
            lines.push((
                format!(
                    "ARCL {:5.2}°  ARCV {:5.2}°  DAZ {:+6.2}°",
                    g.arcl_deg, g.arcv_deg, g.daz_deg
                ),
                Color32::from_gray(225),
            ));
            lines.push((
                format!(
                    "W′ {:4.2}′  illum. {:4.2}%  {}",
                    g.width_topo_arcmin,
                    snap.illumination.fraction * 100.0,
                    age_text(g.age_hours, night.key.crescent)
                ),
                Color32::from_gray(225),
            ));
        } else {
            lines.push((
                format!(
                    "{}  illum. {:.1}%",
                    phase_name(snap.elongation_lon_deg),
                    snap.illumination.fraction * 100.0
                ),
                Color32::from_gray(225),
            ));
        }
        draw::info_box(painter, rect, Align2::LEFT_TOP, &lines);
    }
}

/// Apparent horizontal coordinates of a direction of date (stars, the
/// ecliptic): no parallax, Sæmundsson refraction.
fn star_horizontal(ra: f64, dec: f64, lat: f64, lst: f64) -> Horizontal {
    let h = equatorial_to_horizontal(
        &Equatorial {
            ra,
            dec,
            distance_km: 1.0,
        },
        lat,
        lst,
    );
    Horizontal {
        alt: apparent_altitude(h.alt, &ATMOSPHERE),
        az: h.az,
    }
}

/// Azimuth ticks and cardinal points along the horizon.
fn horizon_ticks(painter: &egui::Painter, proj: &HorizonProjection, rect: Rect) {
    let px_per_deg = proj.px_per_rad * DEG as f32;
    let (minor, major) = if px_per_deg > 12.0 {
        (1, 5)
    } else if px_per_deg > 3.0 {
        (5, 15)
    } else {
        (15, 45)
    };
    for az in (0..360).step_by(minor) {
        let p = proj.project(0.0, f64::from(az) * DEG);
        if !rect.x_range().contains(p.x) {
            continue;
        }
        let is_major = az % major == 0;
        let len = if is_major { 7.0 } else { 3.5 };
        painter.vline(
            p.x,
            p.y..=p.y + len,
            Stroke::new(1.0, Color32::from_rgb(150, 170, 140)),
        );
        if az % 45 == 0 {
            draw::label(
                painter,
                p + vec2(0.0, 10.0),
                Align2::CENTER_TOP,
                compass_point(f64::from(az)),
                14.0,
                Color32::from_rgb(235, 230, 200),
            );
        } else if is_major {
            draw::label(
                painter,
                p + vec2(0.0, 10.0),
                Align2::CENTER_TOP,
                format!("{az}°"),
                10.0,
                Color32::from_rgb(170, 180, 160),
            );
        }
    }
}

/// The position under the pointer, at the bottom left of the view.
fn hover_readout(
    ui: &egui::Ui,
    painter: &egui::Painter,
    response: &egui::Response,
    rect: Rect,
    text: impl Fn(Pos2) -> String,
) {
    if let Some(p) = response.hover_pos() {
        draw::info_box(
            painter,
            rect,
            Align2::LEFT_BOTTOM,
            &[(text(p), Color32::from_gray(220))],
        );
        ui.ctx().set_cursor_icon(if response.dragged() {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Crosshair
        });
    }
}

/// The Moon's age (evening) or the time left until conjunction (morning;
/// `CrescentGeometry::age_hours` is positive before conjunction then).
fn age_text(age_hours: f64, crescent: Crescent) -> String {
    match crescent {
        Crescent::Evening => format!("age {age_hours:+.1} h"),
        Crescent::Morning => format!("{age_hours:+.1} h to conjunction"),
    }
}

/// One line describing the night and the criterion's verdict.
fn summary_line(ui: &mut egui::Ui, night: &Night, tz: f64, crescent: Crescent) {
    let hm = |t: Instant| format_hm(t.jd_ut, tz);
    let (sun_word, moon_word) = match crescent {
        Crescent::Evening => ("Sunset", "Moonset"),
        Crescent::Morning => ("Sunrise", "Moonrise"),
    };
    let text = match &night.outcome {
        Outcome::Crescent(c) | Outcome::BeforeConjunction(c) | Outcome::MoonSetsFirst(c) => {
            let mut s = format!("{sun_word} {}", hm(c.sun_event));
            if let Some(m) = c.moon_event {
                s += &format!(" · {moon_word} {}", hm(m));
            }
            s += &format!(
                " · Lag {:.0} min · Best time {} · {}",
                c.geometry.lag_minutes,
                hm(c.evaluation),
                age_text(c.geometry.age_hours, crescent)
            );
            match &night.outcome {
                Outcome::BeforeConjunction(_) => s += " · before conjunction",
                Outcome::MoonSetsFirst(_) => {
                    s += &format!(
                        " · the Moon {} first",
                        if crescent == Crescent::Evening {
                            "sets"
                        } else {
                            "rises after the Sun"
                        }
                    )
                }
                _ => {}
            }
            s
        }
        Outcome::MoonAlwaysUp { sun_event, .. } => {
            format!(
                "{sun_word} {} · the Moon does not set within 12 h",
                hm(*sun_event)
            )
        }
        Outcome::MoonAlwaysDown { sun_event } => {
            format!(
                "{sun_word} {} · the Moon stays below the horizon",
                hm(*sun_event)
            )
        }
        Outcome::NoSunEvent(Polar::MidnightSun) => "Midnight Sun: no sunset".to_owned(),
        Outcome::NoSunEvent(Polar::PolarNight) => "Polar night: no sunrise".to_owned(),
        Outcome::SunAltitudeNotReached { sun_event } => format!(
            "{sun_word} {} · the Sun does not reach the criterion's altitude",
            hm(*sun_event)
        ),
        Outcome::OutsideEphemeris => {
            "Not computed: the date is outside the ephemeris coverage".to_owned()
        }
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{} — {text}", night.key.date_label()));
        if let (Some(c), Some(v)) = (&night.criterion, &night.verdict) {
            if let Some(zone) = c.zones().get(v.zone) {
                let [r, g, b] = zone.rgb;
                ui.separator();
                let (dot, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
                ui.painter()
                    .circle_filled(dot.center(), 5.0, Color32::from_rgb(r, g, b));
                let value = v.value.map(|x| format!(" ({x:+.3})")).unwrap_or_default();
                ui.label(format!(
                    "{}: {} {}{value}",
                    c.name(),
                    zone.code,
                    zone.description
                ));
            }
        } else if let Some(c) = &night.criterion {
            ui.separator();
            ui.weak(format!("{}: no crescent to evaluate", c.name()));
        }
        ui.separator();
        ui.weak(&night.ephemeris_name);
    });
}

impl NightKey {
    /// "Evening of 2024-03-11" / "Morning of …".
    fn date_label(&self) -> String {
        format!(
            "{} of {:04}-{:02}-{:02}",
            match self.crescent {
                Crescent::Evening => "Evening",
                Crescent::Morning => "Morning",
            },
            self.date.year,
            self.date.month,
            self.date.day
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::status::StatusInfo;
    use std::time::{Duration, Instant as Clock};

    /// One headless egui frame of the Sky tab (visible).
    fn frame(ctx: &egui::Context, tab: &mut SkyTab, settings: &mut Settings) {
        let mut status = StatusInfo::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let mut tc = TabContext {
                settings,
                status: &mut status,
            };
            tab.poll(ui.ctx(), &mut tc);
            tab.ui(ui, &mut tc);
        });
        out.textures_delta.clear();
    }

    fn mecca_2024_03_11() -> Settings {
        let mut s = Settings::default();
        s.view.active_tab = Tab::Sky;
        s.time.tz_offset_hours = 3.0;
        s.time.jd_ut = crate::datetime::calendar_to_jd(2024, 3, 11.0) + 0.5;
        s
    }

    #[test]
    fn the_night_is_computed_in_the_background_and_adopted() {
        let ctx = egui::Context::default();
        let mut settings = mecca_2024_03_11();
        settings.sky_tab.view = SkyView::AllSky;
        let mut tab = SkyTab::default();
        let start = Clock::now();
        while tab.night.is_none() {
            assert!(start.elapsed() < Duration::from_secs(60), "never computed");
            frame(&ctx, &mut tab, &mut settings);
            std::thread::sleep(Duration::from_millis(5));
        }
        let night = tab.night.clone().unwrap();
        assert_eq!(night.key, NightKey::from_settings(&settings));
        assert!(matches!(night.outcome, Outcome::Crescent(_)));
        assert_eq!(tab.time, night.default_time());
        // The persisted view option reached the tab.
        assert_eq!(tab.opts.view, SkyView::AllSky);

        // A new date: the old night stays until the new one arrives.
        settings.time.jd_ut += 1.0;
        frame(&ctx, &mut tab, &mut settings);
        assert!(tab.night.is_some());
        while tab.night.as_ref().unwrap().key != NightKey::from_settings(&settings) {
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "never recomputed"
            );
            frame(&ctx, &mut tab, &mut settings);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_date_outside_the_coverage_starts_no_job() {
        // Meeus covers −2000 to 3000.
        let ctx = egui::Context::default();
        let mut settings = mecca_2024_03_11();
        settings.time.jd_ut = crate::datetime::calendar_to_jd(3500, 1, 1.0);
        let key = NightKey::from_settings(&settings);
        let problem = coverage_problem(&key, &settings).unwrap();
        assert!(problem.contains("outside the Meeus coverage"), "{problem}");
        let mut tab = SkyTab::default();
        for _ in 0..3 {
            frame(&ctx, &mut tab, &mut settings);
        }
        assert!(!tab.job.is_running() && tab.night.is_none());
    }

    #[test]
    fn options_are_sanitized_and_persist_without_their_fields() {
        let s = SkySettings {
            span_deg: f32::NAN,
            disc_scale: 1000.0,
            chart_px_per_deg: 0.0,
            speed_min_per_s: 7.0,
            ..SkySettings::default()
        }
        .sanitized();
        assert_eq!(s.span_deg, DEFAULT_SPAN_DEG);
        assert_eq!(s.disc_scale, 30.0);
        assert_eq!(s.chart_px_per_deg, 1.5);
        assert_eq!(s.speed_min_per_s, 10.0);
        let old: SkySettings = serde_json::from_str(r#"{"view":"StarChart"}"#).unwrap();
        assert_eq!(old.view, SkyView::StarChart);
        assert_eq!(old.span_deg, DEFAULT_SPAN_DEG);
        assert!(old.show_stars);
    }
}
