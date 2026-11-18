//! Times the egui Map tab's pipeline outside the GUI: the multi-threaded
//! grid computation (`compute_map`, all cores via `jobs::parallel_rows`)
//! and the view render (`render_view`), at every resolution, and writes the
//! rendered images and an export PNG/SVG to `target/map_timing/`.
//!
//! ```sh
//! cargo run --release -p mooncalc-gui --example map_timing -- [YYYY-MM-DD] [criterion] [width]
//! ```
//!
//! Defaults: 2024-03-10, yallop, 1600 px (the view width of a maximised
//! window). Each timing is the median of 5 runs.

use std::path::PathBuf;
use std::time::Instant as Clock;

use mooncalc_core::MeeusEphemeris;
use mooncalc_core::events::CalendarDate;
use mooncalc_gui::export::{ExportFormat, export_bytes};
use mooncalc_gui::jobs::JobContext;
use mooncalc_gui::settings::Crescent;
use mooncalc_gui::tabs::map::compute::{MapSpec, Resolution, compute_map};
use mooncalc_gui::tabs::map::draw::{self, ExportOptions, MapSettings, ProjectionKind};

fn median_ms(n: usize, mut f: impl FnMut()) -> f64 {
    let mut v: Vec<f64> = (0..n)
        .map(|_| {
            let t = Clock::now();
            f();
            t.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let date = args.first().map_or(CalendarDate::new(2024, 3, 10), |s| {
        let p: Vec<i32> = s
            .split('-')
            .map(|x| x.parse().expect("YYYY-MM-DD"))
            .collect();
        CalendarDate::new(p[0], p[1] as u32, p[2] as u32)
    });
    let criterion = args.get(1).cloned().unwrap_or_else(|| "yallop".into());
    let width: u32 = args.get(2).map_or(1600, |s| s.parse().expect("width"));
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/map_timing");
    std::fs::create_dir_all(&out).expect("output dir");

    let eph = MeeusEphemeris::new();
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    println!("{criterion} {date:?} evening, {threads} threads, view {width} px");
    let settings = MapSettings::default();
    for res in Resolution::ALL {
        let spec = MapSpec {
            date,
            crescent: Crescent::Evening,
            criterion_id: criterion.clone(),
            resolution: res,
            ephemeris: "meeus".into(),
        };
        let job = JobContext::detached();
        let compute = median_ms(5, || {
            compute_map(spec.clone(), &eph, &job).expect("map");
        });
        let map = compute_map(spec.clone(), &eph, &job).expect("map");
        let mut line = format!(
            "{:>6}: compute {compute:7.1} ms ({} cells)",
            res.label(),
            map.cells()
        );
        for kind in ProjectionKind::ALL {
            let s = MapSettings {
                projection: kind,
                ..settings.clone()
            };
            let ms = median_ms(5, || {
                draw::render_view(&map, &s, width, false);
            });
            line.push_str(&format!(", {} {ms:6.1} ms", kind.label()));
            let img = draw::render_view(&map, &s, width, false);
            let name = format!(
                "{}_{}.png",
                res.label().replace('°', "deg"),
                kind.label().to_lowercase()
            );
            std::fs::write(
                out.join(name),
                mooncalc_maps::render::encode_png(&img).expect("png"),
            )
            .expect("write");
        }
        let preview = median_ms(5, || {
            let s = MapSettings {
                projection: ProjectionKind::Globe,
                ..settings.clone()
            };
            draw::render_view(&map, &s, 720, true);
        });
        line.push_str(&format!(", globe drag preview {preview:5.1} ms"));
        println!("{line}");
        if res == Resolution::Deg1 {
            let o = ExportOptions {
                width: 2048,
                legend: true,
                title: true,
            };
            for f in [ExportFormat::Png, ExportFormat::Svg] {
                let t = Clock::now();
                let bytes = export_bytes(&map, &settings, &o, f).expect("export");
                println!(
                    "        export {} 2048 px: {:.0} ms, {} KiB",
                    f.extension(),
                    t.elapsed().as_secs_f64() * 1e3,
                    bytes.len() / 1024
                );
                std::fs::write(out.join(format!("export.{}", f.extension())), bytes)
                    .expect("write");
            }
        }
    }
    println!("wrote {}", out.display());
}
