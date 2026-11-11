//! The topocentric pipeline with the Meeus ephemeris against JPL Horizons
//! topocentric alt/az: `tests/fixtures/ephemeris/horizons_topocentric.csv`,
//! Sun and Moon at five sites and six UT epochs each (2000–2030).
//!
//! Pipeline: UTC → TT (32.184 s + TAI − UTC) → UT1 = TT − ΔT (the core's
//! [`Instant::from_jd_tt`]), geocentric apparent position at TT, GAST with
//! the same nutation, [`topocentric`] (WGS84, site height),
//! [`equatorial_to_horizontal`], then Sæmundsson refraction
//! ([`refraction_from_true`]) at the standard 1010 mbar and 10 °C.
//!
//! **Time scale.** Horizons "UT" is UTC from 1962, so `jd_ut` is a UTC Julian
//! Day. Treating it as TT − ΔT directly would be off by up to the
//! accumulated leap seconds minus ΔT (≈ 0.4–0.7 s here), and treating it as
//! UT1 ignores UT1 − UTC (< 0.9 s); both shift the hour angle by up to
//! ≈ 13″. The test converts UTC to TT exactly and lets the core's ΔT give
//! UT1, so it exercises the same path an application uses. For 2030 that
//! ΔT is an extrapolation; Horizons' own implies ΔT ≈ 69.4 s there.
//!
//! **Refraction.** Horizons' "approximate atmospheric yellow-light
//! refraction" is Sæmundsson's formula (Meeus eq. 16.4) at 1010 mbar and
//! 10 °C, including Meeus' zenith constant, held at its −1° value below
//! that altitude: `refracted − airless` in the fixture matches the core's
//! [`refraction_from_true`] to 0.003′ at every row, and the core clamps at
//! −1° too. Bennett's formula (eq. 16.3), inverted, is 0.01–0.06′ lower.
//!
//! Tolerances (`docs/validation.md`): airless alt/az ≤ 0.01°; topocentric
//! RA/Dec as geocentric plus 1″ (Sun 11″, Moon 19″); refracted altitude, for
//! rows with an airless altitude above 0°, ≤ 0.02° above 10° and ≤ 0.1°
//! from 0° to 10°.
//!
//! Run with `--nocapture` to see the residual statistics.

mod common;

use common::{Csv, Stats, moon_geometric_distance_from_horizons, tai_minus_utc};
use mooncalc_core::angle::{ARCMIN, ARCSEC, DEG, norm_pi};
use mooncalc_core::refraction::{Atmosphere, refraction_from_apparent, refraction_from_true};
use mooncalc_core::time::SECONDS_PER_DAY;
use mooncalc_core::topocentric::{ObserverGeocentric, topocentric};
use mooncalc_core::transform::equatorial_to_horizontal;
use mooncalc_core::{Equatorial, Horizontal, Instant, MeeusEphemeris, SunSeries};

struct Computed {
    jd_tt: f64,
    ut1_minus_utc_s: f64,
    topo: Equatorial,
    airless: Horizontal,
}

/// Run the pipeline for one fixture row.
fn compute(eph: &MeeusEphemeris, r: &common::Record) -> Computed {
    let jd_utc = r.f64("jd_ut");
    let tai_utc = tai_minus_utc(jd_utc).expect("fixture epochs are after 1972");
    let jd_tt = jd_utc + (32.184 + tai_utc) / SECONDS_PER_DAY;
    let inst = Instant::from_jd_tt(jd_tt);
    let both = eph.sun_moon_apparent(jd_tt);
    let geo = match r.str("body") {
        "sun" => both.sun_equatorial,
        "moon" => both.moon_equatorial,
        b => panic!("unexpected body {b}"),
    };
    let lat = r.f64("lat_deg") * DEG;
    let lon = r.f64("lon_deg") * DEG;
    let obs = ObserverGeocentric::new(lat, r.f64("height_m"));
    let last = inst.gast_with(&both.nutation) + lon;
    let topo = topocentric(&geo, &obs, last);
    Computed {
        jd_tt,
        ut1_minus_utc_s: (inst.jd_ut - jd_utc) * SECONDS_PER_DAY,
        topo,
        airless: equatorial_to_horizontal(&topo, lat, last),
    }
}

