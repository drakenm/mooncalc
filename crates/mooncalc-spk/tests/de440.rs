//! Tests against a real DE440/DE440s kernel and JPL Horizons.
//!
//! Set `MOONCALC_DE440` to the path of `de440s.bsp` (or `de440.bsp`) from
//! <https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets/> to run
//! them; without it they print a notice and pass.
//!
//! The reference vectors in `tests/data/horizons_geocentric.csv` are
//! geometric geocentric ICRF states from Horizons; the file header records
//! the query. Horizons computes them from DE441, which shares DE440's
//! observational fit; the residuals printed by `matches_horizons` measure the
//! combined DE440-vs-DE441 and reader differences.

mod common;

use std::io::BufReader;
use std::sync::atomic::Ordering;

use mooncalc_spk::{ByteOrder, Spk, naif, time};

fn kernel_path() -> Option<std::path::PathBuf> {
    match std::env::var_os("MOONCALC_DE440") {
        Some(p) => Some(p.into()),
        None => {
            eprintln!(
                "skipping DE440 test: set MOONCALC_DE440 to the path of de440s.bsp \
                 (https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets/)"
            );
            None
        }
    }
}

struct Row {
    target: i32,
    center: i32,
    tt: bool,
    jd: f64,
    pos: [f64; 3],
    vel: [f64; 3],
}

fn fixture() -> Vec<Row> {
    include_str!("data/horizons_geocentric.csv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            let n = |i: usize| f[i].parse::<f64>().unwrap();
            Row {
                target: f[0].parse().unwrap(),
                center: f[1].parse().unwrap(),
                tt: match f[2] {
                    "TT" => true,
                    "TDB" => false,
                    other => panic!("unknown time scale {other}"),
                },
                jd: n(3),
                pos: [n(4), n(5), n(6)],
                vel: [n(7), n(8), n(9)],
            }
        })
        .collect()
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[test]
fn lists_de440_segments() {
    let Some(path) = kernel_path() else { return };
    let (reader, count) = common::CountingReader::new(std::fs::File::open(&path).unwrap());
    let spk = Spk::from_reader(reader).unwrap();
    // Opening touches only the file record and one summary/name pair.
    assert!(count.load(Ordering::Relaxed) <= 4 * 1024);

    let fr = spk.file_record();
    assert_eq!(fr.id_word, "DAF/SPK");
    assert_eq!(spk.byte_order(), ByteOrder::Little);
    let segs = spk.segments();
    assert!(segs.len() >= 14, "{} segments", segs.len());
    for s in segs {
        assert_eq!(s.frame, naif::FRAME_J2000);
        assert_eq!(s.data_type, 2);
        assert!(s.name.starts_with("DE-0440"), "{}", s.name);
    }
    let pair = |t, c| segs.iter().any(|s| s.target == t && s.center == c);
    assert!(pair(naif::MOON, naif::EARTH_MOON_BARYCENTER));
    assert!(pair(naif::EARTH, naif::EARTH_MOON_BARYCENTER));
    assert!(pair(
        naif::EARTH_MOON_BARYCENTER,
        naif::SOLAR_SYSTEM_BARYCENTER
    ));
    assert!(pair(naif::SUN, naif::SOLAR_SYSTEM_BARYCENTER));

    // de440s spans 1849-12-26 to 2150-01-22; de440 is wider.
    let (start, end) = spk
        .common_coverage(&[
            naif::SUN,
            naif::MOON,
            naif::EARTH,
            naif::EARTH_MOON_BARYCENTER,
        ])
        .unwrap();
    assert!(time::et_to_jd_tdb(start) <= 2_396_752.5);
    assert!(time::et_to_jd_tdb(end) >= 2_506_352.5);
}

