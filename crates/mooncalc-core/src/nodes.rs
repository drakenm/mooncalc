//! Passages of the Moon through the nodes of its orbit and the Moon's
//! maximum declinations.
//!
//! * [`node_passage_meeus`]: Meeus, *Astronomical Algorithms* 2nd ed.,
//!   ch. 51 (mean passage plus periodic terms; agrees with the refined
//!   instants to a few minutes).
//! * [`max_declination_meeus`]: ch. 52 (Tables 52.A and 52.B). Against the
//!   refined extremes it agrees to < 10 min and < 25″ over 2000–2022,
//!   but deviates by up to ~40 min and ~100″ in some later years (e.g.
//!   around 2030 and 2049); use the refinement when precision matters.
//! * [`refine_node_passage`], [`refine_max_declination`]: ephemeris-agnostic
//!   refinements — the zero of the Moon's apparent ecliptic latitude, and
//!   the extremum of its apparent declination, from any [`Ephemeris`].

use crate::angle::DEG;
use crate::ephemeris::Ephemeris;
use crate::search;

/// Which node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// The Moon crosses the ecliptic going north.
    Ascending,
    /// Going south.
    Descending,
}

/// A passage of the Moon through a node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodePassage {
    pub kind: NodeKind,
    /// Instant, TT Julian Day.
    pub jd_tt: f64,
}

/// North or south.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeclinationExtreme {
    North,
    South,
}

/// A maximum (north) or minimum (south) of the Moon's declination.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaxDeclination {
    pub kind: DeclinationExtreme,
    /// Instant, TT Julian Day.
    pub jd_tt: f64,
    /// Geocentric apparent declination, radians (negative for south).
    pub dec: f64,
}

fn horner(x: f64, c: &[f64]) -> f64 {
    c.iter().rev().fold(0.0, |acc, &k| acc * x + k)
}

/// Node number nearest a TT Julian Day: an integer for an ascending node,
/// integer + 0.5 for a descending one (Meeus eq. 51.1 inverted; k = 0 at
/// the ascending node of 2000 January 21).
pub fn node_number_near(jd_tt: f64, kind: NodeKind) -> f64 {
    let k = (jd_tt - 2_451_565.161_9) / 27.212_220_817;
    match kind {
        NodeKind::Ascending => k.round(),
        NodeKind::Descending => (k - 0.5).round() + 0.5,
    }
}

/// Node passage number `k` by Meeus ch. 51 (integer: ascending node;
/// integer + 0.5: descending node).
pub fn node_passage_meeus(k: f64) -> NodePassage {
    let t = k / 1342.23;
    let d = (183.638
        + 331.737_356_82 * k
        + t * t * horner(t, &[0.001_485_2, 0.000_002_09, -0.000_000_01]))
        * DEG;
    let m = (17.4006 + 26.820_372_5 * k + t * t * horner(t, &[0.000_118_6, 0.000_000_06])) * DEG;
    let mp = (38.3776
        + 355.527_473_13 * k
        + t * t * horner(t, &[0.012_349_9, 0.000_014_627, -0.000_000_069]))
        * DEG;
    let om = (123.9767 - 1.440_989_56 * k
        + t * t * horner(t, &[0.002_060_8, 0.000_002_14, -0.000_000_016]))
        * DEG;
    let v = (299.75 + t * (132.85 - 0.009_173 * t)) * DEG;
    let p = om + (272.75 - 2.3 * t) * DEG;
    let e = 1.0 - t * (0.002_516 + 0.000_007_4 * t);
    let jde = 2_451_565.161_9
        + 27.212_220_817 * k
        + t * t * horner(t, &[0.000_276_2, 0.000_000_021, -0.000_000_000_088])
        - 0.4721 * mp.sin()
        - 0.1649 * (2.0 * d).sin()
        - 0.0868 * (2.0 * d - mp).sin()
        + 0.0084 * (2.0 * d + mp).sin()
        - 0.0083 * e * (2.0 * d - m).sin()
        - 0.0039 * e * (2.0 * d - m - mp).sin()
        + 0.0034 * (2.0 * mp).sin()
        - 0.0031 * (2.0 * d - 2.0 * mp).sin()
        + 0.0030 * e * (2.0 * d + m).sin()
        + 0.0028 * e * (m - mp).sin()
        + 0.0026 * e * m.sin()
        + 0.0025 * (4.0 * d).sin()
        + 0.0024 * d.sin()
        + 0.0022 * e * (m + mp).sin()
        + 0.0017 * om.sin()
        + 0.0014 * (4.0 * d - mp).sin()
        + 0.0005 * e * (2.0 * d + m - mp).sin()
        + 0.0004 * e * (2.0 * d - m + mp).sin()
        - 0.0003 * e * (2.0 * d - 2.0 * m).sin()
        + 0.0003 * e * (4.0 * d - m).sin()
        + 0.0003 * v.sin()
        + 0.0003 * p.sin();
    let kind = if k.rem_euclid(1.0) < 0.25 {
        NodeKind::Ascending
    } else {
        NodeKind::Descending
    };
    NodePassage { kind, jd_tt: jde }
}