#[derive(Default)]
struct BodyStats {
    ra: Stats,
    dec: Stats,
    az: Stats,
    alt: Stats,
    dist: Stats,
    refr_low: Stats,
    refr_high: Stats,
}

fn label(r: &common::Record) -> String {
    format!(
        "{} {} {}",
        r.str("site"),
        r.str("body"),
        r.str("calendar_ut")
    )
}

fn run(series: SunSeries) -> (BodyStats, BodyStats) {
    let csv = Csv::fixture("ephemeris/horizons_topocentric.csv");
    assert_eq!(csv.len(), 60);
    let eph = MeeusEphemeris::with_sun_series(series);
    let mut sun = BodyStats::default();
    let mut moon = BodyStats::default();
    for r in csv.records() {
        let c = compute(&eph, &r);
        let is_sun = r.str("body") == "sun";
        let s = if is_sun { &mut sun } else { &mut moon };
        let l = label(&r);
        let dec_h = r.f64("dec_app_topo_deg") * DEG;
        let alt_h = r.f64("alt_airless_deg") * DEG;
        s.ra.add(
            norm_pi(c.topo.ra - r.f64("ra_app_topo_deg") * DEG) * dec_h.cos() / ARCSEC,
            l.clone(),
        );
        s.dec.add((c.topo.dec - dec_h) / ARCSEC, l.clone());
        // Azimuth on the sky (× cos alt).
        s.az.add(
            norm_pi(c.airless.az - r.f64("az_airless_deg") * DEG) * alt_h.cos() / ARCSEC,
            l.clone(),
        );
        s.alt.add((c.airless.alt - alt_h) / ARCSEC, l.clone());
        let reference_km = if is_sun {
            r.f64("delta_topo_km")
        } else {
            moon_geometric_distance_from_horizons(c.jd_tt, r.f64("delta_topo_km"))
        };
        s.dist.add(c.topo.distance_km - reference_km, l.clone());
        if alt_h > 0.0 {
            let apparent =
                c.airless.alt + refraction_from_true(c.airless.alt, &Atmosphere::STANDARD);
            let d = (apparent - r.f64("alt_refracted_deg") * DEG) / ARCSEC;
            if alt_h > 10.0 * DEG {
                s.refr_high.add(d, l);
            } else {
                s.refr_low.add(d, l);
            }
        }
    }
    (sun, moon)
}

fn print(name: &str, b: &BodyStats) {
    println!("{}", b.ra.report(&format!("{name} topo RA·cosδ"), "\""));
    println!("{}", b.dec.report(&format!("{name} topo Dec"), "\""));
    println!(
        "{}",
        b.az.report(&format!("{name} az·cos(alt) airless"), "\"")
    );
    println!("{}", b.alt.report(&format!("{name} alt airless"), "\""));
    println!("{}", b.dist.report(&format!("{name} topo distance"), " km"));
    println!(
        "{}",
        b.refr_low
            .report(&format!("{name} refracted alt 0–10°"), "\"")
    );
    println!(
        "{}",
        b.refr_high
            .report(&format!("{name} refracted alt >10°"), "\"")
    );
}

fn check(name: &str, b: &BodyStats, radec: f64, altaz: f64, dist_km: f64) {
    for (what, s, lim) in [
        ("topo RA", &b.ra, radec),
        ("topo Dec", &b.dec, radec),
        ("az", &b.az, altaz),
        ("alt", &b.alt, altaz),
        ("refracted alt 0–10°", &b.refr_low, 0.1 * 3600.0),
        ("refracted alt >10°", &b.refr_high, 0.02 * 3600.0),
        ("distance (km)", &b.dist, dist_km),
    ] {
        assert!(
            s.max <= lim,
            "{name} {what}: max {:.3} > {lim} at {}",
            s.max,
            s.max_at
        );
    }
}