/// Horizons serves DE441, not DE440. The two share their observational fit
/// and agree closely near the middle of the span, but drift apart smoothly
/// toward its ends (measured here: Moon 0.02 m in 1974, 0.7 m in 2000,
/// 18 m in 1850 and 2150; Sun at most 0.22 m). So the sub-metre Moon target
/// applies within 1940–2005, and a looser envelope covers the rest. Reader
/// accuracy against DE440 itself is checked by `matches_jpl_testpo`.
#[test]
fn matches_horizons() {
    let Some(path) = kernel_path() else { return };
    let spk = Spk::open(&path).unwrap();
    let core_window = 2_429_629.5..=2_453_371.5; // 1940-01-01 .. 2005-01-01
    let mut worst_core_moon = 0.0f64;
    eprintln!("target scale        jd            |dr| m     |dv| mm/s");
    for r in fixture() {
        let et = if r.tt {
            time::jd_tt_to_et(r.jd)
        } else {
            time::jd_tdb_to_et(r.jd)
        };
        let (p, v) = spk.state(r.target, r.center, et).unwrap();
        let dr = dist(p, r.pos) * 1e3;
        let dv = dist(v, r.vel) * 1e6;
        eprintln!(
            "{:>6} {:>5} {:>17.6} {:>12.4} {:>12.6}",
            r.target,
            if r.tt { "TT" } else { "TDB" },
            r.jd,
            dr,
            dv
        );
        let (tol_m, tol_mm_s) = match r.target {
            naif::MOON if core_window.contains(&r.jd) => {
                worst_core_moon = worst_core_moon.max(dr);
                (1.0, 0.005)
            }
            naif::MOON => (25.0, 0.1),
            naif::SUN => (0.5, 0.002),
            t => panic!("unexpected target {t}"),
        };
        assert!(
            dr < tol_m && dv < tol_mm_s,
            "body {} at JD {}: {dr} m, {dv} mm/s",
            r.target,
            r.jd
        );
    }
    eprintln!("worst Moon residual 1940-2005: {worst_core_moon:.4} m");
}

/// The fixture has the Moon at JD 2451545.0 on both the TT and the TDB
/// scale. The DE440/DE441 offset cancels in the difference of the two
/// vectors (about 5 cm, the Moon's motion over TDB − TT = −73 µs), which
/// therefore checks `time::jd_tt_to_et` against Horizons' own conversion.
#[test]
fn tt_conversion_matches_horizons() {
    let Some(path) = kernel_path() else { return };
    let spk = Spk::open(&path).unwrap();
    let rows = fixture();
    let find = |tt: bool| {
        rows.iter()
            .find(|r| r.target == naif::MOON && r.tt == tt && r.jd == 2_451_545.0)
            .unwrap()
    };
    let (tt_row, tdb_row) = (find(true), find(false));
    let ours_tt = spk.position(naif::MOON, naif::EARTH, time::jd_tt_to_et(2_451_545.0));
    let ours_tdb = spk.position(naif::MOON, naif::EARTH, time::jd_tdb_to_et(2_451_545.0));
    let (ours_tt, ours_tdb) = (ours_tt.unwrap(), ours_tdb.unwrap());
    for k in 0..3 {
        let ours = ours_tt[k] - ours_tdb[k];
        let theirs = tt_row.pos[k] - tdb_row.pos[k];
        assert!(
            (ours - theirs).abs() < 2e-6,
            "axis {k}: {ours} km vs Horizons {theirs} km"
        );
    }
}

