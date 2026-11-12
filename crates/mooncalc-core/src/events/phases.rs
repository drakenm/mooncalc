//! Lunar phases and the Moon's age.
//!
//! A phase is the geocentric instant at which the apparent ecliptic
//! longitude of the Moon minus that of the Sun, `λ☾ − λ☉`, equals 0° (new
//! Moon, the conjunction), 90° (first quarter), 180° (full) or 270° (last
//! quarter). This is the definition used by the Astronomical Almanac and
//! USNO.
//!
//! * [`meeus_phase_jde`] is the closed-form series of Meeus, *Astronomical
//!   Algorithms* 2nd ed., ch. 49 (mean phase, periodic terms and the 14
//!   planetary arguments), accurate to a few seconds near the present
//!   against ELP-2000/82. It is the first guess.
//! * [`phase_near`] refines that guess against any [`Ephemeris`] by secant
//!   iteration on `λ☾ − λ☉`, so the result is consistent with the backend
//!   in use.

use std::f64::consts::TAU;

use crate::angle::{DEG, norm_pi};
use crate::ephemeris::Ephemeris;
use crate::time::Instant;

/// Mean synodic month, days (Meeus eq. 49.1).
pub const SYNODIC_MONTH_DAYS: f64 = 29.530_588_861;

/// The four principal phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    New,
    FirstQuarter,
    Full,
    LastQuarter,
}

impl Phase {
    /// All four, in order through a lunation.
    pub const ALL: [Self; 4] = [Self::New, Self::FirstQuarter, Self::Full, Self::LastQuarter];

    /// Target `λ☾ − λ☉`, radians.
    pub fn elongation(self) -> f64 {
        self.fraction() * TAU
    }

    /// Fraction of a lunation: 0, ¼, ½, ¾ (the fractional part of Meeus' k).
    pub fn fraction(self) -> f64 {
        match self {
            Self::New => 0.0,
            Self::FirstQuarter => 0.25,
            Self::Full => 0.5,
            Self::LastQuarter => 0.75,
        }
    }
}

/// `λ☾ − λ☉` (apparent, geocentric) at a TT Julian Day, radians in `(−π, π]`.
pub fn elongation_in_longitude(eph: &(impl Ephemeris + ?Sized), jd_tt: f64) -> f64 {
    norm_pi(eph.moon_ecliptic(jd_tt).lon - eph.sun_ecliptic(jd_tt).lon)
}

