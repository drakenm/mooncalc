//! **Trajectory** tab: the Moon's (and the Sun's) course over one local
//! day, in the style of mooncalc.org.
//!
//! * A 24-hour altitude plot in local time with twilight bands, rise,
//!   culmination and set marked, and the shown instant.
//! * A polar "compass" (seen from above: north up, east right) with the
//!   paths' azimuths, rise and set directions, the current direction and
//!   the moonlight shadow.
//! * Readouts: rise/transit/set times and azimuths, current altitude and
//!   azimuth, distance, illuminated fraction, phase and age, next new and
//!   full Moon, twilight, and the shadow of a 1 m object.
//!
//! The shown instant follows the app time; the slider moves it within the
//! local day without touching the app time ("Apply" copies it there), and
//! **Live** keeps the app time at the current time (persisted in
//! [`TrajectorySettings`]).
//!
//! The day is computed on a background job ([`crate::jobs`],
//! cancel-and-replace); a day outside the ephemeris coverage shows a notice
//! instead.

use std::sync::Arc;

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, pos2, vec2};
use mooncalc_core::Instant;
use mooncalc_core::angle::DEG;
use mooncalc_core::events::phases::Phase;
use mooncalc_core::events::{Circumpolar, DayEvents};
use serde::{Deserialize, Serialize};

use super::TabContext;
use super::ephemeris::busy;
use super::ephemeris::cached::CachedJob;
use crate::datetime;
use crate::sky::draw::{self, MOON_PATH, SUN_PATH};
use crate::sky::geometry::{CompassProjection, compass_point, phase_name, shadow_length};
use crate::sky::scene::{DAY_SAMPLE_STEP, Day, DayKey, Snapshot};
use crate::sky::{format_date_hm, format_hm};
use crate::widgets::coverage::coverage_notice;

/// How often live mode refreshes, seconds.
const LIVE_REFRESH_S: f32 = 1.0;
/// Days either side of the local day that its interpolation table spans.
const DAY_MARGIN_DAYS: f64 = 1.0;
/// The phases a day lists span from this many days before it ...
const PHASES_BEFORE_DAYS: f64 = 32.0;
/// ... to this many after.
const PHASES_AFTER_DAYS: f64 = 62.0;

/// Persisted options of the Trajectory tab.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrajectorySettings {
    /// Keep the app time at the current time while the tab is shown.
    pub live: bool,
}

/// State of the Trajectory tab.
#[derive(Default)]
pub struct TrajectoryTab {
    /// The day on screen (the last one computed).
    day: Option<Arc<Day>>,
    job: CachedJob<DayKey, Arc<Day>>,
    /// The instant shown, UT JD.
    time: f64,
    /// The app time last seen, to follow its changes.
    last_app_jd: Option<f64>,
}

impl std::fmt::Debug for TrajectoryTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrajectoryTab")
            .field("time", &self.time)
            .finish_non_exhaustive()
    }
}

/// Colour of the sky band for a Sun altitude (degrees).
fn band_color(sun_alt_deg: f64) -> Color32 {
    if sun_alt_deg >= -0.833 {
        Color32::from_rgb(58, 104, 160)
    } else if sun_alt_deg >= -6.0 {
        Color32::from_rgb(42, 70, 118)
    } else if sun_alt_deg >= -12.0 {
        Color32::from_rgb(28, 42, 82)
    } else if sun_alt_deg >= -18.0 {
        Color32::from_rgb(18, 25, 54)
    } else {
        Color32::from_rgb(10, 12, 28)
    }
}

