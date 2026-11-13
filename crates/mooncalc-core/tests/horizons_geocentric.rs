//! The Meeus backend (VSOP87D Sun, Meeus-truncated ELP-2000/82 Moon) against
//! JPL Horizons (DE441) geocentric apparent positions:
//! `tests/fixtures/ephemeris/horizons_geocentric.csv`, 44 hand-picked TT
//! epochs per body, 1600–2200, and
//! `tests/fixtures/ephemeris/horizons_geocentric_random.csv`, 120 seeded
//! random TT epochs per body, 1900–2100. Tolerances are those suggested in `docs/validation.md`
//! ("Ephemeris"): for 1900–2100, Sun λ, α, δ ≤ 10″; Moon λ, α, δ ≤ 18″,
//! β ≤ 0.003° and distance ≤ 15 km; doubled outside 1900–2100.
//!
//! **Distances.** Horizons' `delta` is the light-time ("apparent") range
//! `|r_body(t − τ) − r_earth(t)|`, formed in the barycentric frame. To first
//! order it differs from the geometric geocentric distance ρ that the core
//! returns (and that parallax and semi-diameters need) by
//! `ρ − Δ = τ (v_E + v_rel) · û`, where `v_E` is the Earth's barycentric
//! velocity and `v_rel` the body's geocentric velocity. For the Moon
//! (τ ≈ 1.28 s) the `v_E` part is about +38 km · sin D, which would dominate
//! the comparison, so the test converts Horizons' range to a geometric one
//! before comparing. For the Sun the remaining term is the Sun's own
//! barycentric motion (≲ 15 m/s over τ ≈ 499 s, a few km), which the
//! heliocentric VSOP87D series cannot supply; Sun distances are compared
//! as given.
//!
//! The DE440 backend (`mooncalc-spk`) returns this light-time range as its
//! `distance_km`, so Meeus − DE440 Moon distances show the same ≈ 38 km ·
//! sin D term (RMS ≈ 27 km). Against the raw range the Moon is held to the
//! orchestrator's backend-comparison limit of 60 km. Against the geometric
//! distance it is held to the suggested 15 km.
//!
//! Moon longitude is held to 15″ (1900–2100) and 30″ outside, tighter than
//! the suggested 18″: Meeus − DE440 over 10 000 epochs in 1900–2100 reaches
//! 12.7″ (RMS 2.7″).
//!
//! Run with `cargo test -p mooncalc-core --test horizons_geocentric --
//! --nocapture` to see the residual statistics.

mod common;

use common::{Csv, Stats, moon_geometric_distance_from_horizons};
use mooncalc_core::angle::{ARCSEC, DEG, norm_pi, separation};
use mooncalc_core::meeus::moon::moon_mean_ecliptic;
use mooncalc_core::nutation::NutationObliquity;
use mooncalc_core::{MeeusEphemeris, SunSeries};

/// TT JD of 1900-01-01.0 and 2100-01-01.0 (the "modern" window).
const JD_1900: f64 = 2_415_020.5;
const JD_2100: f64 = 2_488_069.5;

#[derive(Default)]
struct BodyStats {
    ra: Stats,
    dec: Stats,
    lon: Stats,
    lat: Stats,
    dist: Stats,
    /// Core geometric distance minus Horizons' raw light-time range.
    dist_raw: Stats,
    elong: Stats,
}

/// Residuals for one body, split into the modern window and outside it.
#[derive(Default)]
struct Split {
    modern: BodyStats,
    outside: BodyStats,
}

struct Row {
    body: String,
    jd_tt: f64,
    label: String,
    ra: f64,
    dec: f64,
    lon: f64,
    lat: f64,
    dist_km: f64,
    elong: f64,
}

const FIXED: &str = "ephemeris/horizons_geocentric.csv";
const RANDOM: &str = "ephemeris/horizons_geocentric_random.csv";

/// The rows of one geocentric fixture, checking its row count.
fn rows(rel: &str, n: usize) -> Vec<Row> {
    let csv = Csv::fixture(rel);
    assert_eq!(csv.len(), n, "{rel}");
    csv.records()
        .map(|r| Row {
            body: r.str("body").to_owned(),
            jd_tt: r.f64("jd_tt"),
            label: r.str("calendar_tt").to_owned(),
            ra: r.f64("ra_app_deg") * DEG,
            dec: r.f64("dec_app_deg") * DEG,
            lon: r.f64("ecl_lon_app_deg") * DEG,
            lat: r.f64("ecl_lat_app_deg") * DEG,
            dist_km: r.f64("delta_km"),
            elong: r.f64("elong_sot_deg") * DEG,
        })
        .collect()
}

