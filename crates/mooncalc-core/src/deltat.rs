//! ΔT = TT − UT.
//!
//! * 1620–2010: observed values, Meeus *Astronomical Algorithms* 2nd ed.,
//!   Table 10.A (biennial, extended to 2010 with the published values).
//! * 2011–2026.75: annual values derived from the IERS EOP 20 C04 series
//!   (Paris Observatory), ΔT = 32.184 s + (TAI − UTC) − (UT1 − UTC), at
//!   0h UTC on January 1 (and 2026 October 1, the last value used).
//! * 1600–1620: a linear bridge from the polynomial value at 1600 to the
//!   first table value.
//! * Elsewhere: the polynomial expressions of F. Espenak & J. Meeus,
//!   *Five Millennium Canon of Solar Eclipses: −1999 to +3000*
//!   (NASA/TP-2006-214141, 2006), also published on the NASA eclipse web
//!   site ("Polynomial expressions for Delta T").
//! * After the last observed value the Espenak–Meeus polynomial is used with
//!   the offset at the end of the table fading linearly to zero over
//!   [`FADE_YEARS`] years, so ΔT is continuous and returns to the canonical
//!   long-term extrapolation.
//!
//! Inside the table, values are linearly interpolated; the table is smooth
//! enough that this adds < 0.1 s. The Espenak–Meeus expressions assume a
//! lunar secular acceleration of −25.858″/cy²; no correction is applied for
//! other values.

use crate::time::{J2000, julian_day};

/// Years over which the offset between the last observed value and the
/// polynomial extrapolation is faded out.
pub const FADE_YEARS: f64 = 75.0;

const TABLE_10A_START: f64 = 1620.0;
/// Start of the linear bridge from the polynomials to Table 10.A.
const BRIDGE_START: f64 = 1600.0;
/// Meeus Table 10.A, years 1620, 1622, …, 2010 (seconds).
#[rustfmt::skip]
const TABLE_10A: [f64; 196] = [
    121.0, 112.0, 103.0, 95.0, 88.0, 82.0, 77.0, 72.0, 68.0, 63.0,
    60.0, 56.0, 53.0, 51.0, 48.0, 46.0, 44.0, 42.0, 40.0, 38.0,
    35.0, 33.0, 31.0, 29.0, 26.0, 24.0, 22.0, 20.0, 18.0, 16.0,
    14.0, 12.0, 11.0, 10.0, 9.0, 8.0, 7.0, 7.0, 7.0, 7.0,
    7.0, 7.0, 8.0, 8.0, 9.0, 9.0, 9.0, 9.0, 9.0, 10.0,
    10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 11.0, 11.0, 11.0,
    11.0, 11.0, 12.0, 12.0, 12.0, 12.0, 13.0, 13.0, 13.0, 14.0,
    14.0, 14.0, 14.0, 15.0, 15.0, 15.0, 15.0, 15.0, 16.0, 16.0,
    16.0, 16.0, 16.0, 16.0, 16.0, 16.0, 15.0, 15.0, 14.0, 13.0,
    13.1, 12.5, 12.2, 12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 11.9,
    11.6, 11.0, 10.2, 9.2, 8.2, 7.1, 6.2, 5.6, 5.4, 5.3,
    5.4, 5.6, 5.9, 6.2, 6.5, 6.8, 7.1, 7.3, 7.5, 7.6,
    7.7, 7.3, 6.2, 5.2, 2.7, 1.4, -1.2, -2.8, -3.8, -4.8,
    -5.5, -5.3, -5.6, -5.7, -5.9, -6.0, -6.3, -6.5, -6.2, -4.7,
    -2.8, -0.1, 2.6, 5.3, 7.7, 10.4, 13.3, 16.0, 18.2, 20.2,
    21.1, 22.4, 23.5, 23.8, 24.3, 24.0, 23.9, 23.9, 23.7, 24.0,
    24.3, 25.3, 26.2, 27.3, 28.2, 29.1, 30.0, 30.7, 31.4, 32.2,
    33.1, 34.0, 35.0, 36.5, 38.3, 40.2, 42.2, 44.5, 46.5, 48.5,
    50.5, 52.2, 53.8, 54.9, 55.8, 56.9, 58.3, 60.0, 61.6, 63.0,
    63.8, 64.3, 64.6, 64.8, 65.5, 66.1,
];
const TABLE_10A_END: f64 = TABLE_10A_START + 2.0 * (TABLE_10A.len() as f64 - 1.0);

