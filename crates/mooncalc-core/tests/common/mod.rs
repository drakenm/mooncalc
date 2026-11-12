//! Shared helpers for the fixture-driven integration tests: a minimal CSV
//! reader for the files in `tests/fixtures/` (plain UTF-8, one header row,
//! no quoting, no comment lines; see `docs/validation.md`) and residual
//! statistics.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

use mooncalc_core::SunSeries;
use mooncalc_core::matrix::from_spherical;
use mooncalc_core::meeus::moon::moon_mean_ecliptic;
use mooncalc_core::meeus::sun::sun_geometric;
use mooncalc_core::time::{SECONDS_PER_DAY, julian_day};
use mooncalc_core::topocentric::AU_KM;

/// Speed of light, km/s.
pub const C_KM_S: f64 = 299_792.458;

/// One CSV file: the header and the data rows, as strings.
pub struct Csv {
    pub columns: HashMap<String, usize>,
    pub rows: Vec<Vec<String>>,
}

impl Csv {
    /// Read `tests/fixtures/<rel>` from the repository root.
    pub fn fixture(rel: &str) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let header = lines.next().expect("empty fixture");
        let columns = header
            .split(',')
            .enumerate()
            .map(|(i, name)| (name.trim().to_owned(), i))
            .collect::<HashMap<_, _>>();
        let rows = lines
            .map(|l| {
                l.split(',')
                    .map(|f| f.trim().to_owned())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for (i, r) in rows.iter().enumerate() {
            assert_eq!(
                r.len(),
                columns.len(),
                "{rel}: row {} has wrong width",
                i + 2
            );
        }
        Self { columns, rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Iterate over rows with named-field access.
    pub fn records(&self) -> impl Iterator<Item = Record<'_>> {
        self.rows.iter().map(move |r| Record {
            columns: &self.columns,
            fields: r,
        })
    }
}

/// One CSV row with named-field access.
pub struct Record<'a> {
    columns: &'a HashMap<String, usize>,
    fields: &'a [String],
}

impl Record<'_> {
    pub fn str(&self, name: &str) -> &str {
        let i = *self
            .columns
            .get(name)
            .unwrap_or_else(|| panic!("no column {name}"));
        &self.fields[i]
    }

    pub fn f64(&self, name: &str) -> f64 {
        let s = self.str(name);
        s.parse()
            .unwrap_or_else(|e| panic!("column {name}: cannot parse {s:?}: {e}"))
    }

    /// `None` for an empty field or one that is not a plain number
    /// (e.g. NASA's `<1`).
    pub fn opt_f64(&self, name: &str) -> Option<f64> {
        self.str(name).parse().ok()
    }
}

/// Running maximum and RMS of absolute residuals, remembering where the
/// maximum occurred.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub n: usize,
    pub sum_sq: f64,
    pub max: f64,
    pub max_at: String,
}

impl Stats {
    pub fn add(&mut self, residual: f64, label: impl Into<String>) {
        let a = residual.abs();
        self.n += 1;
        self.sum_sq += a * a;
        if a > self.max || self.n == 1 {
            self.max = a;
            self.max_at = label.into();
        }
    }

    pub fn rms(&self) -> f64 {
        if self.n == 0 {
            0.0
        } else {
            (self.sum_sq / self.n as f64).sqrt()
        }
    }

    /// One line for `--nocapture` output.
    pub fn report(&self, name: &str, unit: &str) -> String {
        format!(
            "{name:<34} n={:>3}  max={:>9.3}{unit}  rms={:>9.3}{unit}  (max at {})",
            self.n,
            self.max,
            self.rms(),
            self.max_at
        )
    }
}

/// TAI − UTC in seconds for a UTC Julian Day from 1972 on (IERS Bulletin C;
/// the last leap second was 2016-12-31, and none is announced as of
/// 2026-11). Returns `None` before 1972, where UTC used rate offsets.
pub fn tai_minus_utc(jd_utc: f64) -> Option<f64> {
    // (year, month) on whose first day at 0h UTC the offset becomes
    // 10 s, 11 s, 12 s, ... (one second per entry).
    const STEPS: [(i32, u32); 28] = [
        (1972, 1),
        (1972, 7),
        (1973, 1),
        (1974, 1),
        (1975, 1),
        (1976, 1),
        (1977, 1),
        (1978, 1),
        (1979, 1),
        (1980, 1),
        (1981, 7),
        (1982, 7),
        (1983, 7),
        (1985, 7),
        (1988, 1),
        (1990, 1),
        (1991, 1),
        (1992, 7),
        (1993, 7),
        (1994, 7),
        (1996, 1),
        (1997, 7),
        (1999, 1),
        (2006, 1),
        (2009, 1),
        (2012, 7),
        (2015, 7),
        (2017, 1),
    ];
    STEPS
        .iter()
        .enumerate()
        .rev()
        .find(|(_, (y, m))| jd_utc >= julian_day(*y, *m, 1.0))
        .map(|(i, _)| 10.0 + i as f64)
}

/// Geocentric ecliptic position vector (km, mean ecliptic of date) of the
/// Sun (`"sun"`) or the Moon from the core's geometric series.
pub fn geometric_vector(body: &str, jd_tt: f64) -> [f64; 3] {
    if body == "sun" {
        let (lon, lat, r_au) = sun_geometric(jd_tt, SunSeries::Full);
        from_spherical(lon, lat, r_au * AU_KM)
    } else {
        let e = moon_mean_ecliptic(jd_tt);
        from_spherical(e.lon, e.lat, e.distance_km)
    }
}

/// Horizons' light-time ("apparent") range of the Moon, `delta_km`,
/// converted to a geometric distance.
///
/// Horizons forms `Δ = |r_moon(t − τ) − r_obs(t)|` in the barycentric frame,
/// so to first order `ρ − Δ = τ (v_E + v_rel) · û`, with `v_E` the Earth's
/// barycentric velocity and `v_rel` the Moon's geocentric velocity. For
/// τ ≈ 1.28 s the `v_E` part is about +38 km · sin D. The Earth's
/// heliocentric velocity stands in for the barycentric one (they differ by
/// ≲ 15 m/s, i.e. ≲ 0.02 km here). For a topocentric range the observer's
/// rotational velocity (≤ 0.47 km/s, ≤ 0.6 km) is neglected.
pub fn moon_geometric_distance_from_horizons(jd_tt: f64, delta_km: f64) -> f64 {
    let h = 0.01; // days
    let vel = |body: &str| {
        let a = geometric_vector(body, jd_tt - h);
        let b = geometric_vector(body, jd_tt + h);
        [0, 1, 2].map(|i| (b[i] - a[i]) / (2.0 * h * SECONDS_PER_DAY))
    };
    // The Earth's heliocentric velocity is minus the Sun's geocentric one.
    let v_sun = vel("sun");
    let v_rel = vel("moon");
    let u = geometric_vector("moon", jd_tt);
    let r = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    let dot: f64 = (0..3).map(|i| (v_rel[i] - v_sun[i]) * u[i] / r).sum();
    delta_km + delta_km / C_KM_S * dot
}
