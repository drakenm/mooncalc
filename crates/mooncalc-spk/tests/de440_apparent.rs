//! Tests of the apparent-place backend against JPL Horizons and the Meeus
//! backend. Set `MOONCALC_DE440` to the path of `de440s.bsp` (or
//! `de440.bsp`) to run the kernel tests; without it they print a notice and
//! pass.
//!
//! References:
//! * `tests/fixtures/ephemeris/horizons_geocentric.csv` (workspace
//!   validation fixture, 44 epochs 1600–2200, see `docs/validation.md`):
//!   Horizons apparent RA/Dec and ecliptic of date and apparent range.
//! * `tests/data/horizons_apparent.csv` (this crate, 25 more epochs
//!   1850–2150): the same quantities plus the **astrometric** ICRF RA/Dec,
//!   which isolates the ephemeris and light time from the frame models.
//!
//! Horizons computes from DE441, with "EOP-corrected IAU76/80" precession
//! and nutation whose RA origin is "offset −53 mas from the of-date frame
//! defined by the IAU06/00a P & N system". Its EOP corrections are data from
//! 1962-01-20 to 2026-11-10 and are held fixed outside that window, so
//! there it follows IAU 1976 precession plus a constant offset.

mod common;

use mooncalc_core::angle::{ARCSEC, DEG};
use mooncalc_core::matrix;
use mooncalc_core::{Ephemeris, MeeusEphemeris};
use mooncalc_spk::{Body, De440Ephemeris, FrameModel};

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

const AU_KM: f64 = 149_597_870.700;
/// Horizons' EOP data window (TT Julian Days of 1962-01-20 and 2026-11-10).
const EOP_WINDOW: std::ops::RangeInclusive<f64> = 2_437_684.5..=2_461_354.5;
/// Horizons' RA origin minus the IAU 2006/2000A equinox, mas.
const HORIZONS_RA_ORIGIN_MAS: f64 = -53.0;

#[derive(Clone, Copy, Debug)]
struct Row {
    body: Body,
    jd_tt: f64,
    ra: f64,
    dec: f64,
    delta_km: f64,
    lon: f64,
    lat: f64,
    /// Astrometric ICRF RA/Dec (crate fixture only).
    astrometric: Option<(f64, f64)>,
}

fn crate_fixture() -> Vec<Row> {
    include_str!("data/horizons_apparent.csv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<f64> = l.split(',').map(|x| x.trim().parse().unwrap()).collect();
            Row {
                body: match f[0] as i32 {
                    301 => Body::Moon,
                    10 => Body::Sun,
                    t => panic!("unexpected target {t}"),
                },
                jd_tt: f[1],
                ra: f[2] * DEG,
                dec: f[3] * DEG,
                delta_km: f[4] * AU_KM,
                lon: f[5] * DEG,
                lat: f[6] * DEG,
                astrometric: Some((f[7] * DEG, f[8] * DEG)),
            }
        })
        .collect()
}

fn workspace_fixture() -> Vec<Row> {
    let text = include_str!("../../../tests/fixtures/ephemeris/horizons_geocentric.csv");
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<&str> = lines.next().unwrap().split(',').collect();
    let col = |name: &str| header.iter().position(|h| *h == name).unwrap();
    let (c_body, c_jd, c_ra, c_dec) = (
        col("body"),
        col("jd_tt"),
        col("ra_app_deg"),
        col("dec_app_deg"),
    );
    let (c_lon, c_lat, c_km) = (
        col("ecl_lon_app_deg"),
        col("ecl_lat_app_deg"),
        col("delta_km"),
    );
    lines
        .map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            let n = |i: usize| f[i].trim().parse::<f64>().unwrap();
            Row {
                body: match f[c_body] {
                    "moon" => Body::Moon,
                    "sun" => Body::Sun,
                    b => panic!("unexpected body {b}"),
                },
                jd_tt: n(c_jd),
                ra: n(c_ra) * DEG,
                dec: n(c_dec) * DEG,
                delta_km: n(c_km),
                lon: n(c_lon) * DEG,
                lat: n(c_lat) * DEG,
                astrometric: None,
            }
        })
        .collect()
}