/// IERS-derived values (year, seconds) after Table 10.A.
#[rustfmt::skip]
const IERS: [(f64, f64); 17] = [
    (2011.0, 66.325), (2012.0, 66.603), (2013.0, 66.907), (2014.0, 67.281),
    (2015.0, 67.644), (2016.0, 68.102), (2017.0, 68.593), (2018.0, 68.968),
    (2019.0, 69.220), (2020.0, 69.361), (2021.0, 69.359), (2022.0, 69.295),
    (2023.0, 69.204), (2024.0, 69.175), (2025.0, 69.138), (2026.0, 69.110),
    (2026.75, 69.206),
];

/// The last year covered by observed values.
pub const LAST_OBSERVED_YEAR: f64 = IERS[IERS.len() - 1].0;

/// Decimal year for a Julian Day (mean Gregorian year from J2000).
#[inline]
pub fn decimal_year(jd: f64) -> f64 {
    // 2000 January 1.0 = JD 2451544.5.
    2000.0 + (jd - (J2000 - 0.5)) / 365.2425
}

/// Julian Day for a decimal year (inverse of [`decimal_year`]).
#[inline]
pub fn jd_from_decimal_year(year: f64) -> f64 {
    (J2000 - 0.5) + (year - 2000.0) * 365.2425
}

/// ΔT in seconds for a Julian Day (UT; TT gives the same result to well
/// under a millisecond).
pub fn delta_t_seconds(jd: f64) -> f64 {
    delta_t_for_year(decimal_year(jd))
}

/// ΔT in seconds for a decimal year.
pub fn delta_t_for_year(y: f64) -> f64 {
    if (TABLE_10A_START..=TABLE_10A_END).contains(&y) {
        let x = (y - TABLE_10A_START) / 2.0;
        let i = (x.floor() as usize).min(TABLE_10A.len() - 2);
        let f = x - i as f64;
        return TABLE_10A[i] + f * (TABLE_10A[i + 1] - TABLE_10A[i]);
    }
    if (BRIDGE_START..TABLE_10A_START).contains(&y) {
        // Espenak–Meeus (≈120 s at 1600) and Table 10.A (121 s at 1620) are
        // built from different data; bridge them linearly.
        let f = (y - BRIDGE_START) / (TABLE_10A_START - BRIDGE_START);
        let a = espenak_meeus(BRIDGE_START);
        return a + f * (TABLE_10A[0] - a);
    }
    if y > TABLE_10A_END && y <= LAST_OBSERVED_YEAR {
        let mut prev = (TABLE_10A_END, TABLE_10A[TABLE_10A.len() - 1]);
        for &(yr, dt) in &IERS {
            if y <= yr {
                let f = (y - prev.0) / (yr - prev.0);
                return prev.1 + f * (dt - prev.1);
            }
            prev = (yr, dt);
        }
    }
    if y > LAST_OBSERVED_YEAR {
        let (y0, dt0) = IERS[IERS.len() - 1];
        let offset = dt0 - espenak_meeus(y0);
        let w = (1.0 - (y - y0) / FADE_YEARS).max(0.0);
        return espenak_meeus(y) + offset * w;
    }
    espenak_meeus(y)
}