/// Refine a node passage: the zero of the Moon's apparent ecliptic
/// latitude from `eph` within ±0.5 day of `approx`, to ~0.01 s. Returns
/// `approx` unchanged if no sign change is bracketed.
pub fn refine_node_passage(eph: &dyn Ephemeris, approx: &NodePassage) -> NodePassage {
    let lat = |jd: f64| eph.moon_ecliptic(jd).lat;
    let jd_tt = search::find_root(lat, approx.jd_tt - 0.5, approx.jd_tt + 0.5, 1e-7)
        .unwrap_or(approx.jd_tt);
    NodePassage {
        kind: approx.kind,
        jd_tt,
    }
}

/// The first node passage strictly after `jd_tt` (of `kind`, or either if
/// `None`), refined against `eph` when given.
pub fn next_node_passage(
    eph: Option<&dyn Ephemeris>,
    jd_tt: f64,
    kind: Option<NodeKind>,
) -> NodePassage {
    let step = if kind.is_some() { 1.0 } else { 0.5 };
    let mut k = node_number_near(jd_tt, kind.unwrap_or(NodeKind::Ascending)) - 2.0 * step;
    loop {
        let n = node(eph, k);
        if n.jd_tt > jd_tt {
            return n;
        }
        k += step;
    }
}

/// Every node passage (both kinds, chronological) with
/// `jd_from <= jd_tt < jd_to`.
pub fn node_passages_between(
    eph: Option<&dyn Ephemeris>,
    jd_from: f64,
    jd_to: f64,
) -> Vec<NodePassage> {
    let mut out = Vec::new();
    let mut k = node_number_near(jd_from, NodeKind::Ascending) - 1.0;
    let k_end = node_number_near(jd_to, NodeKind::Ascending) + 1.0;
    while k <= k_end {
        let n = node(eph, k);
        if n.jd_tt >= jd_from && n.jd_tt < jd_to {
            out.push(n);
        }
        k += 0.5;
    }
    out
}

fn node(eph: Option<&dyn Ephemeris>, k: f64) -> NodePassage {
    let n = node_passage_meeus(k);
    match eph {
        Some(e) => refine_node_passage(e, &n),
        None => n,
    }
}

/// Constants of Meeus ch. 52 for one extreme: D, M, M′, F at k = 0
/// (degrees), the JDE constant, the 44 time coefficients (Table 52.A) and
/// the 37 declination coefficients (Table 52.B, degrees).
struct MaxDecConstants {
    d: f64,
    m: f64,
    mp: f64,
    f: f64,
    jde: f64,
    time: [f64; 44],
    dec: [f64; 37],
}

#[rustfmt::skip]
const NORTH: MaxDecConstants = MaxDecConstants {
    d: 152.2029, m: 14.8591, mp: 4.6881, f: 325.8867, jde: 2_451_562.589_7,
    time: [
        0.8975, -0.4726, -0.1030, -0.0976, -0.0462, -0.0461, -0.0438, 0.0162, -0.0157, 0.0145,
        0.0136, -0.0095, -0.0091, -0.0089, 0.0075, -0.0068, 0.0061, -0.0047, -0.0043, -0.0040,
        -0.0037, 0.0031, 0.0030, -0.0029, -0.0029, -0.0027, 0.0024, -0.0021, 0.0019, 0.0018,
        0.0018, 0.0017, 0.0017, -0.0014, 0.0013, 0.0013, 0.0012, 0.0011, -0.0011, 0.0010,
        0.0010, -0.0009, 0.0007, -0.0007,
    ],
    dec: [
        5.1093, 0.2658, 0.1448, -0.0322, 0.0133, 0.0125, -0.0124, -0.0101, 0.0097, -0.0087,
        0.0074, 0.0067, 0.0063, 0.0060, -0.0057, -0.0056, 0.0052, 0.0041, -0.0040, 0.0038,
        -0.0034, -0.0029, 0.0029, -0.0028, -0.0028, -0.0023, -0.0021, 0.0019, 0.0018, 0.0017,
        0.0015, 0.0014, -0.0012, -0.0012, -0.0010, -0.0010, 0.0006,
    ],
};