fn compare(series: SunSeries, rows: &[Row]) -> (Split, Split) {
    let eph = MeeusEphemeris::with_sun_series(series);
    let mut sun = Split::default();
    let mut moon = Split::default();
    for row in rows {
        let nut = NutationObliquity::at(row.jd_tt);
        let is_sun = match row.body.as_str() {
            "sun" => true,
            "moon" => false,
            b => panic!("unexpected body {b}"),
        };
        let (ecl, eq) = if is_sun {
            eph.sun_apparent(row.jd_tt, &nut)
        } else {
            eph.moon_apparent(row.jd_tt, &nut)
        };
        let split = if is_sun { &mut sun } else { &mut moon };
        let s = if (JD_1900..=JD_2100).contains(&row.jd_tt) {
            &mut split.modern
        } else {
            &mut split.outside
        };
        let label = format!("{} {}", row.body, row.label);
        // Angular residuals on the sky (× cos δ, × cos β).
        s.ra.add(
            norm_pi(eq.ra - row.ra) * row.dec.cos() / ARCSEC,
            label.clone(),
        );
        s.dec.add((eq.dec - row.dec) / ARCSEC, label.clone());
        s.lon.add(
            norm_pi(ecl.lon - row.lon) * row.lat.cos() / ARCSEC,
            label.clone(),
        );
        s.lat.add((ecl.lat - row.lat) / ARCSEC, label.clone());
        let reference_km = if is_sun {
            row.dist_km
        } else {
            moon_geometric_distance_from_horizons(row.jd_tt, row.dist_km)
        };
        s.dist.add(eq.distance_km - reference_km, label.clone());
        s.dist_raw.add(eq.distance_km - row.dist_km, label.clone());
        if !is_sun {
            // Elongation (geocentric ARCL) from both computed bodies.
            let (sun_ecl, _) = eph.sun_apparent(row.jd_tt, &nut);
            let e = separation(sun_ecl.lon, sun_ecl.lat, ecl.lon, ecl.lat);
            s.elong.add((e - row.elong) / ARCSEC, label);
        }
    }
    (sun, moon)
}

fn print(name: &str, b: &BodyStats, moon: bool) {
    println!("{}", b.ra.report(&format!("{name} RA·cosδ"), "\""));
    println!("{}", b.dec.report(&format!("{name} Dec"), "\""));
    println!("{}", b.lon.report(&format!("{name} λ·cosβ"), "\""));
    println!("{}", b.lat.report(&format!("{name} β"), "\""));
    println!("{}", b.dist.report(&format!("{name} distance"), " km"));
    println!(
        "{}",
        b.dist_raw
            .report(&format!("{name} distance vs raw Δ"), " km")
    );
    if moon {
        println!("{}", b.elong.report(&format!("{name} elongation"), "\""));
    }
}

/// Assert the angular limits (arcseconds) and the distance limit (km).
fn check(name: &str, b: &BodyStats, ang: f64, lat: f64, dist_km: f64) {
    for (what, s, lim) in [
        ("RA", &b.ra, ang),
        ("Dec", &b.dec, ang),
        ("λ", &b.lon, ang),
        ("β", &b.lat, lat),
        ("elongation", &b.elong, ang),
    ] {
        assert!(
            s.max <= lim,
            "{name} {what}: max {:.3}\" > {lim}\" at {}",
            s.max,
            s.max_at
        );
    }
    assert!(
        b.dist.max <= dist_km,
        "{name} distance: max {:.3} km > {dist_km} km at {}",
        b.dist.max,
        b.dist.max_at
    );
}

/// The suggested tolerances of `docs/validation.md`, plus the tighter Moon
/// longitude and the raw light-time range limit (see the module docs).
fn check_suggested(series: SunSeries, sun: &Split, moon: &Split) -> f64 {
    // The Sun angular limit is the full-VSOP87D one for both series: the
    // truncated series agrees with the full one to 0.12″. The fixture docs
    // give no Meeus Sun distance limit: the full series is held to 15 km
    // (the Sun's barycentric light-time term, see the module docs), the
    // truncated one to 150 km (10⁻⁶ AU, its documented truncation bound;
    // 150 km is 0.001″ of solar semi-diameter).
    let sun_km = match series {
        SunSeries::Truncated => 150.0,
        SunSeries::Full => 15.0,
    };
    let parts = [
        ("Sun 1900–2100", &sun.modern, 10.0, 10.0, sun_km),
        ("Sun outside", &sun.outside, 20.0, 20.0, 2.0 * sun_km),
        ("Moon 1900–2100", &moon.modern, 18.0, 10.8, 15.0),
        ("Moon outside", &moon.outside, 36.0, 21.6, 30.0),
    ];
    for (name, b, ang, lat, km) in parts {
        if b.ra.n > 0 {
            check(&format!("{series:?} {name}"), b, ang, lat, km);
        }
    }
    for (name, b, lon, raw) in [
        ("Moon 1900–2100", &moon.modern, 15.0, 60.0),
        ("Moon outside", &moon.outside, 30.0, 60.0),
    ] {
        assert!(
            b.lon.max <= lon,
            "{series:?} {name} λ {:.3}\" > {lon}\" at {}",
            b.lon.max,
            b.lon.max_at
        );
        assert!(
            b.dist_raw.max <= raw,
            "{series:?} {name} raw Δ {:.3} km > {raw} km at {}",
            b.dist_raw.max,
            b.dist_raw.max_at
        );
    }
    sun_km
}

