//! Helpers shared by unit tests.

use crate::coords::{Ecliptic, Equatorial};
use crate::ephemeris::Ephemeris;
use crate::meeus::MeeusEphemeris;

/// Meeus inside `[from, to]` (TT), NaN coordinates outside, like a DE440
/// file evaluated beyond its coverage.
pub(crate) struct Clipped {
    pub inner: MeeusEphemeris,
    pub from: f64,
    pub to: f64,
}

impl Clipped {
    pub fn new(from: f64, to: f64) -> Self {
        Self {
            inner: MeeusEphemeris::new(),
            from,
            to,
        }
    }

    fn ok(&self, jd_tt: f64) -> bool {
        (self.from..=self.to).contains(&jd_tt)
    }

    fn eq(&self, jd_tt: f64, mut e: Equatorial) -> Equatorial {
        if !self.ok(jd_tt) {
            e = Equatorial {
                ra: f64::NAN,
                dec: f64::NAN,
                distance_km: f64::NAN,
            };
        }
        e
    }

    fn ecl(&self, jd_tt: f64, mut e: Ecliptic) -> Ecliptic {
        if !self.ok(jd_tt) {
            e = Ecliptic {
                lon: f64::NAN,
                lat: f64::NAN,
                distance_km: f64::NAN,
            };
        }
        e
    }
}

impl Ephemeris for Clipped {
    fn name(&self) -> &str {
        "clipped Meeus"
    }
    fn valid_range(&self) -> (f64, f64) {
        (self.from, self.to)
    }
    fn sun_equatorial(&self, jd_tt: f64) -> Equatorial {
        self.eq(jd_tt, self.inner.sun_equatorial(jd_tt))
    }
    fn moon_equatorial(&self, jd_tt: f64) -> Equatorial {
        self.eq(jd_tt, self.inner.moon_equatorial(jd_tt))
    }
    fn sun_ecliptic(&self, jd_tt: f64) -> Ecliptic {
        self.ecl(jd_tt, self.inner.sun_ecliptic(jd_tt))
    }
    fn moon_ecliptic(&self, jd_tt: f64) -> Ecliptic {
        self.ecl(jd_tt, self.inner.moon_ecliptic(jd_tt))
    }
}