/// Both fixtures, restricted to the kernel's range.
fn rows_in(eph: &De440Ephemeris) -> Vec<Row> {
    let (a, b) = eph.valid_range();
    let rows: Vec<Row> = workspace_fixture()
        .into_iter()
        .chain(crate_fixture())
        .filter(|r| (a..=b).contains(&r.jd_tt))
        .collect();
    assert!(rows.len() >= 90, "only {} rows in range", rows.len());
    rows
}

fn wrap(a: f64) -> f64 {
    (a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}

fn mas(rad: f64) -> f64 {
    rad / ARCSEC * 1e3
}

/// Largest absolute value.
fn max_abs(v: &[f64]) -> f64 {
    v.iter().fold(0.0f64, |m, x| m.max(x.abs()))
}

#[test]
fn astrometric_matches_horizons() {
    // Light time and the DE440/DE441 difference only. The Moon's DE440 vs
    // DE441 offset reaches 18 m (10 mas) at the ends of the span (see
    // tests/de440.rs) and is below 1 m (0.5 mas) within 1940–2005.
    let Some(path) = kernel_path() else { return };
    let eph = De440Ephemeris::open(&path).unwrap();
    let mut worst = [0.0f64; 2];
    for r in crate_fixture() {
        let (ra, dec) = r.astrometric.unwrap();
        let (p, tau) = eph.astrometric(r.body, r.jd_tt).unwrap();
        let (our_ra, our_dec, dist) = matrix::to_spherical(&p);
        let sep = mas(mooncalc_core::angle::separation(our_ra, our_dec, ra, dec));
        let k = (r.body == Body::Moon) as usize;
        worst[k] = worst[k].max(sep);
        assert!((tau - dist / mooncalc_spk::de440::C_KM_S).abs() < 1e-6);
        let tol = if (2_429_629.5..=2_453_371.5).contains(&r.jd_tt) {
            1.0
        } else {
            12.0
        };
        assert!(sep < tol, "{:?} JD {}: {sep} mas", r.body, r.jd_tt);
    }
    eprintln!(
        "astrometric ICRF vs Horizons: worst Sun {:.2} mas, Moon {:.2} mas",
        worst[0], worst[1]
    );
}

#[test]
fn iau2006_matches_horizons_apparent() {
    let Some(path) = kernel_path() else { return };
    let eph = De440Ephemeris::open(&path).unwrap();
    assert_eq!(eph.frame_model(), FrameModel::Iau2006);
    let mut stats: [[Vec<f64>; 3]; 4] = Default::default(); // [in/out × body][ra, dec, dist]
    eprintln!("body  jd_tt              dRA·cosδ  dDec (mas, RA origin −53 mas removed)  dist (m)");
    for r in rows_in(&eph) {
        let p = eph.apparent(r.body, r.jd_tt).unwrap().equatorial;
        let inside = EOP_WINDOW.contains(&r.jd_tt);
        let dra = (mas(wrap(p.ra - r.ra)) + HORIZONS_RA_ORIGIN_MAS) * r.dec.cos();
        let ddec = mas(p.dec - r.dec);
        let dd = (p.distance_km - r.delta_km) * 1e3;
        eprintln!(
            "{:<5} {:>16.6} {:>9.2} {:>9.2} {:>9.3}{}",
            format!("{:?}", r.body),
            r.jd_tt,
            dra,
            ddec,
            dd,
            if inside { "" } else { "  (outside EOP window)" }
        );
        let k = 2 * (!inside as usize) + (r.body == Body::Moon) as usize;
        stats[k][0].push(dra);
        stats[k][1].push(ddec);
        stats[k][2].push(dd);

        // Distance: DE440 vs DE441 (Moon ≤ 18 m at the ends, Sun < 0.3 m).
        let dist_tol = if r.body == Body::Moon { 20.0 } else { 1.0 };
        assert!(dd.abs() < dist_tol, "{r:?}: distance off by {dd} m");
        if inside {
            // IAU 1980 nutation (63 terms) against the EOP-corrected pole:
            // about ±12 mas.
            assert!(dra.abs() < 20.0 && ddec.abs() < 15.0, "{r:?}: {dra} {ddec}");
        } else {
            // Horizons follows IAU 1976 precession here; the difference is
            // the IAU 2006 rate correction, 0.3″/cy, up to 0.45″ at the ends
            // of de440s (validation.md tolerance 0.5″).
            assert!(
                dra.abs() < 500.0 && ddec.abs() < 500.0,
                "{r:?}: {dra} {ddec}"
            );
        }
    }
    for (k, label) in [
        "Sun, 1962–2026",
        "Moon, 1962–2026",
        "Sun, outside",
        "Moon, outside",
    ]
    .iter()
    .enumerate()
    {
        eprintln!(
            "{label}: n={} max |dRA·cosδ| {:.1} mas, |dDec| {:.1} mas, |dist| {:.3} m",
            stats[k][0].len(),
            max_abs(&stats[k][0]),
            max_abs(&stats[k][1]),
            max_abs(&stats[k][2])
        );
    }
}

/// With [`FrameModel::Iau1976`] the backend uses Horizons' own theory, so
/// the residuals isolate the EOP corrections: the ecliptic latitude (which
/// a correction in longitude Δψ does not change, and whose obliquity both
/// sides take from IAU 1976) agrees everywhere, and outside the EOP window,
/// where Horizons holds its corrections fixed, the ecliptic longitude
/// residual is a constant on each side. This checks light time, aberration
/// and nutation over the whole 1850–2150 span.
#[test]
fn iau1976_matches_horizons_up_to_fixed_eop_offsets() {
    let Some(path) = kernel_path() else { return };
    let eph = De440Ephemeris::open(&path)
        .unwrap()
        .with_frame_model(FrameModel::Iau1976);
    let (mut before, mut after, mut lat) = (Vec::new(), Vec::new(), Vec::new());
    for r in rows_in(&eph) {
        let e = eph.apparent(r.body, r.jd_tt).unwrap().ecliptic;
        let dlon = mas(wrap(e.lon - r.lon)) * r.lat.cos();
        let dlat = mas(e.lat - r.lat);
        lat.push(dlat);
        // Horizons prints ecliptic angles to 1e-7° (0.36 mas).
        assert!(dlat.abs() < 15.0, "{r:?}: dlat {dlat} mas");
        if r.jd_tt < *EOP_WINDOW.start() {
            before.push(dlon);
        } else if r.jd_tt > 2_461_440.5 {
            // After the EOP predictions end (2027-02-04).
            after.push(dlon);
        }
    }
    let spread = |v: &[f64]| {
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        (
            mean,
            max_abs(&v.iter().map(|x| x - mean).collect::<Vec<_>>()),
        )
    };
    let (mb, sb) = spread(&before);
    let (ma, sa) = spread(&after);
    eprintln!(
        "IAU 1976 mode: max |dlat| {:.1} mas; dlon before 1962 {mb:.1} ± {sb:.1} mas (n={}), after 2027 {ma:.1} ± {sa:.1} mas (n={})",
        max_abs(&lat),
        before.len(),
        after.len()
    );
    assert!(before.len() >= 10 && after.len() >= 10);
    assert!(sb < 15.0 && sa < 15.0, "{sb} {sa}");
}

/// The Meeus backend against DE440 over 1900–2100, every 7.3 days with a
/// varying time of day. The core documents the Sun to about 1″ and the
/// Moon to about 10″ in longitude and 4″ in latitude. Meeus's truncated
/// ELP-2000/82 distance series is good to a few tens of km: his own example
/// 47.a gives 368 409.7 km where Horizons has 368 439.4 km.
#[test]
fn meeus_cross_check() {
    let Some(path) = kernel_path() else { return };
    let de = De440Ephemeris::open(&path)
        .unwrap()
        .with_frame_model(FrameModel::Iau1976);
    let meeus = MeeusEphemeris::new();
    let start = 2_415_020.5; // 1900-01-01
    let n = 10_000;
    let step = 73_049.0 / f64::from(n); // to 2100-01-01
    let mut d: [Vec<f64>; 6] = Default::default(); // sun lon, lat, moon lon, lat, sun km, moon km
    let mut worst_moon = (0.0, 0.0);
    for i in 0..n {
        let jd = start + f64::from(i) * step + 0.137;
        let sm = de.sun_moon(jd).unwrap();
        let (s, m) = (meeus.sun_ecliptic(jd), meeus.moon_ecliptic(jd));
        let dsl = wrap(s.lon - sm.sun.ecliptic.lon) * sm.sun.ecliptic.lat.cos() / ARCSEC;
        let dml = wrap(m.lon - sm.moon.ecliptic.lon) * sm.moon.ecliptic.lat.cos() / ARCSEC;
        if dml.abs() > f64::abs(worst_moon.1) {
            worst_moon = (jd, dml);
        }
        d[0].push(dsl);
        d[1].push((s.lat - sm.sun.ecliptic.lat) / ARCSEC);
        d[2].push(dml);
        d[3].push((m.lat - sm.moon.ecliptic.lat) / ARCSEC);
        d[4].push(s.distance_km - sm.sun.ecliptic.distance_km);
        d[5].push(m.distance_km - sm.moon.ecliptic.distance_km);
    }
    let summary = |v: &mut Vec<f64>| {
        let rms = (v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64).sqrt();
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let mut a: Vec<f64> = v.iter().map(|x| x.abs()).collect();
        a.sort_by(f64::total_cmp);
        let p95 = a[a.len() * 95 / 100];
        (mean, rms, p95, a[a.len() - 1])
    };
    let labels = [
        "Sun λ (″)",
        "Sun β (″)",
        "Moon λ (″)",
        "Moon β (″)",
        "Sun r (km)",
        "Moon r (km)",
    ];
    let mut s = Vec::new();
    for (v, label) in d.iter_mut().zip(labels) {
        let (mean, rms, p95, max) = summary(v);
        eprintln!(
            "Meeus − DE440 {label:<12} mean {mean:>9.3} rms {rms:>8.3} p95 {p95:>8.3} max {max:>8.3}"
        );
        s.push(max);
    }
    eprintln!(
        "worst Moon λ at JD {:.3}: {:.2}″",
        worst_moon.0, worst_moon.1
    );
    // Generous bounds: the point of the test is the printed distribution.
    assert!(s[0] < 3.0 && s[1] < 1.0, "Sun {} {}", s[0], s[1]);
    assert!(s[2] < 40.0 && s[3] < 15.0, "Moon {} {}", s[2], s[3]);
    assert!(s[4] < 300.0 && s[5] < 80.0, "distances {} {}", s[4], s[5]);
}

#[test]
fn trait_methods_and_fast_path_agree() {
    let Some(path) = kernel_path() else { return };
    let eph = De440Ephemeris::open(&path).unwrap();
    let dyn_eph: &dyn Ephemeris = &eph;
    assert_eq!(dyn_eph.name(), "JPL DE440 (IAU 2006/1980)");
    let (a, b) = dyn_eph.valid_range();
    // de440s: 1849-12-26 to 2150-01-22 (TDB), less the light-time margin.
    assert!(a > 2_396_752.5 && a < 2_396_753.0, "{a}");
    assert!(b > 2_506_352.0 && b < 2_506_353.0, "{b}");

    for jd in [2_400_000.25, 2_451_545.0, 2_460_384.1, 2_500_000.9] {
        let sm = eph.sun_moon(jd).unwrap();
        // Alternate the calls so the epoch cache is hit and missed.
        assert_eq!(dyn_eph.moon_equatorial(jd), sm.moon.equatorial);
        assert_eq!(dyn_eph.sun_equatorial(jd), sm.sun.equatorial);
        assert_eq!(dyn_eph.sun_ecliptic(jd), sm.sun.ecliptic);
        assert_eq!(dyn_eph.moon_ecliptic(jd), sm.moon.ecliptic);
        assert_eq!(dyn_eph.moon_equatorial(jd + 0.5), {
            eph.apparent(Body::Moon, jd + 0.5).unwrap().equatorial
        });
        // Light time: 1.2–1.4 s for the Moon, 490–510 s for the Sun.
        assert!((1.15..1.4).contains(&sm.moon.light_time_s));
        assert!((485.0..512.0).contains(&sm.sun.light_time_s));
        // Ecliptic and equatorial are the same direction.
        let eps = mooncalc_core::nutation::nutation(jd).deps
            + mooncalc_spk::de440::mean_obliquity_iau2006(jd);
        let back = mooncalc_core::transform::ecliptic_to_equatorial(&sm.moon.ecliptic, eps);
        assert!(wrap(back.ra - sm.moon.equatorial.ra).abs() < 1e-12);
        assert!((back.dec - sm.moon.equatorial.dec).abs() < 1e-12);
    }

    // Outside the range: NaN from the trait, an error from the fallible API.
    for jd in [a - 1.0, b + 1.0] {
        assert!(dyn_eph.moon_equatorial(jd).ra.is_nan());
        assert!(dyn_eph.sun_ecliptic(jd).lon.is_nan());
        assert!(eph.apparent(Body::Sun, jd).is_err());
    }
    assert!(!dyn_eph.sun_equatorial(a).ra.is_nan());
    assert!(!dyn_eph.moon_equatorial(b).ra.is_nan());
}

#[test]
fn file_and_memory_backends_agree() {
    let Some(path) = kernel_path() else { return };
    let file = De440Ephemeris::open(&path).unwrap();
    let mem = De440Ephemeris::from_bytes(std::fs::read(&path).unwrap()).unwrap();
    for jd in [2_420_000.5, 2_460_000.123] {
        assert_eq!(file.sun_moon(jd).unwrap(), mem.sun_moon(jd).unwrap());
    }
}

#[test]
fn rejects_kernels_without_the_bodies() {
    // Synthetic SPKs: one with only a Mars barycentre segment, one with
    // every required body but the Sun.
    let seg = |target, center| common::SegSpec {
        name: "SYNTH",
        target,
        center,
        frame: 1,
        data_type: 2,
        start_et: -1e6,
        end_et: 1e6,
        init: -1e6,
        intlen: 2e6,
        records: vec![vec![0.0, 1e6, 1e5, 0.0, 2e5, 0.0, 3e5, 0.0]],
    };
    let mars = common::build_spk(&[seg(4, 0)], false, 10);
    assert!(matches!(
        De440Ephemeris::from_bytes(mars),
        Err(mooncalc_spk::SpkError::NoCoverage { body: 10, .. })
    ));
    let no_sun = common::build_spk(&[seg(3, 0), seg(301, 3), seg(399, 3)], false, 10);
    assert!(De440Ephemeris::from_bytes(no_sun).is_err());
    // With the Sun added it opens, and covers the synthetic span.
    let all = common::build_spk(&[seg(3, 0), seg(301, 3), seg(399, 3), seg(10, 0)], true, 2);
    let eph = De440Ephemeris::from_bytes(all).unwrap();
    assert_eq!(eph.name(), "JPL SPK (IAU 2006/1980)");
    let (a, b) = eph.valid_range();
    assert!(a < 2_451_545.0 && b > 2_451_545.0);
}

#[test]
fn is_send_sync_and_boxable() {
    fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<De440Ephemeris>();
    let _: Option<std::sync::Arc<dyn Ephemeris>> = None::<De440Ephemeris>.map(|e| {
        let arc: std::sync::Arc<dyn Ephemeris> = std::sync::Arc::new(e);
        arc
    });
}
