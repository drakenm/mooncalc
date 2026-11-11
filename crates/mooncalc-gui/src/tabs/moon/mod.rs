//! **Moon** tab: a close-up of the Moon at the app time and location.
//!
//! * [`disc`]: the lunar disc with the correct phase (terminator from the
//!   Sun's selenographic longitude and latitude), orientation (axis
//!   position angle P; north up, or zenith up via the parallactic angle)
//!   and libration, with the feature layer of [`features`].
//! * [`libration_graph`]: libration in longitude against latitude through
//!   the current lunation.
//! * A data panel: illumination, phase, age, distance, apparent diameter,
//!   libration, P, colongitude, and the next sunrise/sunset at a chosen
//!   crater (Meeus ch. 53, `mooncalc_core::libration::lunar_sunrise`).

use mooncalc_core::angle::{DEG, norm_2pi};
use mooncalc_core::events::phases::SYNODIC_MONTH_DAYS;
use mooncalc_core::libration::{MoonPhysical, lunar_sunrise, lunar_sunset, sun_altitude_on_moon};
use mooncalc_core::tables::{EphemerisRow, TableOptions, ephemeris_row};
use mooncalc_core::{Ephemeris, Instant};
use serde::{Deserialize, Serialize};

use super::TabContext;
use super::ephemeris::format::{self, AngleStyle};
use super::ephemeris::{EphKey, ephemeris, set_app_time, time_link};

pub mod disc;
pub mod features;
pub mod libration_graph;

/// Persisted choices of the Moon tab.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoonSettings {
    /// Rotate the view so the observer's zenith is up (else north up).
    pub zenith_up: bool,
    /// Librations, P and illumination as seen from the app location.
    pub topocentric: bool,
    pub show_maria: bool,
    pub show_craters: bool,
    pub show_labels: bool,
    /// Crater for the lunar sunrise/sunset times.
    pub crater: String,
}

impl Default for MoonSettings {
    fn default() -> Self {
        Self {
            zenith_up: false,
            topocentric: true,
            show_maria: true,
            show_craters: true,
            show_labels: true,
            crater: "Copernicus".into(),
        }
    }
}

/// Parallactic angle q of a body at hour angle `h` and declination `dec`
/// for an observer at latitude `lat` (radians): the position angle of the
/// zenith on the body's disc (Meeus eq. 14.1).
pub fn parallactic_angle(h: f64, dec: f64, lat: f64) -> f64 {
    h.sin().atan2(lat.tan() * dec.cos() - dec.sin() * h.cos())
}

/// Name of the phase from the elongation in longitude λ☾ − λ☉ (radians):
/// the principal phases within ±6° (about half a day).
pub fn phase_name(elong_lon: f64) -> &'static str {
    let e = norm_2pi(elong_lon) / DEG;
    match e {
        e if !(6.0..354.0).contains(&e) => "New Moon",
        e if e < 84.0 => "Waxing crescent",
        e if e < 96.0 => "First quarter",
        e if e < 174.0 => "Waxing gibbous",
        e if e < 186.0 => "Full Moon",
        e if e < 264.0 => "Waning gibbous",
        e if e < 276.0 => "Last quarter",
        _ => "Waning crescent",
    }
}

/// The next instant after `jd_tt` (TT) at which the Sun rises (`rise`) or
/// sets at selenographic (`eta`, `theta`).
pub fn next_lunar_sun_event(
    eph: &dyn Ephemeris,
    eta: f64,
    theta: f64,
    jd_tt: f64,
    rise: bool,
) -> Option<f64> {
    let f = |jd: f64| {
        if rise {
            lunar_sunrise(eph, eta, theta, jd)
        } else {
            lunar_sunset(eph, eta, theta, jd)
        }
    };
    let mut t = f(jd_tt);
    for _ in 0..3 {
        if !t.is_finite() {
            return None;
        }
        if t > jd_tt {
            return Some(t);
        }
        t = f(t + SYNODIC_MONTH_DAYS);
    }
    None
}

/// Everything the tab shows for one instant and observer.
struct MoonState {
    row: EphemerisRow,
    /// Parallactic angle at the Moon, radians.
    q: f64,
    /// Elongation in longitude, radians.
    elong_lon: f64,
    crater: Option<CraterEvents>,
}

struct CraterEvents {
    name: String,
    sun_alt: f64,
    sunrise: Option<Instant>,
    sunset: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq)]
struct StateKey {
    jd_ut: f64,
    location: (f64, f64, f64),
    crater: String,
    eph: EphKey,
}

/// Quantised view geometry and texture size, to reuse the texture.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TextureKey {
    view: [i64; 5],
    size: usize,
    maria: bool,
}