/// Meeus ch. 49: the JDE (TT) of the phase with lunation number `k`, where
/// `k` is an integer for new Moon plus 0.25, 0.5 or 0.75 for the other
/// phases and `k = 0` is the new Moon of 2000 January 6.
pub fn meeus_phase_jde(k: f64) -> f64 {
    let t = k / 1236.85;
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    let mut jde = 2_451_550.097_66 + SYNODIC_MONTH_DAYS * k + 0.000_154_37 * t2
        - 0.000_000_150 * t3
        + 0.000_000_000_73 * t4;
    let e = 1.0 - 0.002_516 * t - 0.000_007_4 * t2;
    let m = (2.5534 + 29.105_356_70 * k - 0.000_001_4 * t2 - 0.000_000_11 * t3) * DEG;
    let mp = (201.5643 + 385.816_935_28 * k + 0.010_758_2 * t2 + 0.000_012_38 * t3
        - 0.000_000_058 * t4)
        * DEG;
    let f = (160.7108 + 390.670_502_84 * k - 0.001_611_8 * t2 - 0.000_002_27 * t3
        + 0.000_000_011 * t4)
        * DEG;
    let om = (124.7746 - 1.563_755_88 * k + 0.002_067_2 * t2 + 0.000_002_15 * t3) * DEG;
    let s = f64::sin;
    let frac = k - k.floor();
    let quarter = (frac - 0.25).abs() < 1e-6 || (frac - 0.75).abs() < 1e-6;
    jde += if quarter {
        let mut c = -0.62801 * s(mp) + 0.17172 * e * s(m) - 0.01183 * e * s(mp + m)
            + 0.00862 * s(2.0 * mp)
            + 0.00804 * s(2.0 * f)
            + 0.00454 * e * s(mp - m)
            + 0.00204 * e * e * s(2.0 * m)
            - 0.00180 * s(mp - 2.0 * f)
            - 0.00070 * s(mp + 2.0 * f)
            - 0.00040 * s(3.0 * mp)
            - 0.00034 * e * s(2.0 * mp - m)
            + 0.00032 * e * s(m + 2.0 * f)
            + 0.00032 * e * s(m - 2.0 * f)
            - 0.00028 * e * e * s(mp + 2.0 * m)
            + 0.00027 * e * s(2.0 * mp + m)
            - 0.00017 * s(om)
            - 0.00005 * s(mp - m - 2.0 * f)
            + 0.00004 * s(2.0 * mp + 2.0 * f)
            - 0.00004 * s(mp + m + 2.0 * f)
            + 0.00004 * s(mp - 2.0 * m)
            + 0.00003 * s(mp + m - 2.0 * f)
            + 0.00003 * s(3.0 * m)
            + 0.00002 * s(2.0 * mp - 2.0 * f)
            + 0.00002 * s(mp - m + 2.0 * f)
            - 0.00002 * s(3.0 * mp + m);
        let w = 0.00306 - 0.00038 * e * m.cos() + 0.00026 * mp.cos() - 0.00002 * (mp - m).cos()
            + 0.00002 * (mp + m).cos()
            + 0.00002 * (2.0 * f).cos();
        c += if frac < 0.5 { w } else { -w };
        c
    } else {
        let full = (frac - 0.5).abs() < 1e-6;
        let (c1, c2, c3, c4, c5, c6, c7) = if full {
            (
                -0.40614, 0.17302, 0.01614, 0.01043, 0.00734, -0.00515, 0.00209,
            )
        } else {
            (
                -0.40720, 0.17241, 0.01608, 0.01039, 0.00739, -0.00514, 0.00208,
            )
        };
        c1 * s(mp)
            + c2 * e * s(m)
            + c3 * s(2.0 * mp)
            + c4 * s(2.0 * f)
            + c5 * e * s(mp - m)
            + c6 * e * s(mp + m)
            + c7 * e * e * s(2.0 * m)
            - 0.00111 * s(mp - 2.0 * f)
            - 0.00057 * s(mp + 2.0 * f)
            + 0.00056 * e * s(2.0 * mp + m)
            - 0.00042 * s(3.0 * mp)
            + 0.00042 * e * s(m + 2.0 * f)
            + 0.00038 * e * s(m - 2.0 * f)
            - 0.00024 * e * s(2.0 * mp - m)
            - 0.00017 * s(om)
            - 0.00007 * s(mp + 2.0 * m)
            + 0.00004 * s(2.0 * mp - 2.0 * f)
            + 0.00004 * s(3.0 * m)
            + 0.00003 * s(mp + m - 2.0 * f)
            + 0.00003 * s(2.0 * mp + 2.0 * f)
            - 0.00003 * s(mp + m + 2.0 * f)
            + 0.00003 * s(mp - m + 2.0 * f)
            - 0.00002 * s(mp - m - 2.0 * f)
            - 0.00002 * s(3.0 * mp + m)
            + 0.00002 * s(4.0 * mp)
    };
    // Additional corrections for all phases (planetary arguments A1–A14).
    const A: [(f64, f64, f64); 14] = [
        (0.000_325, 299.77, 0.107_408),
        (0.000_165, 251.88, 0.016_321),
        (0.000_164, 251.83, 26.651_886),
        (0.000_126, 349.42, 36.412_478),
        (0.000_110, 84.66, 18.206_239),
        (0.000_062, 141.74, 53.303_771),
        (0.000_060, 207.14, 2.453_732),
        (0.000_056, 154.84, 7.306_860),
        (0.000_047, 34.52, 27.261_239),
        (0.000_042, 207.19, 0.121_824),
        (0.000_040, 291.34, 1.844_379),
        (0.000_037, 161.72, 24.198_154),
        (0.000_035, 239.56, 25.513_099),
        (0.000_023, 331.55, 3.592_518),
    ];
    for (i, &(amp, a0, a1)) in A.iter().enumerate() {
        let mut arg = a0 + a1 * k;
        if i == 0 {
            arg -= 0.009_173 * t2;
        }
        jde += amp * (arg * DEG).sin();
    }
    jde
}

