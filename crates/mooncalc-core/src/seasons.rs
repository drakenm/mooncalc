//! Equinoxes and solstices (Meeus, *Astronomical Algorithms* 2nd ed.,
//! ch. 27) and the equation of time (ch. 28).
//!
//! Two methods are given for the seasons:
//!
//! * [`season_jde_meeus`]: the mean instants of Tables 27.A/27.B corrected
//!   by the 24 periodic terms of Table 27.C. Meeus states an error below
//!   51 s for 1951–2050 and a few minutes at the ends of the range
//!   −1000…+3000.
//! * [`season_jde`]: the instant when the Sun's apparent longitude, from
//!   any [`Ephemeris`], equals 0°, 90°, 180° or 270°, found by the
//!   iteration of Meeus eq. 27.1. Accurate to the ephemeris (about 1 s with
//!   the default VSOP87D Sun).

use crate::angle::{DEG, norm_2pi, norm_pi};
use crate::ephemeris::Ephemeris;
use crate::nutation::{NutationObliquity, mean_obliquity};
use crate::time::{DAYS_PER_CENTURY, J2000, centuries_since_j2000};

/// One of the four astronomical seasons' starting points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Season {
    /// Sun's apparent longitude 0° (northern spring).
    MarchEquinox,
    /// 90°.
    JuneSolstice,
    /// 180°.
    SeptemberEquinox,
    /// 270°.
    DecemberSolstice,
}

impl Season {
    /// All four in calendar order.
    pub const ALL: [Season; 4] = [
        Season::MarchEquinox,
        Season::JuneSolstice,
        Season::SeptemberEquinox,
        Season::DecemberSolstice,
    ];

