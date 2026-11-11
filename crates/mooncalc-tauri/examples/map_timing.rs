//! Map timing: serial vs parallel evaluation of a world map, and the size and
//! encode cost of the wire result.
//!
//! ```sh
//! cargo run --release -p mooncalc-tauri --example map_timing -- [resolution_deg] [criterion]
//! ```
//!
//! Defaults: 0.25° (1440 × 720), Yallop, evening of 2024-03-10, Meeus.
//! Each measurement is the best of three runs.

use std::time::Instant;

use mooncalc_app::dto::{CivilDate, Crescent, MapParams, MapTiming};
use mooncalc_app::ephemeris::meeus;
use mooncalc_app::map::MapJob;
use mooncalc_tauri_lib::parallel::{default_threads, evaluate_parallel};

fn best<T>(mut f: impl FnMut() -> T) -> (T, f64) {
    let mut out = None;
    let mut best = f64::INFINITY;
    for _ in 0..3 {
        let t = Instant::now();
        let v = f();
        best = best.min(t.elapsed().as_secs_f64() * 1e3);
        out = Some(v);
    }
    (out.unwrap(), best)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let resolution_deg: f64 = args.next().map_or(0.25, |a| a.parse().unwrap());
    let criterion_id = args.next().unwrap_or_else(|| "yallop".into());
    let params = MapParams {
        date: CivilDate {
            year: 2024,
            month: 3,
            day: 10,
        },
        criterion_id,
        crescent: Crescent::Evening,
        resolution_deg,
        bounds: None,
        ephemeris_id: None,
        include_values: true,
    };
    let eph = meeus(false);
    let (job, prepare) = best(|| MapJob::prepare(&*eph, &params).unwrap());
    println!(
        "grid {} x {} ({} cells), {} threads available",
        job.cols(),
        job.rows(),
        job.cols() * job.rows(),
        default_threads()
    );
    println!("prepare (ephemeris table, conjunctions): {prepare:8.1} ms");
    let (serial, t_serial) = best(|| job.evaluate());
    println!("evaluate, 1 thread:                      {t_serial:8.1} ms");
    let mut threads = 2;
    while threads < default_threads() {
        let (_, t) = best(|| evaluate_parallel(&job, threads, || true).unwrap());
        println!(
            "evaluate, {threads:2} threads:                    {t:8.1} ms  ({:.1}x)",
            t_serial / t
        );
        threads *= 2;
    }
    let ((par, used), t_par) =
        best(|| evaluate_parallel(&job, default_threads(), || true).unwrap());
    println!(
        "evaluate, {used:2} threads:                    {t_par:8.1} ms  ({:.1}x)",
        t_serial / t_par
    );
    assert_eq!(par.grid, serial.grid, "parallel and serial maps differ");
    let (result, t_result) = best(|| job.result(&par, MapTiming::default()));
    println!("result (zones, i16 values, counts):      {t_result:8.1} ms");
    let (json, t_json) = best(|| serde_json::to_string(&result).unwrap());
    println!(
        "JSON serialise:                          {t_json:8.1} ms, {:.2} MB",
        json.len() as f64 / 1e6
    );
    println!(
        "  cells {:.2} MB base64, values {:.2} MB base64",
        result.cells.len() as f64 / 1e6,
        result.values.as_ref().map_or(0, |v| v.data.len()) as f64 / 1e6
    );
}