/// Meeus' lunation number of `phase` nearest to TT Julian Day `jd_tt`.
pub fn lunation_near(phase: Phase, jd_tt: f64) -> f64 {
    ((jd_tt - 2_451_550.097_66) / SYNODIC_MONTH_DAYS - phase.fraction()).round() + phase.fraction()
}

/// The instant of `phase` nearest to `jd_tt` (TT), refined against `eph`.
/// Returns the TT Julian Day, converged to about 0.01 s.
pub fn phase_near_tt(eph: &(impl Ephemeris + ?Sized), phase: Phase, jd_tt: f64) -> f64 {
    let target = phase.elongation();
    let f = |t: f64| norm_pi(elongation_in_longitude(eph, t) - target);
    let mut t0 = meeus_phase_jde(lunation_near(phase, jd_tt));
    let mut f0 = f(t0);
    // Mean rate of λ☾ − λ☉ ≈ 12.19°/day; the secant takes over after one step.
    let mut t1 = t0 - f0 / (TAU / SYNODIC_MONTH_DAYS);
    for _ in 0..20 {
        let f1 = f(t1);
        if f1 == f0 {
            break;
        }
        let t2 = t1 - f1 * (t1 - t0) / (f1 - f0);
        (t0, f0, t1) = (t1, f1, t2);
        if (t1 - t0).abs() < 1e-7 {
            break;
        }
    }
    t1
}

/// The instant of `phase` nearest to `t`, refined against `eph`.
pub fn phase_near(eph: &(impl Ephemeris + ?Sized), phase: Phase, t: Instant) -> Instant {
    Instant::from_jd_tt(phase_near_tt(eph, phase, t.jd_tt()))
}

/// The first `phase` strictly after `t`.
pub fn next_phase(eph: &(impl Ephemeris + ?Sized), phase: Phase, t: Instant) -> Instant {
    let mut p = phase_near(eph, phase, t);
    if p.jd_ut <= t.jd_ut {
        p = phase_near(eph, phase, p.add_days(SYNODIC_MONTH_DAYS));
    }
    p
}

/// The last `phase` at or before `t`.
pub fn previous_phase(eph: &(impl Ephemeris + ?Sized), phase: Phase, t: Instant) -> Instant {
    let mut p = phase_near(eph, phase, t);
    if p.jd_ut > t.jd_ut {
        p = phase_near(eph, phase, p.add_days(-SYNODIC_MONTH_DAYS));
    }
    p
}

/// Every principal phase in `[t0, t1)`, in time order.
///
/// Phases where `eph` returns NaN (outside its valid range) are left out.
pub fn phases_between(
    eph: &(impl Ephemeris + ?Sized),
    t0: Instant,
    t1: Instant,
) -> Vec<(Phase, Instant)> {
    let mut out = Vec::new();
    let k0 = ((t0.jd_tt() - 2_451_550.097_66) / SYNODIC_MONTH_DAYS).floor() - 1.0;
    let mut k = k0;
    loop {
        for phase in Phase::ALL {
            let jde = meeus_phase_jde(k + phase.fraction());
            let t = Instant::from_jd_tt(phase_near_tt(eph, phase, jde));
            if t.jd_ut.is_nan() {
                // Outside the ephemeris range. A NaN never compares `>=`
                // the end, so stop on the Meeus estimate instead (within
                // hours of the true phase).
                if jde > t1.jd_tt() + 1.0 {
                    return out;
                }
                continue;
            }
            if t.jd_ut >= t1.jd_ut {
                return out;
            }
            if t.jd_ut >= t0.jd_ut {
                out.push((phase, t));
            }
        }
        k += 1.0;
    }
}