/// State of the Moon tab.
#[derive(Default)]
pub struct MoonTab {
    state: Option<(StateKey, MoonState)>,
    curve: Option<libration_graph::LunationCurve>,
    texture: Option<(TextureKey, egui::TextureHandle)>,
}

impl MoonTab {
    fn update(&mut self, tc: &TabContext<'_>, eph: &dyn Ephemeris) {
        let s = &*tc.settings;
        let l = &s.location;
        let key = StateKey {
            jd_ut: s.time.jd_ut,
            location: (l.lat_deg, l.lon_deg, l.elevation_m),
            crater: s.moon_tab.crater.clone(),
            eph: EphKey::of(s),
        };
        if self.state.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let instant = s.instant();
        let loc = s.observer();
        let opts = TableOptions {
            location: Some(loc),
            ..TableOptions::default()
        };
        let row = ephemeris_row(eph, instant, &opts);
        let topo = row.moon.topocentric.expect("location given");
        let lst = instant.local_apparent_sidereal_time(loc.lon_deg * DEG);
        let q = parallactic_angle(
            lst - topo.equatorial.ra,
            topo.equatorial.dec,
            loc.lat_deg * DEG,
        );
        let elong_lon = row.moon.ecliptic.lon - row.sun.ecliptic.lon;
        let crater = features::CRATERS
            .iter()
            .find(|c| c.name == key.crater)
            .map(|c| {
                let (eta, theta) = (c.lon * DEG, c.lat * DEG);
                let p = row.physical;
                CraterEvents {
                    name: c.name.to_string(),
                    sun_alt: sun_altitude_on_moon(eta, theta, p.sun_lon, p.sun_lat),
                    sunrise: next_lunar_sun_event(eph, eta, theta, row.jd_tt, true)
                        .map(Instant::from_jd_tt),
                    sunset: next_lunar_sun_event(eph, eta, theta, row.jd_tt, false)
                        .map(Instant::from_jd_tt),
                }
            });
        if !self.curve.as_ref().is_some_and(|c| c.contains(instant)) {
            self.curve = Some(libration_graph::LunationCurve::around(eph, instant));
        }
        self.state = Some((
            key,
            MoonState {
                row,
                q,
                elong_lon,
                crater,
            },
        ));
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tc: &mut TabContext<'_>) {
        let eph = ephemeris(tc.settings);
        self.update(tc, eph.as_ref());
        let Some((_, st)) = self.state.as_ref() else {
            return;
        };
        let ms = tc.settings.moon_tab.clone();
        let physical: MoonPhysical = if ms.topocentric {
            st.row.topocentric_physical.unwrap_or(st.row.physical)
        } else {
            st.row.physical
        };
        let rotation = physical.libration.position_angle - if ms.zenith_up { st.q } else { 0.0 };
        let view = disc::View {
            l: physical.libration.lon,
            b: physical.libration.lat,
            sun_lon: physical.sun_lon,
            sun_lat: physical.sun_lat,
            rotation,
        };
        let mut clicked: Option<Instant> = None;

        egui::Panel::right("moon_side")
            .resizable(true)
            .default_size(380.0)
            .min_size(300.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let ms = &mut tc.settings.moon_tab;
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Orientation:");
                        ui.selectable_value(&mut ms.zenith_up, false, "North up");
                        ui.selectable_value(&mut ms.zenith_up, true, "Zenith up")
                            .on_hover_text("Rotate by the parallactic angle: as seen in the sky");
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut ms.topocentric, "Topocentric")
                            .on_hover_text(
                                "Librations, axis angle and illumination as seen from the app \
                             location (else geocentric)",
                            );
                        ui.checkbox(&mut ms.show_maria, "Maria");
                        ui.checkbox(&mut ms.show_craters, "Craters");
                        ui.checkbox(&mut ms.show_labels, "Labels");
                    });
                    ui.separator();
                    data_grid(
                        ui,
                        st,
                        &physical,
                        ms.topocentric,
                        tc.settings.time.tz_offset_hours,
                    );
                    ui.separator();
                    clicked = crater_ui(ui, st, tc);
                    ui.separator();
                    ui.strong("Libration through this lunation (geocentric)");
                    if let Some(curve) = &self.curve {
                        let now = libration_graph::lb_deg(eph.as_ref(), tc.settings.instant());
                        let w = ui.available_width().clamp(240.0, 420.0);
                        libration_graph::show(ui, egui::vec2(w, w * 0.9), curve, eph.as_ref(), now);
                        let tz = tc.settings.time.tz_offset_hours;
                        ui.weak(format!(
                            "New Moon {} → {}",
                            format::date_time(curve.new_moon, tz),
                            format::date_time(curve.next_new_moon, tz)
                        ));
                    }
                });
            });

        disc_ui(&mut self.texture, ui, tc, &view, st.q);
        if let Some(t) = clicked {
            set_app_time(tc, t);
        }
    }
}

