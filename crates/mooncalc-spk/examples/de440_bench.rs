//! Time `sun_equatorial` + `moon_equatorial` of the DE440 backend, against
//! the Meeus backend for scale.
//!
//! `cargo run --release -p mooncalc-spk --example de440_bench -- path/to/de440s.bsp`

use std::hint::black_box;
use std::time::Instant;

use mooncalc_core::{Ephemeris, MeeusEphemeris};
use mooncalc_spk::De440Ephemeris;

fn time_pairs(label: &str, eph: &dyn Ephemeris, epochs: &[f64]) {
    // Warm the record caches and the branch predictors.
    for &jd in epochs.iter().take(1000) {
        black_box(eph.sun_equatorial(jd));
        black_box(eph.moon_equatorial(jd));
    }
    let t0 = Instant::now();
    let mut acc = 0.0;
    for &jd in epochs {
        acc += eph.sun_equatorial(jd).ra + eph.moon_equatorial(jd).ra;
    }
    let us = t0.elapsed().as_secs_f64() * 1e6 / epochs.len() as f64;
    black_box(acc);
    println!("{label:<52} {us:>7.2} µs per sun_equatorial + moon_equatorial");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: de440_bench <de440s.bsp>")?;
    let n = 200_000;
    // A time table: one epoch every 5 minutes (consecutive epochs share
    // Chebyshev records), and scattered epochs over 1900–2100 (every call
    // misses the record cache).
    let table: Vec<f64> = (0..n).map(|i| 2_460_000.5 + f64::from(i) / 288.0).collect();
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let scattered: Vec<f64> = (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            2_415_020.5 + (x % 73_049_000) as f64 / 1000.0
        })
        .collect();

    let file = De440Ephemeris::open(&path)?;
    let mem = De440Ephemeris::open_in_memory(&path)?;
    let meeus = MeeusEphemeris::new();
    time_pairs("DE440, file-backed, 5-minute time table", &file, &table);
    time_pairs("DE440, in memory, 5-minute time table", &mem, &table);
    time_pairs("DE440, file-backed, scattered 1900-2100", &file, &scattered);
    time_pairs("DE440, in memory, scattered 1900-2100", &mem, &scattered);
    time_pairs(
        "Meeus (truncated VSOP87D), 5-minute time table",
        &meeus,
        &table,
    );
    Ok(())
}
