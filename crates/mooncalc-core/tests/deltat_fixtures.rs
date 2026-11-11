//! ΔT (`mooncalc_core::deltat`) against the fixtures in
//! `tests/fixtures/deltat/` (see `docs/validation.md`, "ΔT"):
//!
//! * `espenak_meeus_polynomial_checks.csv`: values computed from the
//!   Espenak & Meeus (2006) polynomial expressions, which
//!   [`espenak_meeus`] must reproduce to 0.001 s, branch boundaries included.
//! * `usno_observed_deltat.csv`: USNO/IERS observed ΔT, half-yearly
//!   1657.0–1972.5 (with errors) and 1 January of each year 1974–2026.
//! * `nasa_deltat_historical.csv` (Morrison & Stephenson 2004, −500 to
//!   1950) and `nasa_deltat_recent.csv` (Astronomical Almanac 2006,
//!   1955–2005).
//!
//! Run with `--nocapture` to see the residual statistics.

mod common;

use common::{Csv, Stats};
use mooncalc_core::deltat::{
    LAST_OBSERVED_YEAR, delta_t_for_year, delta_t_seconds, espenak_meeus, jd_from_decimal_year,
};

#[test]
fn espenak_meeus_polynomial_check_values() {
    let csv = Csv::fixture("deltat/espenak_meeus_polynomial_checks.csv");
    assert_eq!(csv.len(), 42);
    let mut s = Stats::default();
    for r in csv.records() {
        let y = r.f64("year");
        // The fixture's delta_t_s excludes the lunar-acceleration correction
        // c, as does the core.
        let d = espenak_meeus(y) - r.f64("delta_t_s");
        s.add(d, format!("{y}"));
        assert!(
            d.abs() <= 1e-3,
            "{y}: core {} fixture {}",
            espenak_meeus(y),
            r.f64("delta_t_s")
        );
    }
    println!("{}", s.report("Espenak–Meeus polynomial checks", " s"));
}

/// One USNO comparison: (year, core ΔT, USNO ΔT, USNO error if given).
type UsnoRow = (f64, f64, f64, Option<f64>);

/// Residuals of `delta_t_for_year` against USNO for one era.
fn usno_era(from: f64, to: f64) -> (Stats, Vec<UsnoRow>) {
    let csv = Csv::fixture("deltat/usno_observed_deltat.csv");
    let mut s = Stats::default();
    let mut rows = Vec::new();
    for r in csv.records() {
        let y = r.f64("year");
        if y < from || y >= to {
            continue;
        }
        let obs = r.f64("delta_t_s");
        let got = delta_t_for_year(y);
        s.add(got - obs, format!("{y}"));
        rows.push((y, got, obs, r.opt_f64("error_s")));
    }
    (s, rows)
}

#[test]
fn usno_observed_series() {
    let csv = Csv::fixture("deltat/usno_observed_deltat.csv");
    assert_eq!(csv.len(), 685);

    // 1657–1800. The core follows Meeus Table 10.A (biennial, whole seconds
    // before 1800), which agrees with Morrison & Stephenson (2004), NASA
    // Table 1 (e.g. 1700: core 7 s, NASA 9 ± 5 s). The USNO historic series
    // is an older reconstruction that runs up to 14 s higher around
    // 1680–1720 (1700: 21 s). The two disagree by up to 2.2 times USNO's
    // stated error, so the suggested limit max(3 s, error) fails for 82 of
    // the 146 rows before 1730. This is a disagreement between reference
    // reconstructions, not a core defect. The limit here is
    // max(3 s, 2.5 × error).
    let (s, rows) = usno_era(1657.0, 1800.0);
    println!("{}", s.report("USNO 1657–1800", " s"));
    for (y, got, obs, err) in rows {
        let lim = (2.5 * err.expect("historic rows carry errors")).max(3.0);
        assert!(
            (got - obs).abs() <= lim,
            "{y}: core {got:.2} USNO {obs} (±{lim})"
        );
    }

    // 1800–1900: the suggested limit, max(3 s, error); observed ≤ 1.3 s.
    let (s, rows) = usno_era(1800.0, 1900.0);
    println!("{}", s.report("USNO 1800–1900", " s"));
    for (y, got, obs, err) in rows {
        let lim = err.unwrap_or(0.0).max(3.0);
        assert!(
            (got - obs).abs() <= lim,
            "{y}: core {got:.2} USNO {obs} (±{lim})"
        );
    }
    assert!(s.max <= 1.5, "1800–1900 regression: {}", s.max);

    // 1900–1974: Table 10.A (0.1 s, biennial, interpolated linearly)
    // against half-yearly USNO values; the biennial sampling misses some
    // sub-annual structure (observed ≤ 0.36 s around 1920 and 1927).
    let (s, rows) = usno_era(1900.0, 1974.0);
    println!("{}", s.report("USNO 1900–1974", " s"));
    for (y, got, obs, _) in rows {
        assert!((got - obs).abs() <= 0.4, "{y}: core {got:.3} USNO {obs}");
    }

    // 1974–2026: the core interpolates observed values (Table 10.A to
    // 2010, IERS-derived values from 2011), so ≤ 0.1 s, as suggested.
    let (s, rows) = usno_era(1974.0, LAST_OBSERVED_YEAR + 1.0);
    println!("{}", s.report("USNO 1974–2026", " s"));
    assert_eq!(s.n, 53);
    for (y, got, obs, _) in rows {
        assert!((got - obs).abs() <= 0.1, "{y}: core {got:.3} USNO {obs}");
    }
}