/// Draw the disc, its overlay and the sky directions into the rest of `ui`.
fn disc_ui(
    texture: &mut Option<(TextureKey, egui::TextureHandle)>,
    ui: &mut egui::Ui,
    tc: &TabContext<'_>,
    view: &disc::View,
    q: f64,
) {
    let ms = &tc.settings.moon_tab;
    let tz = tc.settings.time.tz_offset_hours;
    ui.horizontal_wrapped(|ui| {
        ui.heading("Moon close-up");
        ui.label(format!(
            "{} {} — {}",
            format::date_time(tc.settings.instant(), tz),
            format::zone_label(tz),
            tc.settings.location.name
        ));
    });
    let rect = ui.available_rect_before_wrap();
    let radius = (rect.width().min(rect.height()) / 2.0 / 1.55).max(40.0);
    let (rect, _) = ui.allocate_exact_size(rect.size(), egui::Sense::hover());
    let center = rect.center();
    let painter = ui.painter_at(rect);

    let ppp = ui.ctx().pixels_per_point();
    let size = ((2.0 * radius * ppp) as usize).clamp(64, 512);
    let quant = |a: f64| (a / (0.02 * DEG)).round() as i64;
    let key = TextureKey {
        view: [
            quant(view.l),
            quant(view.b),
            quant(view.sun_lon),
            quant(view.sun_lat),
            quant(view.rotation),
        ],
        size,
        maria: ms.show_maria,
    };
    if texture.as_ref().is_none_or(|(k, _)| *k != key) {
        let image = disc::rasterize(view, size, ms.show_maria);
        let tex = ui
            .ctx()
            .load_texture("moon_disc", image, egui::TextureOptions::LINEAR);
        *texture = Some((key, tex));
    }
    if let Some((_, tex)) = texture.as_ref() {
        painter.image(
            tex.id(),
            egui::Rect::from_center_size(center, egui::vec2(2.0 * radius, 2.0 * radius)),
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
    disc::draw_overlay(
        &painter,
        view,
        center,
        radius,
        &disc::Overlay {
            maria: ms.show_maria,
            craters: ms.show_craters,
            labels: ms.show_labels,
            highlight: Some(ms.crater.as_str()),
        },
    );
    let sky = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let offset = if ms.zenith_up { -q } else { 0.0 };
    if ms.zenith_up {
        disc::draw_direction(&painter, center, radius, 0.0, "Zenith", weak);
    }
    disc::draw_direction(&painter, center, radius, offset, "N", sky);
    disc::draw_direction(&painter, center, radius, offset + 90.0 * DEG, "E", sky);
    painter.text(
        rect.left_bottom() + egui::vec2(4.0, -4.0),
        egui::Align2::LEFT_BOTTOM,
        "Naked-eye view (east left of north). Green cross: mean centre (0°, 0°); Np: lunar \
             north pole. Mare outlines are schematic.",
        egui::FontId::proportional(11.0),
        weak,
    );
}

fn data_grid(ui: &mut egui::Ui, st: &MoonState, p: &MoonPhysical, topocentric: bool, tz: f64) {
    let r = &st.row;
    let illum = if topocentric {
        r.topocentric_illumination.unwrap_or(r.illumination)
    } else {
        r.illumination
    };
    let topo = r.moon.topocentric.expect("location given");
    let style = AngleStyle::Sexagesimal;
    let rows: Vec<(&str, String)> = vec![
        ("Phase", phase_name(st.elong_lon).to_string()),
        ("Illuminated", format!("{:.1} %", illum.fraction * 100.0)),
        ("Age", format!("{:.2} days", r.moon_age_days)),
        (
            "Last new Moon",
            format::date_time(Instant::from_jd_tt(r.conjunction_jd_tt), tz),
        ),
        ("Elongation", format!("{:.2}°", illum.elongation / DEG)),
        ("Phase angle", format!("{:.2}°", illum.phase_angle / DEG)),
        (
            "Distance (geocentric)",
            format!("{:.0} km", r.moon.equatorial.distance_km),
        ),
        (
            "Distance (topocentric)",
            format!("{:.0} km", topo.equatorial.distance_km),
        ),
        (
            "Apparent diameter",
            format::small_angle(2.0 * topo.semidiameter, style),
        ),
        (
            "Libration l, b",
            format!(
                "{:+.2}°, {:+.2}°",
                p.libration.lon / DEG,
                p.libration.lat / DEG
            ),
        ),
        (
            "Axis position angle P",
            format!("{:.2}°", p.libration.position_angle / DEG),
        ),
        (
            "Bright limb χ",
            format!(
                "{:.1}° (from zenith {:.1}°)",
                illum.bright_limb_angle / DEG,
                norm_2pi(illum.bright_limb_angle - st.q) / DEG
            ),
        ),
        ("Parallactic angle q", format!("{:+.1}°", st.q / DEG)),
        ("Colongitude", format!("{:.2}°", p.colongitude / DEG)),
        ("Subsolar latitude", format!("{:+.2}°", p.sun_lat / DEG)),
        (
            "Altitude, azimuth",
            format!(
                "{}, {}",
                format::signed_angle(topo.apparent_altitude, AngleStyle::Decimal),
                format::angle_360(topo.horizontal.az, AngleStyle::Decimal)
            ),
        ),
    ];
    egui::Grid::new("moon_data")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (k, v) in rows {
                ui.label(k);
                ui.monospace(v);
                ui.end_row();
            }
        });
    ui.weak(if topocentric {
        "Librations, P and illumination: topocentric (app location)."
    } else {
        "Librations, P and illumination: geocentric."
    });
}