/// The 120 seeded random epochs over 1900–2100. They sample the Meeus
/// truncation error more widely than the hand-picked epochs: the Moon's
/// RA and λ reach 11.0″ (1960-12-18), above the hand-picked set's 6.1″
/// and in line with the 12.7″ Meeus − DE440 maximum over 10 000 epochs.
#[test]
fn meeus_vs_horizons_geocentric_random() {
    let rows = rows(RANDOM, 240);
    for series in [SunSeries::Truncated, SunSeries::Full] {
        let (sun, moon) = compare(series, &rows);
        println!("--- {series:?} VSOP87D, random epochs ---");
        print("Sun 1900–2100", &sun.modern, false);
        print("Moon 1900–2100", &moon.modern, true);
        assert_eq!(sun.modern.ra.n, 120);
        assert_eq!(moon.modern.ra.n, 120);
        assert_eq!(sun.outside.ra.n + moon.outside.ra.n, 0);
        let sun_km = check_suggested(series, &sun, &moon);

        // Regression guards near the observed level, as for the fixed set.
        check(
            &format!("{series:?} Sun random (observed)"),
            &sun.modern,
            0.5,
            0.2,
            sun_km,
        );
        check(
            &format!("{series:?} Moon random (observed)"),
            &moon.modern,
            14.0,
            5.0,
            11.0,
        );
        assert!(sun.modern.ra.rms() < 0.15 && moon.modern.ra.rms() < 4.0);
    }
}

#[test]
fn meeus_vs_horizons_geocentric() {
    let rows = rows(FIXED, 88);
    for series in [SunSeries::Truncated, SunSeries::Full] {
        let (sun, moon) = compare(series, &rows);
        println!("--- {series:?} VSOP87D ---");
        print("Sun 1900–2100", &sun.modern, false);
        print("Sun outside", &sun.outside, false);
        print("Moon 1900–2100", &moon.modern, true);
        print("Moon outside", &moon.outside, true);
        assert_eq!(sun.modern.ra.n + sun.outside.ra.n, 44);
        assert_eq!(moon.modern.ra.n + moon.outside.ra.n, 44);

        let sun_km = check_suggested(series, &sun, &moon);

        // Regression guards near the observed level (about 1.5–3 times the
        // observed maxima), so that a defect much smaller than the
        // published tolerances still shows up.
        check(
            &format!("{series:?} Sun (observed)"),
            &sun.modern,
            0.5,
            0.2,
            sun_km,
        );
        check(
            &format!("{series:?} Sun outside (observed)"),
            &sun.outside,
            0.5,
            0.2,
            sun_km,
        );
        check(
            &format!("{series:?} Moon (observed)"),
            &moon.modern,
            9.0,
            4.0,
            7.0,
        );
        check(
            &format!("{series:?} Moon outside (observed)"),
            &moon.outside,
            12.0,
            4.0,
            10.0,
        );
        assert!(sun.modern.ra.rms() < 0.15 && moon.modern.ra.rms() < 4.0);
    }
}

/// Meeus example 47.a (1992 April 12, 0h TD) is in the fixture: the
/// geometric distance derived from Horizons should be within a few km of
/// both the core and Meeus' printed Δ = 368 409.7 km.
#[test]
fn meeus_47a_epoch_distance() {
    let row = rows(FIXED, 88)
        .into_iter()
        .find(|r| r.body == "moon" && r.jd_tt == 2_448_724.5)
        .expect("47.a epoch in fixture");
    let rho = moon_geometric_distance_from_horizons(row.jd_tt, row.dist_km);
    let core = moon_mean_ecliptic(row.jd_tt).distance_km;
    assert!((core - 368_409.7).abs() < 0.1, "core {core}");
    assert!(
        (rho - core).abs() < 6.0,
        "Horizons geometric {rho}, core {core}"
    );
}
