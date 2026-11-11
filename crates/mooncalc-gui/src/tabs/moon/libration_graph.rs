//! Monthly libration graph: the geocentric libration in longitude against
//! the libration in latitude (Meeus ch. 53) through the current lunation,
//! a Lissajous-like curve, with the app time marked.

use mooncalc_core::angle::DEG;
use mooncalc_core::events::Phase;
use mooncalc_core::events::phases::{next_phase, previous_phase};
use mooncalc_core::libration::physical_ephemeris;
use mooncalc_core::{Ephemeris, Instant};

use crate::tabs::ephemeris::chart::{self, Chart, Marker};

/// Samples per day.
const PER_DAY: f64 = 4.0;

/// The libration curve of one lunation.
#[derive(Clone, Debug, PartialEq)]
pub struct LunationCurve {
    pub new_moon: Instant,
    pub next_new_moon: Instant,
    pub full_moon: Instant,
    /// (l, b) in degrees, every 6 hours from `new_moon` to `next_new_moon`.
    pub points: Vec<[f64; 2]>,
}

impl LunationCurve {
    /// The curve of the lunation containing `t`.
    pub fn around(eph: &dyn Ephemeris, t: Instant) -> Self {
        let new_moon = previous_phase(eph, Phase::New, t);
        let next_new_moon = next_phase(eph, Phase::New, t);
        let full_moon = next_phase(eph, Phase::Full, new_moon);
        let days = next_new_moon.jd_ut - new_moon.jd_ut;
        let n = (days * PER_DAY).ceil() as usize;
        let points = (0..=n)
            .map(|i| {
                let ti = new_moon.add_days(days * i as f64 / n as f64);
                lb_deg(eph, ti)
            })
            .collect();
        Self {
            new_moon,
            next_new_moon,
            full_moon,
            points,
        }
    }

    pub fn contains(&self, t: Instant) -> bool {
        t.jd_ut >= self.new_moon.jd_ut && t.jd_ut < self.next_new_moon.jd_ut
    }
}

/// Geocentric (l, b) in degrees at `t`.
pub fn lb_deg(eph: &dyn Ephemeris, t: Instant) -> [f64; 2] {
    let p = physical_ephemeris(eph, t.jd_tt());
    [p.libration.lon / DEG, p.libration.lat / DEG]
}

/// Draw the graph `size` points large, marking `now` (l, b in degrees).
pub fn show(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    curve: &LunationCurve,
    eph: &dyn Ephemeris,
    now: [f64; 2],
) {
    let ticks = |lo: f64, hi: f64| -> Vec<(f64, String)> {
        chart::nice_ticks(lo, hi, 8)
            .into_iter()
            .map(|v| (v, format!("{v:+.0}°")))
            .collect()
    };
    let x_ticks = ticks(-8.0, 8.0);
    let y_ticks = ticks(-8.0, 8.0);
    let first = curve.points[0];
    let full = lb_deg(eph, curve.full_moon);
    let markers = [
        Marker {
            x: first[0],
            y: first[1],
            color: egui::Color32::from_gray(90),
            label: "New".into(),
        },
        Marker {
            x: full[0],
            y: full[1],
            color: egui::Color32::from_gray(235),
            label: "Full".into(),
        },
        Marker {
            x: now[0],
            y: now[1],
            color: egui::Color32::from_rgb(230, 160, 40),
            label: "now".into(),
        },
    ];
    chart::show(
        ui,
        size,
        &Chart {
            x_range: (-9.0, 9.0),
            y_range: (-8.5, 8.5),
            x_ticks: &x_ticks,
            y_ticks: &y_ticks,
            x_label: "Libration in longitude l",
            y_label: "Latitude b",
            points: &curve.points,
            markers: &markers,
            equal_aspect: true,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_core::MeeusEphemeris;

    #[test]
    fn lunation_curve_spans_the_libration_range() {
        let eph = MeeusEphemeris::new();
        let t = Instant::from_calendar(2024, 3, 11, 12, 0, 0.0);
        let c = LunationCurve::around(&eph, t);
        assert!(c.contains(t));
        assert!(c.full_moon.jd_ut > c.new_moon.jd_ut && c.full_moon.jd_ut < c.next_new_moon.jd_ut);
        // Librations reach several degrees in both coordinates and stay
        // within the physical limits (|l| < 8.2°, |b| < 7°).
        let max_l = c.points.iter().map(|p| p[0].abs()).fold(0.0, f64::max);
        let max_b = c.points.iter().map(|p| p[1].abs()).fold(0.0, f64::max);
        assert!((4.0..8.2).contains(&max_l), "{max_l}");
        assert!((5.0..7.0).contains(&max_b), "{max_b}");
    }
}