fn crater_ui(ui: &mut egui::Ui, st: &MoonState, tc: &mut TabContext<'_>) -> Option<Instant> {
    let tz = tc.settings.time.tz_offset_hours;
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.label("Crater:");
        egui::ComboBox::from_id_salt("moon_crater")
            .selected_text(tc.settings.moon_tab.crater.clone())
            .height(320.0)
            .show_ui(ui, |ui| {
                let mut names: Vec<&str> = features::CRATERS.iter().map(|c| c.name).collect();
                names.sort_unstable();
                for n in names {
                    ui.selectable_value(&mut tc.settings.moon_tab.crater, n.to_string(), n);
                }
            });
    });
    let Some(c) = &st.crater else {
        ui.weak("Unknown crater.");
        return None;
    };
    egui::Grid::new("crater_events")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Sun altitude");
            let alt = c.sun_alt / DEG;
            ui.monospace(format!(
                "{alt:+.1}° ({})",
                if alt > 0.0 { "day" } else { "night" }
            ));
            ui.end_row();
            for (label, t) in [("Next sunrise", c.sunrise), ("Next sunset", c.sunset)] {
                ui.label(label);
                match t {
                    Some(t) => {
                        if time_link(ui, format::date_time(t, tz)) {
                            clicked = Some(t);
                        }
                    }
                    None => {
                        ui.weak("—");
                    }
                }
                ui.end_row();
            }
        });
    ui.weak(format!("Sun on the horizon at {} (Meeus ch. 53).", c.name));
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::MeeusEphemeris;

    #[test]
    fn parallactic_angle_signs() {
        // On the meridian q = 0; west of it (H > 0) q > 0 for a northern
        // observer looking south; at the pole-facing side q = 180°.
        let lat = 40.0 * DEG;
        assert!(parallactic_angle(0.0, 10.0 * DEG, lat).abs() < 1e-12);
        assert!(parallactic_angle(0.5, 10.0 * DEG, lat) > 0.0);
        assert!(parallactic_angle(-0.5, 10.0 * DEG, lat) < 0.0);
        assert!(
            (parallactic_angle(0.0, 60.0 * DEG, lat).abs() - std::f64::consts::PI).abs() < 1e-12
        );
    }

    #[test]
    fn phase_names() {
        assert_eq!(phase_name(0.0), "New Moon");
        assert_eq!(phase_name(-3.0 * DEG), "New Moon");
        assert_eq!(phase_name(30.0 * DEG), "Waxing crescent");
        assert_eq!(phase_name(90.0 * DEG), "First quarter");
        assert_eq!(phase_name(180.0 * DEG), "Full Moon");
        assert_eq!(phase_name(-90.0 * DEG), "Last quarter");
        assert_eq!(phase_name(-30.0 * DEG), "Waning crescent");
    }

    #[test]
    fn lunar_sunrise_is_next_and_ordered() {
        // Copernicus (9.6° N, 20.1° W) gets the Sun about two days after
        // first quarter: first quarter was 2024 March 17, so sunrise is
        // around March 19.
        let eph = MeeusEphemeris::new();
        let jd = Instant::from_calendar(2024, 3, 11, 0, 0, 0.0).jd_tt();
        let (eta, theta) = (-20.1 * DEG, 9.6 * DEG);
        let rise = next_lunar_sun_event(&eph, eta, theta, jd, true).unwrap();
        let set = next_lunar_sun_event(&eph, eta, theta, jd, false).unwrap();
        assert!(rise > jd && rise - jd < SYNODIC_MONTH_DAYS);
        let c = Instant::from_jd_tt(rise).to_calendar();
        assert_eq!((c.month, c.day), (3, 19));
        // Sunset follows about half a lunation later.
        assert!((set - rise - SYNODIC_MONTH_DAYS / 2.0).abs() < 1.5);
    }
}