impl TrajectoryTab {
    /// Adopt a finished day and report a running job.
    pub fn poll(&mut self, _ctx: &egui::Context, tc: &mut TabContext<'_>) {
        self.job.poll(tc.status);
        if let Some(day) = self.job.get()
            && !self.day.as_ref().is_some_and(|d| Arc::ptr_eq(d, day))
        {
            self.day = Some(day.clone());
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        // Follow the app time; in live mode keep it at "now".
        if tc.settings.trajectory_tab.live {
            let now = datetime::now_jd();
            let minute = datetime::round_to_minute(now);
            if (tc.settings.time.jd_ut - minute).abs() > 1e-9 {
                tc.settings.time.jd_ut = minute;
            }
            self.time = now;
            self.last_app_jd = Some(tc.settings.time.jd_ut);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f32(LIVE_REFRESH_S));
        } else if self.last_app_jd != Some(tc.settings.time.jd_ut) {
            self.time = tc.settings.time.jd_ut;
            self.last_app_jd = Some(tc.settings.time.jd_ut);
        }
        let key = DayKey::new(tc.settings, self.time);
        let tz = tc.settings.time.tz_offset_hours;
        let start = key.zone.day_bounds(key.date).0.jd_ut;
        self.controls(ui, tc, start, tz);

        let active = crate::backend::resolve(tc.settings.backend, &tc.settings.de440_path);
        if let Some(problem) =
            active.span_problem(start, start + 1.0, DAY_MARGIN_DAYS, DAY_MARGIN_DAYS)
        {
            coverage_notice(ui, &problem);
            return;
        }
        let eph = active.eph.clone();
        self.job.request(
            ui.ctx(),
            key.clone(),
            "Computing the Moon's day",
            move |key, job| {
                job.checkpoint()?;
                Ok(Arc::new(Day::compute(key.clone(), eph.as_ref())))
            },
        );
        // The previous day may stand in while the new one is computed, as
        // long as its interpolation table spans the shown instant.
        let Some(day) = self.day.clone().filter(|d| {
            (d.start - DAY_MARGIN_DAYS..=d.start + 1.0 + DAY_MARGIN_DAYS).contains(&self.time)
        }) else {
            busy(ui, true);
            return;
        };
        if day.key != key {
            busy(ui, true);
        }
        let phases_complete = active
            .span_problem(start, start + 1.0, PHASES_BEFORE_DAYS, PHASES_AFTER_DAYS)
            .is_none();
        let snap = day.snapshot(self.time);

        let avail = ui.available_size();
        let side = (avail.y * 0.58).clamp(220.0, 520.0).min(avail.x * 0.42);
        ui.horizontal(|ui| {
            let plot_w = (avail.x - side - 2.0 * ui.spacing().item_spacing.x).max(300.0);
            let live = &mut tc.settings.trajectory_tab.live;
            self.altitude_plot(ui, &day, &snap, tz, vec2(plot_w, side), live);
            compass(ui, &day, &snap, tz, side);
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical().show(ui, |ui| readouts(ui, &day, &snap, tz, phases_complete));
    }

    fn controls(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>, day_start: f64, tz: f64) {
        ui.horizontal_wrapped(|ui| {
            ui.toggle_value(&mut tc.settings.trajectory_tab.live, "● Live")
                .on_hover_text("Keep the app time at the current time");
            if ui.button("« Day").clicked() {
                tc.settings.trajectory_tab.live = false;
                self.time -= 1.0;
            }
            if ui.button("Day »").clicked() {
                tc.settings.trajectory_tab.live = false;
                self.time += 1.0;
            }
            ui.label(format!(
                "{} — {}, {}",
                format_date_hm(self.time, tz),
                tc.settings.location.name,
                crate::datetime::format_utc_offset(tz)
            ));
            if ui
                .button("Apply")
                .on_hover_text("Set the app time to the shown instant")
                .clicked()
            {
                tc.settings.time.jd_ut = datetime::round_to_minute(self.time);
                self.last_app_jd = Some(tc.settings.time.jd_ut);
            }
        });
        ui.horizontal(|ui| {
            ui.weak("00:00");
            let mut t = self.time;
            ui.spacing_mut().slider_width = (ui.available_width() - 50.0).max(150.0);
            let r = ui.add(
                egui::Slider::new(&mut t, day_start..=day_start + 1.0 - 1.0 / 1440.0)
                    .show_value(false)
                    .smart_aim(false),
            );
            if r.changed() {
                tc.settings.trajectory_tab.live = false;
                self.time = t;
            }
            ui.weak("24:00");
        });
    }

    fn altitude_plot(
        &mut self,
        ui: &mut egui::Ui,
        day: &Day,
        snap: &Snapshot,
        tz: f64,
        size: egui::Vec2,
        live: &mut bool,
    ) {
        let (response, painter) = ui.allocate_painter(size, Sense::click_and_drag());
        let outer = response.rect;
        let rect = Rect::from_min_max(outer.min + vec2(34.0, 6.0), outer.max - vec2(8.0, 20.0));
        let x_of = |jd: f64| rect.left() + ((jd - day.start) as f32) * rect.width();
        let y_of = |alt: f64| rect.top() + ((90.0 - alt / DEG) / 180.0) as f32 * rect.height();
        let jd_of = |x: f32| day.start + f64::from((x - rect.left()) / rect.width());

        // Twilight bands.
        for w in day.samples.windows(2) {
            let mid = 0.5 * (w[0].sun.alt + w[1].sun.alt) / DEG;
            let r =
                Rect::from_x_y_ranges(x_of(w[0].jd_ut)..=x_of(w[1].jd_ut) + 0.5, rect.y_range());
            painter.rect_filled(r, 0.0, band_color(mid));
        }
        // Grid.
        let grid = Stroke::new(0.5, Color32::from_white_alpha(35));
        for alt in (-90..=90).step_by(30) {
            let y = y_of(f64::from(alt) * DEG);
            let stroke = if alt == 0 {
                Stroke::new(1.4, Color32::from_rgb(130, 160, 120))
            } else {
                grid
            };
            painter.hline(rect.x_range(), y, stroke);
            painter.text(
                pos2(rect.left() - 4.0, y),
                Align2::RIGHT_CENTER,
                format!("{alt}°"),
                egui::FontId::proportional(10.0),
                ui.visuals().weak_text_color(),
            );
        }
        for h in (0..=24).step_by(3) {
            let x = x_of(day.start + f64::from(h) / 24.0);
            painter.vline(x, rect.y_range(), grid);
            painter.text(
                pos2(x, rect.bottom() + 3.0),
                Align2::CENTER_TOP,
                format!("{h:02}"),
                egui::FontId::proportional(10.0),
                ui.visuals().weak_text_color(),
            );
        }
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, Color32::from_white_alpha(50)),
            egui::StrokeKind::Inside,
        );

        // Curves.
        let curve = |f: fn(&Snapshot) -> f64| -> Vec<(Pos2, bool)> {
            day.samples
                .iter()
                .map(|s| (pos2(x_of(s.jd_ut), y_of(f(s))), true))
                .collect()
        };
        draw::polyline(
            &painter,
            &curve(|s| s.sun.alt),
            Stroke::new(2.0, SUN_PATH),
            f32::INFINITY,
        );
        draw::polyline(
            &painter,
            &curve(|s| s.moon.alt),
            Stroke::new(2.2, MOON_PATH),
            f32::INFINITY,
        );

        // Events.
        let mark = |t: Option<Instant>, body_moon: bool, kind: &str| {
            let Some(t) = t else { return };
            let s = day.snapshot(t.jd_ut);
            let state = if body_moon { s.moon } else { s.sun };
            let color = if body_moon { MOON_PATH } else { SUN_PATH };
            let p = pos2(x_of(t.jd_ut), y_of(state.alt));
            painter.circle_filled(p, 4.0, color);
            painter.circle_stroke(p, 4.0, Stroke::new(1.0, Color32::BLACK));
            // Moon labels to the right, Sun labels to the left, so that
            // nearby events do not overlap; transits above (Moon) / below (Sun).
            let (anchor, off) = match (kind, body_moon) {
                ("transit", true) => (Align2::CENTER_BOTTOM, vec2(0.0, -7.0)),
                ("transit", false) => (Align2::CENTER_TOP, vec2(0.0, 7.0)),
                (_, true) => (Align2::LEFT_TOP, vec2(3.0, 6.0)),
                (_, false) => (Align2::RIGHT_TOP, vec2(-3.0, 6.0)),
            };
            draw::label(
                &painter,
                p + off,
                anchor,
                format_hm(t.jd_ut, tz),
                10.5,
                color,
            );
        };
        mark(day.moon.rise, true, "rise");
        mark(day.moon.set, true, "set");
        mark(day.moon.upper_transit, true, "transit");
        mark(day.sun.rise, false, "rise");
        mark(day.sun.set, false, "set");
        mark(day.sun.upper_transit, false, "transit");

        // The shown instant.
        let x = x_of(self.time);
        painter.vline(
            x,
            rect.y_range(),
            Stroke::new(1.2, Color32::from_rgb(255, 90, 90)),
        );
        painter.circle_filled(pos2(x, y_of(snap.sun.alt)), 5.0, draw::SUN);
        let mp = pos2(x, y_of(snap.moon.alt));
        let limb = if snap.elongation_lon_deg < 180.0 {
            vec2(1.0, 0.0)
        } else {
            vec2(-1.0, 0.0)
        };
        draw::moon(&painter, mp, 7.0, snap.illumination.fraction, limb);

        // Interaction: click or drag to move the instant; hover readout.
        if let Some(p) = response.interact_pointer_pos()
            && (response.clicked() || response.dragged())
            && rect.x_range().contains(p.x)
        {
            *live = false;
            self.time = jd_of(p.x);
        }
        if let Some(p) = response.hover_pos()
            && rect.contains(p)
        {
            let t = jd_of(p.x);
            let s = day.snapshot(t);
            painter.vline(
                p.x,
                rect.y_range(),
                Stroke::new(0.8, Color32::from_white_alpha(90)),
            );
            draw::info_box(
                &painter,
                rect,
                Align2::RIGHT_TOP,
                &[
                    (format_hm(t, tz), Color32::WHITE),
                    (
                        format!("Moon {:+.1}°  {:.0}°", s.moon.alt / DEG, s.moon.az / DEG),
                        draw::MOON_LIT,
                    ),
                    (
                        format!("Sun  {:+.1}°  {:.0}°", s.sun.alt / DEG, s.sun.az / DEG),
                        draw::SUN,
                    ),
                ],
            );
        }
    }
}