#[rustfmt::skip]
const SOUTH: MaxDecConstants = MaxDecConstants {
    d: 345.6676, m: 1.3951, mp: 186.2100, f: 145.1633, jde: 2_451_548.928_9,
    time: [
        -0.8975, -0.4726, -0.1030, -0.0976, 0.0541, 0.0516, -0.0438, 0.0112, 0.0157, 0.0023,
        -0.0136, 0.0110, 0.0091, 0.0089, 0.0075, -0.0030, -0.0061, -0.0047, -0.0043, 0.0040,
        -0.0037, -0.0031, 0.0030, 0.0029, -0.0029, -0.0027, 0.0024, -0.0021, -0.0019, -0.0006,
        -0.0018, -0.0017, 0.0017, 0.0014, -0.0013, -0.0013, 0.0012, 0.0011, 0.0011, 0.0010,
        0.0010, -0.0009, -0.0007, -0.0007,
    ],
    dec: [
        -5.1093, 0.2658, -0.1448, 0.0322, 0.0133, 0.0125, -0.0015, 0.0101, -0.0097, 0.0087,
        0.0074, 0.0067, -0.0063, -0.0060, 0.0057, -0.0056, -0.0052, -0.0041, -0.0040, -0.0038,
        0.0034, -0.0029, 0.0029, 0.0028, -0.0028, 0.0023, 0.0021, 0.0019, 0.0018, -0.0017,
        0.0015, 0.0014, 0.0012, -0.0012, 0.0010, -0.0010, 0.0037,
    ],
};

/// Number of the declination extreme nearest a TT Julian Day (Meeus
/// eq. 52.1 inverted; always an integer).
pub fn max_declination_number_near(jd_tt: f64, kind: DeclinationExtreme) -> f64 {
    let c = match kind {
        DeclinationExtreme::North => &NORTH,
        DeclinationExtreme::South => &SOUTH,
    };
    ((jd_tt - c.jde) / 27.321_582_247).round()
}

/// Greatest northern or southern declination number `k` by Meeus ch. 52.
pub fn max_declination_meeus(k: f64, kind: DeclinationExtreme) -> MaxDeclination {
    let c = match kind {
        DeclinationExtreme::North => &NORTH,
        DeclinationExtreme::South => &SOUTH,
    };
    let t = k / 1336.86;
    let d = (c.d + 333.070_554_6 * k + t * t * (-0.000_421_4 + 0.000_000_11 * t)) * DEG;
    let m = (c.m + 26.928_159_2 * k + t * t * (-0.000_035_5 - 0.000_000_1 * t)) * DEG;
    let mp = (c.mp + 356.956_279_4 * k + t * t * (0.010_306_6 + 0.000_012_51 * t)) * DEG;
    let f = (c.f + 1.446_780_7 * k + t * t * (-0.002_069 - 0.000_002_15 * t)) * DEG;
    let e = 1.0 - t * (0.002_516 + 0.000_007_4 * t);
    let (s, co) = (f64::sin, f64::cos);
    // Table 52.A arguments, in the book's order.
    let targ = [
        co(f),
        s(mp),
        s(2.0 * f),
        s(2.0 * d - mp),
        co(mp - f),
        co(mp + f),
        s(2.0 * d),
        e * s(m),
        co(3.0 * f),
        s(mp + 2.0 * f),
        co(2.0 * d - f),
        co(2.0 * d - mp - f),
        co(2.0 * d - mp + f),
        co(2.0 * d + f),
        s(2.0 * mp),
        s(mp - 2.0 * f),
        co(2.0 * mp - f),
        s(mp + 3.0 * f),
        e * s(2.0 * d - m - mp),
        co(mp - 2.0 * f),
        s(2.0 * d - 2.0 * mp),
        s(f),
        s(2.0 * d + mp),
        co(mp + 2.0 * f),
        e * s(2.0 * d - m),
        s(mp + f),
        e * s(m - mp),
        s(mp - 3.0 * f),
        s(2.0 * mp + f),
        co(2.0 * d - 2.0 * mp - f),
        s(3.0 * f),
        co(mp + 3.0 * f),
        co(2.0 * mp),
        co(2.0 * d - mp),
        co(2.0 * d + mp + f),
        co(mp),
        s(3.0 * mp + f),
        s(2.0 * d - mp + f),
        co(2.0 * d - 2.0 * mp),
        co(d + f),
        e * s(m + mp),
        s(2.0 * d - 2.0 * f),
        co(2.0 * mp + f),
        co(3.0 * mp + f),
    ];
    // Table 52.B arguments.
    let darg = [
        s(f),
        co(2.0 * f),
        s(2.0 * d - f),
        s(3.0 * f),
        co(2.0 * d - 2.0 * f),
        co(2.0 * d),
        s(mp - f),
        s(mp + 2.0 * f),
        co(f),
        e * s(2.0 * d + m - f),
        s(mp + 3.0 * f),
        s(d + f),
        s(mp - 2.0 * f),
        e * s(2.0 * d - m - f),
        s(2.0 * d - mp - f),
        co(mp + f),
        co(mp + 2.0 * f),
        co(2.0 * mp + f),
        co(mp - 3.0 * f),
        co(2.0 * mp - f),
        co(mp - 2.0 * f),
        s(2.0 * mp),
        s(3.0 * mp + f),
        e * co(2.0 * d + m - f),
        co(mp - f),
        co(3.0 * f),
        s(2.0 * d + f),
        co(mp + 3.0 * f),
        co(d + f),
        s(2.0 * mp - f),
        co(3.0 * mp + f),
        co(2.0 * d + 2.0 * mp + f),
        s(2.0 * d - 2.0 * mp - f),
        co(2.0 * mp),
        co(mp),
        s(2.0 * f),
        s(mp + f),
    ];
    let jde = c.jde
        + 27.321_582_247 * k
        + t * t * (0.000_119_804 - 0.000_000_141 * t)
        + c.time.iter().zip(targ).map(|(a, x)| a * x).sum::<f64>();
    let dec_deg = 23.6961 - 0.013_004 * t + c.dec.iter().zip(darg).map(|(a, x)| a * x).sum::<f64>();
    let sign = match kind {
        DeclinationExtreme::North => 1.0,
        DeclinationExtreme::South => -1.0,
    };
    MaxDeclination {
        kind,
        jd_tt: jde,
        dec: sign * dec_deg * DEG,
    }
}