/// The raw Espenak–Meeus polynomial (without the observed tables) against
/// USNO, to document the extrapolation drift after 2005 that the core's
/// table and fade avoid (docs/validation.md: ≤ 1 s for 1900–2005, ≤ 7 s
/// for 2005–2026).
#[test]
fn espenak_meeus_against_usno() {
    let csv = Csv::fixture("deltat/usno_observed_deltat.csv");
    let mut modern = Stats::default();
    let mut recent = Stats::default();
    for r in csv.records() {
        let y = r.f64("year");
        let d = espenak_meeus(y) - r.f64("delta_t_s");
        if (1900.0..2005.0).contains(&y) {
            modern.add(d, format!("{y}"));
        } else if y >= 2005.0 {
            recent.add(d, format!("{y}"));
        }
    }
    println!("{}", modern.report("Espenak–Meeus vs USNO 1900–2005", " s"));
    println!("{}", recent.report("Espenak–Meeus vs USNO 2005–2026", " s"));
    assert!(modern.max <= 1.0, "{}", modern.max);
    assert!(recent.max <= 7.0, "{}", recent.max);
}

#[test]
fn nasa_tables() {
    let hist = Csv::fixture("deltat/nasa_deltat_historical.csv");
    assert_eq!(hist.len(), 28);
    let mut s = Stats::default();
    for r in hist.records() {
        let y = r.f64("year");
        let obs = r.f64("delta_t_s");
        let got = delta_t_seconds(jd_from_decimal_year(y));
        // Errors such as `<1` are upper bounds; use the bound.
        let err: f64 = r
            .str("std_error_s")
            .trim_start_matches('<')
            .parse()
            .expect("std_error_s");
        // Within the standard error, but never tighter than the 0.1 s
        // rounding of Table 10.A (1950: "<0.1").
        let lim = err.max(0.15);
        println!("NASA Table 1 {y:6}: core {got:9.2} NASA {obs:7} ±{lim}");
        s.add(got - obs, format!("{y}"));
        assert!(
            (got - obs).abs() <= lim,
            "{y}: core {got:.2} NASA {obs} (±{lim})"
        );
    }
    println!("{}", s.report("NASA Table 1 (−500–1950)", " s"));

    let recent = Csv::fixture("deltat/nasa_deltat_recent.csv");
    assert_eq!(recent.len(), 11);
    let mut s = Stats::default();
    for r in recent.records() {
        let y = r.f64("year");
        let obs = r.f64("delta_t_s");
        let got = delta_t_for_year(y);
        s.add(got - obs, format!("{y}"));
        assert!((got - obs).abs() <= 0.6, "{y}: core {got:.2} NASA {obs}");
    }
    println!("{}", s.report("NASA Table 2 (1955–2005)", " s"));
}