/// The polar diagram of azimuths (seen from above).
fn compass(ui: &mut egui::Ui, day: &Day, snap: &Snapshot, tz: f64, side: f32) {
    let (response, painter) = ui.allocate_painter(vec2(side, side), Sense::hover());
    let rect = response.rect;
    let proj = CompassProjection {
        center: rect.center(),
        radius: side * 0.5 - 30.0,
    };
    let r = proj.radius;
    painter.circle_filled(proj.center, r, Color32::from_rgb(20, 28, 44));
    let grid = Stroke::new(0.6, Color32::from_white_alpha(40));
    for alt in [30.0_f32, 60.0] {
        painter.circle_stroke(proj.center, r * (1.0 - alt / 90.0), grid);
    }
    for k in 0..16 {
        let az = f64::from(k) * 22.5 * DEG;
        let inner = if k % 2 == 0 { 0.0 } else { 0.9 };
        painter.line_segment([proj.rim(az, inner), proj.rim(az, 1.0)], grid);
    }
    painter.circle_stroke(
        proj.center,
        r,
        Stroke::new(1.4, Color32::from_rgb(130, 160, 120)),
    );
    for (name, az) in [("N", 0.0), ("E", 90.0), ("S", 180.0), ("W", 270.0)] {
        draw::label(
            &painter,
            proj.rim(az * DEG, 1.0 + 16.0 / r),
            Align2::CENTER_CENTER,
            name,
            14.0,
            Color32::from_rgb(225, 225, 205),
        );
    }

    // Rise/set directions.
    let ray = |t: Option<Instant>, moon: bool, dashed: bool| {
        let Some(t) = t else { return };
        let s = day.snapshot(t.jd_ut);
        let az = if moon { s.moon.az } else { s.sun.az };
        let color = if moon { MOON_PATH } else { SUN_PATH };
        let a = proj.center;
        let b = proj.rim(az, 1.0);
        if dashed {
            painter.add(egui::Shape::dashed_line(
                &[a, b],
                Stroke::new(1.2, color),
                5.0,
                4.0,
            ));
        } else {
            painter.line_segment([a, b], Stroke::new(1.6, color));
        }
        // Moon times above the ray, Sun times below, clear of the
        // cardinal letters.
        let nudge = vec2(0.0, if moon { -8.0 } else { 8.0 });
        draw::label(
            &painter,
            proj.rim(az, 1.0 + 6.0 / r) + nudge,
            if (az / DEG) < 180.0 {
                Align2::LEFT_CENTER
            } else {
                Align2::RIGHT_CENTER
            },
            format_hm(t.jd_ut, tz),
            10.0,
            color,
        );
    };
    ray(day.sun.rise, false, true);
    ray(day.sun.set, false, true);
    ray(day.moon.rise, true, false);
    ray(day.moon.set, true, false);

    // Paths above the horizon.
    let path = |f: fn(&Snapshot) -> (f64, f64)| -> Vec<(Pos2, bool)> {
        day.samples
            .iter()
            .map(|s| {
                let (alt, az) = f(s);
                (proj.project(alt, az), alt >= 0.0)
            })
            .collect()
    };
    draw::polyline(
        &painter,
        &path(|s| (s.sun.alt, s.sun.az)),
        Stroke::new(1.5, SUN_PATH),
        r,
    );
    draw::polyline(
        &painter,
        &path(|s| (s.moon.alt, s.moon.az)),
        Stroke::new(2.0, MOON_PATH),
        r,
    );

    // Current position, its direction and the moonlight shadow.
    if snap.moon.alt > 0.0 {
        let p = proj.project(snap.moon.alt, snap.moon.az);
        painter.line_segment(
            [proj.center, proj.rim(snap.moon.az, 1.0)],
            Stroke::new(1.0, Color32::from_rgb(255, 90, 90)),
        );
        if let Some(len) = shadow_length(1.0, snap.moon.alt) {
            let frac = (len / (1.0 + len)) as f32; // 0…1
            let end = proj.rim(snap.moon.az + std::f64::consts::PI, frac * 0.8);
            painter.line_segment(
                [proj.center, end],
                Stroke::new(3.0, Color32::from_black_alpha(200)),
            );
        }
        let limb = if snap.elongation_lon_deg < 180.0 {
            vec2(1.0, 0.0)
        } else {
            vec2(-1.0, 0.0)
        };
        draw::moon(&painter, p, 8.0, snap.illumination.fraction, limb);
    }
    if snap.sun.alt > 0.0 {
        let p = proj.project(snap.sun.alt, snap.sun.az);
        draw::sun(&painter, p, 6.0, false);
    }
    painter.text(
        rect.left_top() + vec2(4.0, 2.0),
        Align2::LEFT_TOP,
        "Seen from above",
        egui::FontId::proportional(10.0),
        ui.visuals().weak_text_color(),
    );
}