/// The new Moon (geocentric conjunction in longitude) nearest to `t`.
pub fn nearest_new_moon(eph: &(impl Ephemeris + ?Sized), t: Instant) -> Instant {
    phase_near(eph, Phase::New, t)
}

/// The Moon's age at `t`: days since the last new Moon at or before `t`.
pub fn moon_age_days(eph: &(impl Ephemeris + ?Sized), t: Instant) -> f64 {
    t.jd_ut - previous_phase(eph, Phase::New, t).jd_ut
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeus::MeeusEphemeris;

    #[test]
    fn meeus_49a_new_moon() {
        // 1977 February: k = −283, JDE = 2443192.65118
        // (1977 Feb 18, 3h37m42s TD).
        assert!((lunation_near(Phase::New, 2_443_190.0) + 283.0).abs() < 1e-12);
        let jde = meeus_phase_jde(-283.0);
        assert!((jde - 2_443_192.651_18).abs() < 2e-5, "{jde}");
        // Meeus notes the correct instant is 3h37m40s TD; refining against
        // the truncated ELP-2000/82 (about 10″ in longitude, i.e. 20 s of
        // time) lands within 10 s of it (6.6 s at the time of writing).
        let eph = MeeusEphemeris::new();
        let t = phase_near_tt(&eph, Phase::New, jde);
        let exact = 2_443_192.5 + (3.0 + 37.0 / 60.0 + 40.0 / 3600.0) / 24.0;
        assert!(
            (t - exact).abs() * 86_400.0 < 10.0,
            "{} s",
            (t - exact) * 86_400.0
        );
    }

    #[test]
    fn meeus_49b_first_quarter() {
        // First quarter of 2044 January: k = 544.75, JDE = 2467636.49186.
        let jde = meeus_phase_jde(544.75);
        assert!((jde - 2_467_636.491_86).abs() < 2e-5, "{jde}");
    }

    #[test]
    fn next_previous_and_age() {
        let eph = MeeusEphemeris::new();
        let t = Instant::from_calendar(2024, 3, 11, 0, 0, 0.0);
        let prev = previous_phase(&eph, Phase::New, t);
        let next = next_phase(&eph, Phase::New, t);
        assert!(prev.jd_ut < t.jd_ut && next.jd_ut > t.jd_ut);
        assert!((next.jd_ut - prev.jd_ut - SYNODIC_MONTH_DAYS).abs() < 0.6);
        // New Moon 2024 March 10, 09:00 UT.
        let c = prev.to_calendar();
        assert_eq!((c.year, c.month, c.day, c.hour), (2024, 3, 10, 9));
        assert!((moon_age_days(&eph, t) - (t.jd_ut - prev.jd_ut)).abs() < 1e-12);
        let all = phases_between(&eph, t, t.add_days(60.0));
        assert_eq!(all.len(), 8);
        assert_eq!(all[0].0, Phase::FirstQuarter);
        assert!(all.windows(2).all(|w| w[0].1.jd_ut < w[1].1.jd_ut));
    }

    #[test]
    fn phases_between_ends_outside_the_ephemeris_range() {
        // Before the fix a NaN phase never compared >= the end, so the loop
        // never returned (the Sky tab asks for ±2 months around its date).
        let t = Instant::from_calendar(2024, 3, 11, 0, 0, 0.0);
        let eph = crate::test_support::Clipped::new(t.jd_tt() - 20.0, t.jd_tt() + 20.0);
        let all = phases_between(&eph, t.add_days(-60.0), t.add_days(60.0));
        // Only the phases inside the covered span, in order.
        assert!((3..=6).contains(&all.len()), "{all:?}");
        assert!(all.iter().all(|(_, p)| p.jd_ut.is_finite()));
        assert!(all.windows(2).all(|w| w[0].1.jd_ut < w[1].1.jd_ut));
        assert!(phases_between(&eph, t.add_days(100.0), t.add_days(200.0)).is_empty());
    }
}