    /// The Sun's apparent ecliptic longitude at this event, radians.
    pub fn solar_longitude(self) -> f64 {
        match self {
            Season::MarchEquinox => 0.0,
            Season::JuneSolstice => 90.0 * DEG,
            Season::SeptemberEquinox => 180.0 * DEG,
            Season::DecemberSolstice => 270.0 * DEG,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Meeus Table 27.A (years −1000…+1000, `Y = year / 1000`): JDE₀ polynomial
/// coefficients for March, June, September, December.
const TABLE_27A: [[f64; 5]; 4] = [
    [
        1_721_139.291_89,
        365_242.137_40,
        0.061_34,
        0.001_11,
        -0.000_71,
    ],
    [
        1_721_233.254_01,
        365_241.725_62,
        -0.052_32,
        0.009_07,
        0.000_25,
    ],
    [
        1_721_325.704_55,
        365_242.495_58,
        -0.116_77,
        -0.002_97,
        0.000_74,
    ],
    [
        1_721_414.399_87,
        365_242.882_57,
        -0.007_69,
        -0.009_33,
        -0.000_06,
    ],
];

/// Meeus Table 27.B (years +1000…+3000, `Y = (year − 2000) / 1000`).
const TABLE_27B: [[f64; 5]; 4] = [
    [
        2_451_623.809_84,
        365_242.374_04,
        0.051_69,
        -0.004_11,
        -0.000_57,
    ],
    [
        2_451_716.567_67,
        365_241.626_03,
        0.003_25,
        0.008_88,
        -0.000_30,
    ],
    [
        2_451_810.217_15,
        365_242.017_67,
        -0.115_75,
        0.003_37,
        0.000_78,
    ],
    [
        2_451_900.059_52,
        365_242.740_49,
        -0.062_23,
        -0.008_23,
        0.000_32,
    ],
];

/// Meeus Table 27.C: periodic terms `A cos(B + C T)`, B and C in degrees.
#[rustfmt::skip]
const TABLE_27C: [[f64; 3]; 24] = [
    [485.0, 324.96, 1934.136], [203.0, 337.23, 32964.467],
    [199.0, 342.08, 20.186], [182.0, 27.85, 445267.112],
    [156.0, 73.14, 45036.886], [136.0, 171.52, 22518.443],
    [77.0, 222.54, 65928.934], [74.0, 296.72, 3034.906],
    [70.0, 243.58, 9037.513], [58.0, 119.81, 33718.147],
    [52.0, 297.17, 150.678], [50.0, 21.02, 2281.226],
    [45.0, 247.54, 29929.562], [44.0, 325.15, 31555.956],
    [29.0, 60.93, 4443.417], [18.0, 155.12, 67555.328],
    [17.0, 288.79, 4562.452], [16.0, 198.04, 62894.029],
    [14.0, 199.76, 31436.921], [12.0, 95.39, 14577.848],
    [12.0, 287.11, 31931.756], [12.0, 320.81, 34777.259],
    [9.0, 227.73, 1222.114], [8.0, 15.45, 16859.074],
];

fn horner(x: f64, c: &[f64]) -> f64 {
    c.iter().rev().fold(0.0, |acc, &k| acc * x + k)
}

/// Mean instant JDE₀ (TT Julian Day) of a season (Meeus Tables 27.A/B).
pub fn mean_season_jde(year: i32, season: Season) -> f64 {
    if year < 1000 {
        horner(year as f64 / 1000.0, &TABLE_27A[season.index()])
    } else {
        horner((year - 2000) as f64 / 1000.0, &TABLE_27B[season.index()])
    }
}

/// Instant (TT Julian Day) of a season by Meeus' ch. 27 method: JDE₀ plus
/// `0.00001 S / Δλ` from Table 27.C. Valid for years −1000…+3000.
pub fn season_jde_meeus(year: i32, season: Season) -> f64 {
    let jde0 = mean_season_jde(year, season);
    let t = (jde0 - J2000) / DAYS_PER_CENTURY;
    let w = (35_999.373 * t - 2.47) * DEG;
    let dl = 1.0 + 0.0334 * w.cos() + 0.0007 * (2.0 * w).cos();
    let s: f64 = TABLE_27C
        .iter()
        .rev()
        .map(|r| r[0] * ((r[1] + r[2] * t) * DEG).cos())
        .sum();
    jde0 + 0.000_01 * s / dl
}

/// Instant (TT Julian Day) when the Sun's apparent longitude from `eph`
/// reaches the season's value, iterating Meeus eq. 27.1,
/// `JDE ← JDE + 58 sin(κ·90° − λ☉)`, from [`season_jde_meeus`] until the
/// step is below 10⁻⁷ day.
pub fn season_jde(eph: &dyn Ephemeris, year: i32, season: Season) -> f64 {
    let target = season.solar_longitude();
    let mut jde = season_jde_meeus(year, season);
    for _ in 0..20 {
        let lon = eph.sun_ecliptic(jde).lon;
        let step = 58.0 * norm_pi(target - lon).sin();
        jde += step;
        if step.abs() < 1e-7 {
            break;
        }
    }
    jde
}

/// The four seasons of a year from `eph` (TT Julian Days, calendar order).
pub fn seasons_of_year(eph: &dyn Ephemeris, year: i32) -> [f64; 4] {
    Season::ALL.map(|s| season_jde(eph, year, s))
}

/// The Sun's mean longitude L₀ referred to the mean equinox of date,
/// radians (Meeus eq. 28.2, τ in Julian millennia from J2000.0).
fn sun_mean_longitude(jd_tt: f64) -> f64 {
    let tau = centuries_since_j2000(jd_tt) / 10.0;
    let deg = horner(
        tau,
        &[
            280.466_456_7,
            360_007.698_277_9,
            0.030_320_28,
            1.0 / 49_931.0,
            -1.0 / 15_300.0,
            -1.0 / 2_000_000.0,
        ],
    );
    norm_2pi(deg.rem_euclid(360.0) * DEG)
}

/// Equation of time E (apparent minus mean solar time) at a TT Julian Day,
/// radians of hour angle (multiply by `720 / π` for minutes of time).
/// Meeus eq. 28.1, `E = L₀ − 0.0057183° − α + Δψ cos ε`, with the Sun's
/// apparent right ascension α taken from `eph`. Result in `(−π, π]`.
pub fn equation_of_time(eph: &dyn Ephemeris, jd_tt: f64) -> f64 {
    let nut = NutationObliquity::at(jd_tt);
    let alpha = eph.sun_equatorial(jd_tt).ra;
    norm_pi(sun_mean_longitude(jd_tt) - 0.005_718_3 * DEG - alpha + nut.dpsi * nut.eps.cos())
}

/// Equation of time by Smart's formula (Meeus eq. 28.3), radians. Less
/// accurate than [`equation_of_time`] (a few seconds of time) but needs no
/// ephemeris.
pub fn equation_of_time_smart(jd_tt: f64) -> f64 {
    let t = centuries_since_j2000(jd_tt);
    let y = (mean_obliquity(jd_tt) / 2.0).tan().powi(2);
    // Meeus eqs. 25.2, 25.3, 25.4.
    let l0 = (280.466_46 + t * (36_000.769_83 + 0.000_303_2 * t)) * DEG;
    let m = (357.529_11 + t * (35_999.050_29 - 0.000_153_7 * t)) * DEG;
    let e = 0.016_708_634 - t * (0.000_042_037 + 0.000_000_126_7 * t);
    y * (2.0 * l0).sin() - 2.0 * e * m.sin() + 4.0 * e * y * m.sin() * (2.0 * l0).cos()
        - 0.5 * y * y * (4.0 * l0).sin()
        - 1.25 * e * e * (2.0 * m).sin()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeus::MeeusEphemeris;
    use crate::time::julian_day;

    #[test]
    fn meeus_27a() {
        // June solstice 1962: JDE₀ = 2437837.38589, JDE = 2437837.39245
        // (1962 June 21, 21h25m08s TD).
        assert!((mean_season_jde(1962, Season::JuneSolstice) - 2_437_837.385_89).abs() < 1e-5);
        assert!((season_jde_meeus(1962, Season::JuneSolstice) - 2_437_837.392_45).abs() < 1e-5);
    }

    /// Meeus ch. 27: the table of instants for 1996–2005 computed with the
    /// complete VSOP87 theory, TD.
    /// `(year, [(day, h, m, s); 4])` for March, June, September, December.
    /// Day, hour, minute, second (TD).
    type Dhms = (u32, u32, u32, u32);

    #[rustfmt::skip]
    const VSOP87_SEASONS: [(i32, [Dhms; 4]); 10] = [
        (1996, [(20, 8, 4, 7), (21, 2, 24, 46), (22, 18, 1, 8), (21, 14, 6, 56)]),
        (1997, [(20, 13, 55, 42), (21, 8, 20, 59), (22, 23, 56, 49), (21, 20, 8, 5)]),
        (1998, [(20, 19, 55, 35), (21, 14, 3, 38), (23, 5, 38, 15), (22, 1, 57, 31)]),
        (1999, [(21, 1, 46, 53), (21, 19, 50, 11), (23, 11, 32, 34), (22, 7, 44, 52)]),
        (2000, [(20, 7, 36, 19), (21, 1, 48, 46), (22, 17, 28, 40), (21, 13, 38, 30)]),
        (2001, [(20, 13, 31, 47), (21, 7, 38, 48), (22, 23, 5, 32), (21, 19, 22, 34)]),
        (2002, [(20, 19, 17, 13), (21, 13, 25, 29), (23, 4, 56, 28), (22, 1, 15, 26)]),
        (2003, [(21, 1, 0, 50), (21, 19, 11, 32), (23, 10, 47, 53), (22, 7, 4, 53)]),
        (2004, [(20, 6, 49, 42), (21, 0, 57, 57), (22, 16, 30, 54), (21, 12, 42, 40)]),
        (2005, [(20, 12, 34, 29), (21, 6, 47, 12), (22, 22, 24, 14), (21, 18, 36, 1)]),
    ];

    #[test]
    fn meeus_table_vsop87_seasons() {
        let eph = MeeusEphemeris::with_sun_series(crate::meeus::SunSeries::Full);
        let mut worst_meeus: f64 = 0.0;
        let mut worst_eph: f64 = 0.0;
        for (year, events) in VSOP87_SEASONS {
            for (i, (d, h, m, s)) in events.iter().enumerate() {
                let month = 3 + 3 * i as u32;
                let expected = julian_day(year, month, *d as f64)
                    + (*h as f64 * 3600.0 + *m as f64 * 60.0 + *s as f64) / 86_400.0;
                let season = Season::ALL[i];
                let a = (season_jde_meeus(year, season) - expected).abs() * 86_400.0;
                let b = (season_jde(&eph, year, season) - expected).abs() * 86_400.0;
                worst_meeus = worst_meeus.max(a);
                worst_eph = worst_eph.max(b);
            }
        }
        // Meeus: the ch. 27 method errs by < 51 s over 1951–2050.
        assert!(worst_meeus < 51.0, "Meeus method {worst_meeus} s");
        // The ephemeris iteration matches the complete VSOP87 to ~1 s.
        assert!(worst_eph < 2.0, "ephemeris method {worst_eph} s");
        let all = seasons_of_year(&eph, 2000);
        assert!(all.windows(2).all(|w| w[1] > w[0] + 85.0));
    }

    #[test]
    fn meeus_28a() {
        // 1992 October 13.0 TD: E = +3.427351° = +13m42.6s.
        let e = equation_of_time(&MeeusEphemeris::new(), 2_448_908.5);
        assert!((e / DEG - 3.427_351).abs() < 1e-4, "{}", e / DEG);
        let minutes = e / DEG * 4.0;
        assert!((minutes * 60.0 - (13.0 * 60.0 + 42.6)).abs() < 0.1);
    }

    #[test]
    fn meeus_28b() {
        // Smart's formula, same date: E = 0.059825 rad = +13m42.7s.
        let e = equation_of_time_smart(2_448_908.5);
        assert!((e - 0.059_825).abs() < 2e-6, "{e}");
        // And it agrees with the rigorous value to a few seconds all year.
        let eph = MeeusEphemeris::new();
        for i in 0..36 {
            let jd = 2_460_310.5 + 10.0 * i as f64;
            let d = (equation_of_time(&eph, jd) - equation_of_time_smart(jd)) / DEG * 240.0;
            assert!(d.abs() < 10.0, "{jd}: {d} s");
        }
    }
}