/// A rise/set/transit line: "07:12  az 84.5° E".
fn event_text(day: &Day, t: Option<Instant>, moon: bool, tz: f64, transit: bool) -> String {
    let Some(t) = t else {
        return "—".to_owned();
    };
    let s = day.snapshot(t.jd_ut);
    let b = if moon { s.moon } else { s.sun };
    if transit {
        format!("{}   alt {:+.1}°", format_hm(t.jd_ut, tz), b.alt / DEG)
    } else {
        format!(
            "{}   az {:.1}° {}",
            format_hm(t.jd_ut, tz),
            b.az / DEG,
            compass_point(b.az / DEG)
        )
    }
}

fn circumpolar_note(ev: &DayEvents, body: &str) -> Option<String> {
    ev.circumpolar.map(|c| match c {
        Circumpolar::AlwaysUp => format!("The {body} stays above the horizon all day"),
        Circumpolar::AlwaysDown => format!("The {body} stays below the horizon all day"),
    })
}

fn readouts(ui: &mut egui::Ui, day: &Day, snap: &Snapshot, tz: f64, phases_complete: bool) {
    let shadow = |alt: f64| match shadow_length(1.0, alt) {
        Some(l) if l < 1000.0 => format!("{l:.2} m"),
        Some(_) => "> 1 km".to_owned(),
        None => "— (below the horizon)".to_owned(),
    };
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        ui.strong("Moon");
        egui::Grid::new("traj_moon").num_columns(2).striped(true).show(ui, |ui| {
            let row = |ui: &mut egui::Ui, k: &str, v: String| {
                ui.label(k);
                ui.monospace(v);
                ui.end_row();
            };
            row(ui, "Moonrise", event_text(day, day.moon.rise, true, tz, false));
            row(ui, "Culmination", event_text(day, day.moon.upper_transit, true, tz, true));
            row(ui, "Moonset", event_text(day, day.moon.set, true, tz, false));
            if let Some(note) = circumpolar_note(&day.moon, "Moon") {
                row(ui, "", note);
            }
            row(
                ui,
                &format!("Position at {}", format_hm(snap.jd_ut, tz)),
                format!(
                    "alt {:+.2}°   az {:.2}° {}",
                    snap.moon.alt / DEG,
                    snap.moon.az / DEG,
                    compass_point(snap.moon.az / DEG)
                ),
            );
            row(
                ui,
                "Distance",
                format!(
                    "{:.0} km (topocentric), {:.0} km (geocentric)",
                    snap.moon.topo.distance_km, snap.moon_geo_distance_km
                ),
            );
            row(ui, "Illuminated", format!("{:.1} %", snap.illumination.fraction * 100.0));
            let age = day
                .moon_age_days(snap.jd_ut)
                .map(|a| format!(", age {a:.2} d"))
                .unwrap_or_default();
            row(ui, "Phase", format!("{}{age}", phase_name(snap.elongation_lon_deg)));
            let next = |p: Phase| {
                day.next_phase(p, snap.jd_ut)
                    .map(|t| format_date_hm(t.jd_ut, tz))
                    .unwrap_or_else(|| "—".to_owned())
            };
            row(ui, "Next new Moon", next(Phase::New));
            row(ui, "Next full Moon", next(Phase::Full));
            row(ui, "Shadow of a 1 m object", shadow(snap.moon.alt));
        });

        let ui = &mut cols[1];
        ui.strong("Sun");
        egui::Grid::new("traj_sun").num_columns(2).striped(true).show(ui, |ui| {
            let row = |ui: &mut egui::Ui, k: &str, v: String| {
                ui.label(k);
                ui.monospace(v);
                ui.end_row();
            };
            row(ui, "Sunrise", event_text(day, day.sun.rise, false, tz, false));
            row(ui, "Solar noon", event_text(day, day.sun.upper_transit, false, tz, true));
            row(ui, "Sunset", event_text(day, day.sun.set, false, tz, false));
            if let Some(note) = circumpolar_note(&day.sun, "Sun") {
                row(ui, "", note);
            }
            for (name, (dawn, dusk)) in ["Civil", "Nautical", "Astronomical"].iter().zip(day.twilights) {
                let f = |t: Option<Instant>| t.map_or("—".to_owned(), |t| format_hm(t.jd_ut, tz));
                row(ui, &format!("{name} twilight"), format!("{} – {}", f(dawn), f(dusk)));
            }
            row(
                ui,
                &format!("Position at {}", format_hm(snap.jd_ut, tz)),
                format!(
                    "alt {:+.2}°   az {:.2}° {}",
                    snap.sun.alt / DEG,
                    snap.sun.az / DEG,
                    compass_point(snap.sun.az / DEG)
                ),
            );
            row(ui, "Shadow of a 1 m object", shadow(snap.sun.alt));
        });
        ui.add_space(6.0);
        ui.weak(format!(
            "Times in UTC{:+}; rise/set for the upper limb with 34′ refraction. {}. Curves every {} min.",
            tz,
            day.ephemeris_name,
            (DAY_SAMPLE_STEP * 1440.0).round()
        ));
        if !phases_complete {
            ui.weak(
                "The new and full Moons and the age near this date reach beyond the ephemeris \
                 coverage; those outside it are not listed.",
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::status::StatusInfo;
    use std::time::{Duration, Instant as Clock};

    fn frame(ctx: &egui::Context, tab: &mut TrajectoryTab, settings: &mut Settings) {
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

    #[test]
    fn the_day_is_computed_in_the_background() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.time.tz_offset_hours = 0.0;
        settings.time.jd_ut = crate::datetime::calendar_to_jd(2024, 3, 11.0) + 0.5;
        let mut tab = TrajectoryTab::default();
        let start = Clock::now();
        while tab.day.is_none() {
            assert!(start.elapsed() < Duration::from_secs(60), "never computed");
            frame(&ctx, &mut tab, &mut settings);
            std::thread::sleep(Duration::from_millis(5));
        }
        let day = tab.day.clone().unwrap();
        assert_eq!(day.key, DayKey::new(&settings, settings.time.jd_ut));
        assert_eq!(day.samples.len(), 289);
    }

    #[test]
    fn a_day_outside_the_coverage_starts_no_job() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.time.jd_ut = crate::datetime::calendar_to_jd(-2500, 1, 1.0);
        let mut tab = TrajectoryTab::default();
        for _ in 0..3 {
            frame(&ctx, &mut tab, &mut settings);
        }
        assert!(!tab.job.is_running() && tab.day.is_none());
    }
}