/// JPL's own check values for DE440 (`testpo.440`, a subset in
/// `tests/data/testpo440_subset.txt`), computed from the same Chebyshev
/// coefficients the SPK file carries. Agreement to f64 rounding verifies the
/// reader independently of any DE440/DE441 difference.
#[test]
fn matches_jpl_testpo() {
    let Some(path) = kernel_path() else { return };
    let spk = Spk::open(&path).unwrap();
    const AU_KM: f64 = 149_597_870.700;
    // testpo body codes to NAIF IDs (4-9 are system barycentres).
    let naif_id = |code: i32| match code {
        1..=2 | 4..=9 => code,
        3 => naif::EARTH,
        10 => naif::MOON,
        11 => naif::SUN,
        12 => naif::SOLAR_SYSTEM_BARYCENTER,
        13 => naif::EARTH_MOON_BARYCENTER,
        _ => panic!("unexpected testpo code {code}"),
    };
    // Bodies whose mutual vectors stay within ~1.1 au (Earth, Moon, Sun,
    // SSB, EMB): rounding there is far below 0.1 mm.
    let inner = |code: &str| matches!(code, "3" | "10" | "11" | "12" | "13");
    let (mut worst_pos, mut worst_inner, mut worst_vel, mut n) = (0.0f64, 0.0f64, 0.0f64, 0);
    for line in include_str!("data/testpo440_subset.txt").lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let jd: f64 = f[2].parse().unwrap();
        let target = naif_id(f[3].parse().unwrap());
        let center = naif_id(f[4].parse().unwrap());
        let coord: usize = f[5].parse().unwrap();
        let value: f64 = f[6].parse().unwrap();

        let (p, v) = spk.state(target, center, time::jd_tdb_to_et(jd)).unwrap();
        if coord <= 3 {
            let want_km = value * AU_KM;
            let err_km = (p[coord - 1] - want_km).abs();
            worst_pos = worst_pos.max(err_km);
            if inner(f[3]) && inner(f[4]) {
                worst_inner = worst_inner.max(err_km);
            }
            // 0.1 mm plus a few f64 ulps of the barycentric vectors being
            // chained: outer-planet vectors (~5e9 km, ulp 1 mm) differ from
            // testpo only by rounding.
            let et = time::jd_tdb_to_et(jd);
            let k = coord - 1;
            let scale = [target, center]
                .map(|b| spk.position(b, naif::SOLAR_SYSTEM_BARYCENTER, et).unwrap()[k].abs());
            let tol_km = 1e-7 + 4.0 * f64::EPSILON * (scale[0] + scale[1] + want_km.abs());
            assert!(err_km < tol_km, "{line}: off by {err_km} km");
        } else {
            let err_km_s = (v[coord - 4] - value * AU_KM / 86_400.0).abs();
            worst_vel = worst_vel.max(err_km_s);
            assert!(err_km_s < 1e-12, "{line}: off by {err_km_s} km/s");
        }
        n += 1;
    }
    assert!(n > 900, "only {n} testpo lines");
    eprintln!(
        "testpo: {n} values, worst position {:.3} mm (Earth/Moon/Sun/EMB/SSB {:.4} mm), worst velocity {:.3e} mm/s",
        worst_pos * 1e6,
        worst_inner * 1e6,
        worst_vel * 1e6
    );
}

#[test]
fn in_memory_and_file_readers_agree() {
    let Some(path) = kernel_path() else { return };
    let file = Spk::from_reader(BufReader::new(std::fs::File::open(&path).unwrap())).unwrap();
    let mem = Spk::open_in_memory(&path).unwrap();
    for jd in [2_400_000.5, 2_451_545.0, 2_460_000.25, 2_500_000.75] {
        let et = time::jd_tdb_to_et(jd);
        for (t, c) in [(naif::MOON, naif::EARTH), (naif::SUN, naif::EARTH)] {
            assert_eq!(file.state(t, c, et).unwrap(), mem.state(t, c, et).unwrap());
        }
    }
}

#[test]
fn velocity_is_the_derivative_of_position() {
    let Some(path) = kernel_path() else { return };
    let spk = Spk::open(&path).unwrap();
    // Central difference with h = 1 s: truncation error ~ h²·|r'''|/6,
    // far below 1e-6 km/s for both bodies.
    let h = 1.0;
    for jd in [2_420_000.3, 2_451_545.0, 2_470_000.8] {
        let et = time::jd_tdb_to_et(jd);
        for t in [naif::MOON, naif::SUN] {
            let (_, v) = spk.state(t, naif::EARTH, et).unwrap();
            let a = spk.position(t, naif::EARTH, et - h).unwrap();
            let b = spk.position(t, naif::EARTH, et + h).unwrap();
            for k in 0..3 {
                let num = (b[k] - a[k]) / (2.0 * h);
                assert!(
                    (num - v[k]).abs() < 1e-6,
                    "body {t} axis {k}: {num} vs {}",
                    v[k]
                );
            }
        }
    }
}