#[test]
fn meeus_pipeline_vs_horizons_topocentric() {
    for series in [SunSeries::Truncated, SunSeries::Full] {
        let (sun, moon) = run(series);
        println!("--- {series:?} VSOP87D ---");
        print("Sun", &sun);
        print("Moon", &moon);
        assert_eq!(sun.alt.n + moon.alt.n, 60);
        // Suggested tolerances.
        let sun_km = match series {
            SunSeries::Truncated => 150.0,
            SunSeries::Full => 15.0,
        };
        check(&format!("{series:?} Sun"), &sun, 11.0, 36.0, sun_km);
        check(&format!("{series:?} Moon"), &moon, 19.0, 36.0, 15.0);
        // Regression guards near the observed level.
        check(
            &format!("{series:?} Sun (observed)"),
            &sun,
            1.0,
            10.0,
            sun_km,
        );
        check(
            &format!("{series:?} Moon (observed)"),
            &moon,
            8.0,
            12.0,
            6.0,
        );
        for b in [&sun, &moon] {
            assert!(b.refr_low.max <= 12.0 && b.refr_high.max <= 12.0);
        }
    }
}

/// Horizons' refraction model is Sæmundsson at the standard atmosphere,
/// independently of any ephemeris error: apply the core's refraction to
/// Horizons' own airless altitude and compare.
#[test]
fn horizons_refraction_is_saemundsson() {
    let csv = Csv::fixture("ephemeris/horizons_topocentric.csv");
    let mut sae = Stats::default();
    let mut ben = Stats::default();
    for r in csv.records() {
        let alt = r.f64("alt_airless_deg") * DEG;
        let refr_h = r.f64("alt_refracted_deg") * DEG - alt;
        // Below the horizon too: both clamp at −1°.
        let d = refraction_from_true(alt, &Atmosphere::STANDARD) - refr_h;
        sae.add(d / ARCMIN, label(&r));
        if alt > 0.0 {
            // Bennett, solved for the apparent altitude by iteration.
            let mut app = alt + refr_h;
            for _ in 0..20 {
                app = alt + refraction_from_apparent(app, &Atmosphere::STANDARD);
            }
            ben.add((app - alt - refr_h) / ARCMIN, label(&r));
        }
        // Azimuth is unaffected by refraction.
        assert_eq!(r.f64("az_airless_deg"), r.f64("az_refracted_deg"));
    }
    println!("{}", sae.report("Sæmundsson − Horizons (all rows)", "'"));
    println!("{}", ben.report("Bennett − Horizons (alt > 0)", "'"));
    assert_eq!(sae.n, 60);
    // Horizons prints altitudes to 1e-9°, so this is the model identity
    // up to rounding of Meeus' printed constants.
    assert!(sae.max < 0.001, "Sæmundsson differs by {}′", sae.max);
    // Bennett and Sæmundsson are consistent to about 0.07′ (Meeus ch. 16).
    assert!(ben.max < 0.1, "Bennett differs by {}′", ben.max);
}

/// UT1 − UTC implied by the core's ΔT at the fixture epochs, against the
/// IERS values (Bulletin A/C04: 2000-01-01 +0.355 s, 2015-06-15 −0.67 s,
/// 2024–2025 within ±0.05 s). For 2030 the core extrapolates.
#[test]
fn ut1_minus_utc_from_core_delta_t() {
    let csv = Csv::fixture("ephemeris/horizons_topocentric.csv");
    let eph = MeeusEphemeris::new();
    for r in csv.records() {
        let c = compute(&eph, &r);
        let jd = r.f64("jd_ut");
        let year = 2000.0 + (jd - 2_451_544.5) / 365.25;
        let lim = if year < 2027.0 { 0.1 } else { 1.0 };
        let expected = if year < 2001.0 {
            0.355
        } else if year < 2016.0 {
            -0.67
        } else if year < 2027.0 {
            0.0
        } else {
            -0.25 // as implied by Horizons for 2030-09-01
        };
        assert!(
            (c.ut1_minus_utc_s - expected).abs() <= lim,
            "{}: UT1−UTC {:.3} s, expected {expected} ± {lim}",
            label(&r),
            c.ut1_minus_utc_s
        );
    }
}
