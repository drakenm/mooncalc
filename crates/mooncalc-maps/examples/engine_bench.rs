//! Times the grid engine and optionally renders the map.
//!
//! ```text
//! cargo run --release -p mooncalc-maps --example engine_bench -- \
//!     [YYYY-MM-DD] [step_deg] [criterion_id] [out.png]
//! ```
//!
//! Defaults: 2024-03-11, 1°, yallop. Prints the single-threaded timing, an
//! ASCII sketch of the zones (letters = zone codes, `.` = no crescent), and
//! writes a PNG when a path is given.

use std::time::Instant as Clock;

use mooncalc_core::MeeusEphemeris;
use mooncalc_core::events::CalendarDate;
use mooncalc_maps::engine::{EngineContext, evaluate_map};
use mooncalc_maps::grid::VisibilityGrid;
use mooncalc_maps::render::{MapData, RenderOptions, encode_png, render_raster};
use mooncalc_visibility::circumstances::Sighting;
use mooncalc_visibility::criteria::by_id;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let date = args.first().map_or(CalendarDate::new(2024, 3, 11), |s| {
        let p: Vec<u32> = s
            .split('-')
            .map(|x| x.parse().expect("YYYY-MM-DD"))
            .collect();
        CalendarDate::new(p[0] as i32, p[1], p[2])
    });
    let step: f64 = args.get(1).map_or(1.0, |s| s.parse().expect("step"));
    let criterion = by_id(args.get(2).map_or("yallop", String::as_str)).expect("criterion id");
    let rows = (180.0 / step).round() as usize;
    let grid = VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, rows, 2 * rows);
    let eph = MeeusEphemeris::new();

    let t0 = Clock::now();
    let ctx = EngineContext::new(&eph, criterion.as_ref(), &grid, date, Sighting::Evening);
    let t1 = Clock::now();
    let map = evaluate_map(&ctx, |_, _| true).expect("not cancelled");
    let t2 = Clock::now();
    let cells = rows * 2 * rows;
    println!(
        "{} {:?} {}° grid ({cells} cells): context {:.1} ms, evaluation {:.1} ms ({:.2} µs/cell), single-threaded",
        criterion.id(),
        date,
        step,
        (t1 - t0).as_secs_f64() * 1e3,
        (t2 - t1).as_secs_f64() * 1e3,
        (t2 - t1).as_secs_f64() * 1e6 / cells as f64
    );

    // ASCII sketch at about 5° resolution.
    let zones = criterion.zones();
    let (sr, sc) = ((rows / 36).max(1), (2 * rows / 72).max(1));
    for r in (0..rows).step_by(sr) {
        let line: String = (0..2 * rows)
            .step_by(sc)
            .map(|c| match map.grid.get(r, c) {
                Some(z) => zones[z as usize].code.chars().next().unwrap_or('?'),
                None => '.',
            })
            .collect();
        println!("{:+4.0} {line}", map.grid.cell_center(r, 0).0);
    }

    if let Some(path) = args.get(3) {
        let palette: Vec<[u8; 3]> = zones.iter().map(|z| z.rgb).collect();
        let data = MapData {
            grid: &map.grid,
            palette: &palette,
            none_color: None,
            field: map.field.zone_field(),
        };
        let img = render_raster(&data, &RenderOptions::default().fit_height());
        std::fs::write(path, encode_png(&img).expect("png")).expect("write");
        println!("wrote {path}");
    }
}