/// Refine a declination extreme: the maximum (north) or minimum (south) of
/// the Moon's apparent declination from `eph` within ±1 day of `approx`.
pub fn refine_max_declination(eph: &dyn Ephemeris, approx: &MaxDeclination) -> MaxDeclination {
    let dec = |jd: f64| eph.moon_equatorial(jd).dec;
    let (a, b, tol) = (approx.jd_tt - 1.0, approx.jd_tt + 1.0, 1e-6);
    let (jd_tt, dec) = match approx.kind {
        DeclinationExtreme::North => search::maximize(dec, a, b, tol),
        DeclinationExtreme::South => search::minimize(dec, a, b, tol),
    };
    MaxDeclination {
        kind: approx.kind,
        jd_tt,
        dec,
    }
}

/// Every declination extreme (both kinds, chronological) with
/// `jd_from <= jd_tt < jd_to`, refined against `eph` when given.
pub fn max_declinations_between(
    eph: Option<&dyn Ephemeris>,
    jd_from: f64,
    jd_to: f64,
) -> Vec<MaxDeclination> {
    let mut out = Vec::new();
    for kind in [DeclinationExtreme::North, DeclinationExtreme::South] {
        let mut k = max_declination_number_near(jd_from, kind) - 1.0;
        let k_end = max_declination_number_near(jd_to, kind) + 1.0;
        while k <= k_end {
            let mut x = max_declination_meeus(k, kind);
            if let Some(e) = eph {
                x = refine_max_declination(e, &x);
            }
            if x.jd_tt >= jd_from && x.jd_tt < jd_to {
                out.push(x);
            }
            k += 1.0;
        }
    }
    out.sort_by(|a, b| a.jd_tt.total_cmp(&b.jd_tt));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeus::MeeusEphemeris;
    use crate::time::julian_day;

    #[test]
    fn node_search_ends_outside_the_ephemeris_range() {
        // Refining against NaN positions used to yield a NaN instant, which
        // never compares > the start, so next/previous looped forever. They
        // now fall back to the Meeus ch. 51 instant.
        let t = julian_day(2024, 3, 11.0);
        let eph = crate::test_support::Clipped::new(t - 1.0, t + 1.0);
        let next = next_node_passage(Some(&eph), t, None);
        assert!(next.jd_tt.is_finite() && next.jd_tt > t, "{next:?}");
        let k = node_number_near(next.jd_tt, next.kind);
        assert_eq!(next, node_passage_meeus(k));
        let all = node_passages_between(Some(&eph), t - 40.0, t + 40.0);
        assert!(all.iter().all(|n| n.jd_tt.is_finite()));
    }

    #[test]
    fn meeus_51a() {
        // Ascending node of 1987 May: k = −170, JDE = 2446938.76803
        // (1987 May 23, 6h26m TD).
        assert_eq!(
            node_number_near(julian_day(1987, 5, 15.0), NodeKind::Ascending),
            -170.0
        );
        let n = node_passage_meeus(-170.0);
        assert_eq!(n.kind, NodeKind::Ascending);
        assert!((n.jd_tt - 2_446_938.768_03).abs() < 1e-5, "{}", n.jd_tt);
        // The ephemeris refinement agrees to a few minutes.
        let r = refine_node_passage(&MeeusEphemeris::new(), &n);
        assert!(
            (r.jd_tt - n.jd_tt).abs() * 1440.0 < 5.0,
            "{}",
            (r.jd_tt - n.jd_tt) * 1440.0
        );
    }

    #[test]
    fn meeus_52abc() {
        // 52.a: k = −148, northern extreme JDE = 2447518.3346
        // (1988 Dec 22, 20h02m TD), δ = +28.1562°.
        let n = max_declination_meeus(-148.0, DeclinationExtreme::North);
        assert!((n.jd_tt - 2_447_518.334_6).abs() < 1e-4, "{}", n.jd_tt);
        assert!((n.dec / DEG - 28.1562).abs() < 1e-4, "{}", n.dec / DEG);
        assert_eq!(
            max_declination_number_near(julian_day(1988, 12, 20.0), DeclinationExtreme::North),
            -148.0
        );
        // 52.b: k = 659, southern extreme JDE = 2469553.0834
        // (2049 April 21, 14h TD), δ = −22.1384°.
        let s = max_declination_meeus(659.0, DeclinationExtreme::South);
        assert!((s.jd_tt - 2_469_553.083_4).abs() < 1e-4, "{}", s.jd_tt);
        assert!((s.dec / DEG + 22.1384).abs() < 1e-4, "{}", s.dec / DEG);
        // 52.c: k = −26788, northern extreme JDE = 1719672.1412
        // (−4 March 16, 15h TD), δ = +28.9739°.
        let c = max_declination_meeus(-26_788.0, DeclinationExtreme::North);
        assert!((c.jd_tt - 1_719_672.141_2).abs() < 1e-4, "{}", c.jd_tt);
        assert!((c.dec / DEG - 28.9739).abs() < 1e-4, "{}", c.dec / DEG);
        // Refinement against the default ephemeris over 2000–2022: the
        // series agrees to < 10 min and < 25″ (typically 3 min, 5″). (Further out the ch. 52
        // series deviates more, by up to ~100″ and ~40 min around 2049;
        // two independent transcriptions of Tables 52.A/B agree with ours,
        // so this is the series' own error. The instant of an extreme is
        // ill-conditioned: δ changes by only ~0.0002° within ±20 min.)
        let eph = MeeusEphemeris::new();
        for kind in [DeclinationExtreme::North, DeclinationExtreme::South] {
            for k in 0..300 {
                let x = max_declination_meeus(k as f64, kind);
                let r = refine_max_declination(&eph, &x);
                assert!((r.jd_tt - x.jd_tt).abs() * 1440.0 < 10.0, "{kind:?} {k}");
                assert!((r.dec - x.dec).abs() / DEG * 3600.0 < 25.0, "{kind:?} {k}");
            }
        }
    }

    #[test]
    fn search_api() {
        let eph = MeeusEphemeris::new();
        let (a, b) = (julian_day(2024, 1, 1.0), julian_day(2025, 1, 1.0));
        let nodes = node_passages_between(Some(&eph), a, b);
        assert!(nodes.len() == 26 || nodes.len() == 27);
        assert!(nodes.windows(2).all(|w| w[0].kind != w[1].kind));
        for n in &nodes {
            assert!(eph.moon_ecliptic(n.jd_tt).lat.abs() < 1e-9);
            let rising = eph.moon_ecliptic(n.jd_tt + 0.01).lat > 0.0;
            assert_eq!(rising, n.kind == NodeKind::Ascending);
        }
        let next = next_node_passage(None, a, Some(NodeKind::Descending));
        assert!(next.jd_tt > a && next.jd_tt - a < 28.0 && next.kind == NodeKind::Descending);
        let ext = max_declinations_between(Some(&eph), a, b);
        assert!(ext.len() == 26 || ext.len() == 27);
        assert!(ext.windows(2).all(|w| w[0].kind != w[1].kind));
        // 2024–2025 is a major lunar standstill: extremes reach ~±28.5°.
        assert!(
            ext.iter()
                .all(|x| x.dec.abs() / DEG > 27.5 && x.dec.abs() / DEG < 29.0)
        );
    }
}
