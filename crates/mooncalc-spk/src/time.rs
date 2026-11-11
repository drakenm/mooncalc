//! Conversions between Julian Days and SPICE ephemeris time (ET).
//!
//! SPK files are indexed by ET: TDB seconds past the J2000 epoch,
//! JD 2451545.0 TDB (*Time Required Reading*, `time.req`).
//!
//! TDB and TT differ by a periodic term below 1.7 ms. This module uses the
//! two-term approximation of the NAIF leapseconds kernel (`naif0012.tls`,
//! keywords `DELTET/K`, `DELTET/EB` and `DELTET/M`), which follows Moyer,
//! "Transformation from proper time on Earth to coordinate time in solar
//! system barycentric space-time frame of reference", *Celestial Mechanics*
//! 23, 33–56 (1981):
//!
//! ```text
//! TDB − TT = K sin E,   E = M + EB sin M,   M = M0 + M1 · t
//! ```
//!
//! with `K = 1.657e-3 s`, `EB = 1.671e-2`, `M0 = 6.239996`,
//! `M1 = 1.99096871e-7 rad/s` and `t` in seconds past J2000. It agrees with
//! the full Fairhead–Bretagnon series to about 30 µs, which moves the Moon
//! by about 3 cm.

/// Julian Day of the J2000 epoch (2000-01-01 12:00 TDB).
pub const J2000_JD: f64 = 2_451_545.0;

/// Seconds in a Julian day.
pub const SECONDS_PER_DAY: f64 = 86_400.0;

const DELTET_K: f64 = 1.657e-3;
const DELTET_EB: f64 = 1.671e-2;
const DELTET_M0: f64 = 6.239_996;
const DELTET_M1: f64 = 1.990_968_71e-7;

/// ET (TDB seconds past J2000) for a TDB Julian Day.
pub fn jd_tdb_to_et(jd_tdb: f64) -> f64 {
    (jd_tdb - J2000_JD) * SECONDS_PER_DAY
}

/// ET for a two-part TDB Julian Day `jd1 + jd2`. Splitting the date (for
/// example into the integer day and the fraction) keeps sub-microsecond
/// precision that a single `f64` Julian Day (about 40 µs resolution) loses.
pub fn jd2_tdb_to_et(jd1: f64, jd2: f64) -> f64 {
    ((jd1 - J2000_JD) + jd2) * SECONDS_PER_DAY
}

/// TDB Julian Day for an ET.
pub fn et_to_jd_tdb(et: f64) -> f64 {
    J2000_JD + et / SECONDS_PER_DAY
}

/// `TDB − TT` in seconds, at `t` seconds past J2000 (TT or TDB: the
/// difference changes the result by less than a nanosecond).
pub fn tdb_minus_tt(t: f64) -> f64 {
    let m = DELTET_M0 + DELTET_M1 * t;
    DELTET_K * (m + DELTET_EB * m.sin()).sin()
}

/// ET for a TT Julian Day, applying [`tdb_minus_tt`].
pub fn jd_tt_to_et(jd_tt: f64) -> f64 {
    let tt = (jd_tt - J2000_JD) * SECONDS_PER_DAY;
    tt + tdb_minus_tt(tt)
}

/// TT Julian Day for an ET.
pub fn et_to_jd_tt(et: f64) -> f64 {
    J2000_JD + (et - tdb_minus_tt(et)) / SECONDS_PER_DAY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn j2000_is_zero() {
        assert_eq!(jd_tdb_to_et(J2000_JD), 0.0);
        assert_eq!(et_to_jd_tdb(0.0), J2000_JD);
        assert_eq!(jd_tdb_to_et(J2000_JD + 1.0), 86_400.0);
        assert_eq!(jd2_tdb_to_et(2_451_545.0, 0.5), 43_200.0);
    }

    #[test]
    fn tdb_minus_tt_bounds_and_period() {
        let mut max = 0.0f64;
        for day in 0..366 {
            let t = f64::from(day) * SECONDS_PER_DAY;
            max = max.max(tdb_minus_tt(t).abs());
        }
        assert!(max <= DELTET_K && max > 1.6e-3, "max {max}");
        // At J2000: M = 6.239996, E = M + EB sin M, K sin E = -7.3 µs
        // (hand computation from the naif0012.tls constants).
        let at_j2000 = tdb_minus_tt(0.0);
        let e = DELTET_M0 + DELTET_EB * DELTET_M0.sin();
        assert!((at_j2000 - DELTET_K * e.sin()).abs() < 1e-18);
        assert!((at_j2000 + 7.3e-5).abs() < 0.5e-5, "{at_j2000}");
    }

    #[test]
    fn tt_round_trip() {
        for jd in [2_396_800.25, 2_451_545.0, 2_460_000.123, 2_506_000.9] {
            let et = jd_tt_to_et(jd);
            assert!((et_to_jd_tt(et) - jd).abs() * SECONDS_PER_DAY < 1e-4);
        }
    }
}
