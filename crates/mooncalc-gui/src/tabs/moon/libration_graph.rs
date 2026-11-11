//! Monthly libration graph: the geocentric libration in longitude against
//! the libration in latitude (Meeus ch. 53) through the current lunation,
//! a Lissajous-like curve, with the app time marked. The curve comes from
//! [`mooncalc_app::moon::libration`].

use mooncalc_core::Ephemeris;

pub use mooncalc_app::moon::libration::{LunationCurve, lb_deg};

use crate::tabs::ephemeris::chart::{self, Chart, Marker};

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