/// The Espenak & Meeus (2006) polynomial expressions for ΔT (seconds) at a
/// decimal year, without the observed tables.
pub fn espenak_meeus(y: f64) -> f64 {
    fn poly(t: f64, c: &[f64]) -> f64 {
        c.iter().rev().fold(0.0, |acc, &k| acc * t + k)
    }
    if y < -500.0 {
        let u = (y - 1820.0) / 100.0;
        -20.0 + 32.0 * u * u
    } else if y < 500.0 {
        let u = y / 100.0;
        poly(
            u,
            &[
                10583.6,
                -1014.41,
                33.78311,
                -5.952053,
                -0.1798452,
                0.022174192,
                0.0090316521,
            ],
        )
    } else if y < 1600.0 {
        let u = (y - 1000.0) / 100.0;
        poly(
            u,
            &[
                1574.2,
                -556.01,
                71.23472,
                0.319781,
                -0.8503463,
                -0.005050998,
                0.0083572073,
            ],
        )
    } else if y < 1700.0 {
        let t = y - 1600.0;
        poly(t, &[120.0, -0.9808, -0.01532, 1.0 / 7129.0])
    } else if y < 1800.0 {
        let t = y - 1700.0;
        poly(
            t,
            &[8.83, 0.1603, -0.0059285, 0.00013336, -1.0 / 1_174_000.0],
        )
    } else if y < 1860.0 {
        let t = y - 1800.0;
        poly(
            t,
            &[
                13.72,
                -0.332447,
                0.0068612,
                0.0041116,
                -0.00037436,
                0.0000121272,
                -0.0000001699,
                0.000000000875,
            ],
        )
    } else if y < 1900.0 {
        let t = y - 1860.0;
        poly(
            t,
            &[
                7.62,
                0.5737,
                -0.251754,
                0.01680668,
                -0.0004473624,
                1.0 / 233_174.0,
            ],
        )
    } else if y < 1920.0 {
        let t = y - 1900.0;
        poly(t, &[-2.79, 1.494119, -0.0598939, 0.0061966, -0.000197])
    } else if y < 1941.0 {
        let t = y - 1920.0;
        poly(t, &[21.20, 0.84493, -0.076100, 0.0020936])
    } else if y < 1961.0 {
        let t = y - 1950.0;
        poly(t, &[29.07, 0.407, -1.0 / 233.0, 1.0 / 2547.0])
    } else if y < 1986.0 {
        let t = y - 1975.0;
        poly(t, &[45.45, 1.067, -1.0 / 260.0, -1.0 / 718.0])
    } else if y < 2005.0 {
        let t = y - 2000.0;
        poly(
            t,
            &[
                63.86,
                0.3345,
                -0.060374,
                0.0017275,
                0.000651814,
                0.00002373599,
            ],
        )
    } else if y < 2050.0 {
        let t = y - 2000.0;
        poly(t, &[62.92, 0.32217, 0.005589])
    } else if y < 2150.0 {
        let u = (y - 1820.0) / 100.0;
        -20.0 + 32.0 * u * u - 0.5628 * (2150.0 - y)
    } else {
        let u = (y - 1820.0) / 100.0;
        -20.0 + 32.0 * u * u
    }
}

/// ΔT for a UT calendar date, convenient for tests and display.
pub fn delta_t_for_date(year: i32, month: u32, day: f64) -> f64 {
    delta_t_seconds(julian_day(year, month, day))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_values() {
        // Observed ΔT: 1900.0 ≈ −2.8 s, 2000.0 ≈ 63.8 s (Meeus Table 10.A).
        assert!((delta_t_for_date(1900, 1, 1.0) + 2.8).abs() < 0.1);
        assert!((delta_t_for_date(2000, 1, 1.0) - 63.83).abs() < 0.05);
        // Meeus example 10.a: 1977 February 18, ΔT = 48 s (to the second).
        assert!((delta_t_for_date(1977, 2, 18.0) - 48.0).abs() < 1.0);
        // IERS: 2020.0 = 69.36 s, 2017.0 = 68.59 s.
        assert!((delta_t_for_date(2020, 1, 1.0) - 69.36).abs() < 0.01);
        assert!((delta_t_for_date(2017, 1, 1.0) - 68.59).abs() < 0.01);
    }

    #[test]
    fn espenak_meeus_polynomials() {
        // Values from the Espenak & Meeus (2006) table of ΔT.
        for (y, dt, tol) in [
            (-500.0, 17190.0, 20.0),
            (0.0, 10580.0, 10.0),
            (500.0, 5710.0, 10.0),
            (1000.0, 1570.0, 10.0),
            (1500.0, 200.0, 5.0),
            (1700.0, 9.0, 1.0),
            (1800.0, 14.0, 1.0),
            (1900.0, -3.0, 1.0),
            (1950.0, 29.0, 1.0),
            (2000.0, 64.0, 1.0),
        ] {
            let got = espenak_meeus(y);
            assert!((got - dt).abs() < tol, "{y}: {got}");
        }
    }

    #[test]
    fn continuity() {
        // No jumps where the data sources join.
        for y in [1600.0, 1620.0, TABLE_10A_END, LAST_OBSERVED_YEAR] {
            let a = delta_t_for_year(y - 1e-6);
            let b = delta_t_for_year(y + 1e-6);
            assert!((a - b).abs() < 0.5, "{y}: {a} {b}");
        }
        // The far future rejoins the canonical extrapolation.
        let y = LAST_OBSERVED_YEAR + FADE_YEARS + 10.0;
        assert_eq!(delta_t_for_year(y), espenak_meeus(y));
    }

    #[test]
    fn near_future_extrapolation_is_sane() {
        // 2030: ΔT will be close to 69–72 s, not the 75+ s of the raw polynomial.
        let dt = delta_t_for_year(2030.0);
        assert!(dt > 68.0 && dt < 73.0, "{dt}");
    }
}
