//! Ephemeris backends: building one from a choice, and the per-session set a
//! frontend keeps.
//!
//! * `meeus`: Meeus, *Astronomical Algorithms* 2nd ed.: VSOP87D Sun
//!   (truncated series, about 230 terms) and ELP-2000/82 Moon. The default.
//! * `meeus-full`: the same with the full VSOP87D Earth series (2425 terms;
//!   within 0.12″ of the truncated one over 1000–3000, slower).
//! * `de440` (cargo feature `de440`): JPL DE440/DE440s from a `.bsp` file,
//!   from bytes on every target ([`de440_from_bytes`], the WASM path) or from
//!   a path off-wasm ([`de440_open`]).

use std::sync::Arc;

pub use mooncalc_core::Ephemeris;
use mooncalc_core::{MeeusEphemeris, SunSeries};

use crate::dto::{ApiError, EphemerisBackendInfo};

pub const MEEUS: &str = "meeus";
pub const MEEUS_FULL: &str = "meeus-full";
pub const DE440: &str = "de440";

/// The id used when a request names no ephemeris.
pub const DEFAULT: &str = MEEUS;

/// The built-in Meeus ephemeris, truncated (default) or full VSOP87D Sun.
pub fn meeus(full: bool) -> Arc<dyn Ephemeris> {
    Arc::new(MeeusEphemeris::with_sun_series(if full {
        SunSeries::Full
    } else {
        SunSeries::Truncated
    }))
}

/// Parse a DE440/DE440s SPK held in memory.
#[cfg(feature = "de440")]
pub fn de440_from_bytes(bytes: Vec<u8>) -> Result<Arc<dyn Ephemeris>, ApiError> {
    mooncalc_spk::de440::De440Ephemeris::from_bytes(bytes)
        .map(|e| Arc::new(e) as Arc<dyn Ephemeris>)
        .map_err(|e| ApiError::ephemeris_unavailable(format!("DE440: {e}")))
}

/// Open a DE440/DE440s `.bsp` file (records are read on demand).
#[cfg(all(feature = "de440", not(target_arch = "wasm32")))]
pub fn de440_open(path: &str) -> Result<Arc<dyn Ephemeris>, ApiError> {
    let path = path.trim();
    if path.is_empty() {
        return Err(ApiError::invalid_params("no DE440 file given"));
    }
    mooncalc_spk::de440::De440Ephemeris::open(path)
        .map(|e| Arc::new(e) as Arc<dyn Ephemeris>)
        .map_err(|e| ApiError::ephemeris_unavailable(format!("DE440 ({path}): {e}")))
}

/// Fail unless `eph` covers TT Julian Days `jd_tt_from..=jd_tt_to`.
pub fn check_covers(eph: &dyn Ephemeris, jd_tt_from: f64, jd_tt_to: f64) -> Result<(), ApiError> {
    let (a, b) = eph.valid_range();
    if jd_tt_from < a || jd_tt_to > b {
        let d = |jd: f64| {
            let c = mooncalc_core::time::Instant::from_jd_ut(jd).to_calendar();
            format!("{:04}-{:02}-{:02}", c.year, c.month, c.day)
        };
        return Err(ApiError::invalid_params(format!(
            "the date is outside the range of {} ({} to {})",
            eph.name(),
            d(a),
            d(b)
        )));
    }
    Ok(())
}

/// A loaded file-backed ephemeris.
#[derive(Clone)]
struct Loaded {
    file: String,
    eph: Arc<dyn Ephemeris>,
}

/// The ephemerides a frontend session can use: the built-in ones plus an
/// optional loaded DE440 file. Cheap to clone (everything is `Arc`).
#[derive(Clone)]
pub struct Ephemerides {
    meeus: Arc<dyn Ephemeris>,
    meeus_full: Arc<dyn Ephemeris>,
    de440: Option<Loaded>,
}

impl Default for Ephemerides {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Ephemerides {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ephemerides")
            .field("de440", &self.de440.as_ref().map(|l| &l.file))
            .finish_non_exhaustive()
    }
}

impl Ephemerides {
    pub fn new() -> Self {
        Self {
            meeus: meeus(false),
            meeus_full: meeus(true),
            de440: None,
        }
    }

    /// The ephemeris for an id (`None` = [`DEFAULT`]).
    pub fn get(&self, id: Option<&str>) -> Result<Arc<dyn Ephemeris>, ApiError> {
        match id.unwrap_or(DEFAULT) {
            MEEUS => Ok(self.meeus.clone()),
            MEEUS_FULL => Ok(self.meeus_full.clone()),
            DE440 if !cfg!(feature = "de440") => Err(ApiError::ephemeris_unavailable(
                "this build has no DE440 support (cargo feature `de440`)",
            )),
            DE440 => self.de440.as_ref().map(|l| l.eph.clone()).ok_or_else(|| {
                ApiError::ephemeris_unavailable("no DE440 file is loaded; choose a .bsp file first")
            }),
            other => Err(ApiError::invalid_params(format!(
                "unknown ephemeris {other:?}"
            ))),
        }
    }

    /// Use `eph`, read from `file`, as the DE440 backend.
    pub fn set_de440(&mut self, file: impl Into<String>, eph: Arc<dyn Ephemeris>) {
        self.de440 = Some(Loaded {
            file: file.into(),
            eph,
        });
    }

    /// Forget the loaded DE440 file.
    pub fn clear_de440(&mut self) {
        self.de440 = None;
    }

    /// Load a DE440 file from a path and use it (native frontends).
    #[cfg(all(feature = "de440", not(target_arch = "wasm32")))]
    pub fn load_de440_path(&mut self, path: &str) -> Result<EphemerisBackendInfo, ApiError> {
        let eph = de440_open(path)?;
        self.set_de440(path.trim(), eph);
        Ok(self.backend(DE440).expect("de440 is listed"))
    }

    /// Load a DE440 file from bytes and use it (`file` is a display name).
    #[cfg(feature = "de440")]
    pub fn load_de440_bytes(
        &mut self,
        file: &str,
        bytes: Vec<u8>,
    ) -> Result<EphemerisBackendInfo, ApiError> {
        let eph = de440_from_bytes(bytes)?;
        self.set_de440(file, eph);
        Ok(self.backend(DE440).expect("de440 is listed"))
    }

    /// One backend's description and state.
    pub fn backend(&self, id: &str) -> Option<EphemerisBackendInfo> {
        self.backends().into_iter().find(|b| b.id == id)
    }

    /// Every backend this build knows about, with its state.
    pub fn backends(&self) -> Vec<EphemerisBackendInfo> {
        vec![
            EphemerisBackendInfo {
                id: MEEUS.into(),
                name: "Meeus / ELP-2000/82".into(),
                description: "Built-in analytical theory (Meeus, Astronomical Algorithms, \
                              2nd ed.): truncated VSOP87D Sun, ELP-2000/82 Moon. No data \
                              files needed."
                    .into(),
                available: true,
                needs_file: false,
                ready: true,
                file: None,
                detail: Some(self.meeus.name().into()),
            },
            EphemerisBackendInfo {
                id: MEEUS_FULL.into(),
                name: "Meeus, full VSOP87D".into(),
                description: "As Meeus, with the full 2425-term VSOP87D Earth series for the \
                              Sun (within 0.12″ of the truncated series; slower)."
                    .into(),
                available: true,
                needs_file: false,
                ready: true,
                file: None,
                detail: Some(self.meeus_full.name().into()),
            },
            EphemerisBackendInfo {
                id: DE440.into(),
                name: "JPL DE440".into(),
                description: "Numerical ephemeris read from a user-supplied DE440 or DE440s \
                              .bsp file. Requires a build with the `de440` feature."
                    .into(),
                available: cfg!(feature = "de440"),
                needs_file: true,
                ready: self.de440.is_some(),
                file: self.de440.as_ref().map(|l| l.file.clone()),
                detail: self.de440.as_ref().map(|l| l.eph.name().to_owned()),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_backends_are_ready() {
        let e = Ephemerides::new();
        assert!(e.get(None).is_ok());
        assert!(e.get(Some(MEEUS_FULL)).is_ok());
        assert_eq!(
            e.get(Some("vsop2013")).err().unwrap().code,
            "invalid-params"
        );
        assert_eq!(
            e.get(Some(DE440)).err().unwrap().code,
            "ephemeris-unavailable"
        );
        let b = e.backends();
        assert!(b.iter().any(|b| b.id == DEFAULT && b.ready));
        assert_eq!(
            b.iter().find(|b| b.id == DE440).unwrap().available,
            cfg!(feature = "de440")
        );
    }

    #[test]
    fn set_de440_makes_it_ready() {
        // Any ephemeris will do for the bookkeeping.
        let mut e = Ephemerides::new();
        e.set_de440("test.bsp", meeus(false));
        let info = e.backend(DE440).unwrap();
        assert!(info.ready);
        assert_eq!(info.file.as_deref(), Some("test.bsp"));
        if cfg!(feature = "de440") {
            assert!(e.get(Some(DE440)).is_ok());
        }
        e.clear_de440();
        assert!(!e.backend(DE440).unwrap().ready);
    }

    #[test]
    fn coverage_check() {
        let m = meeus(false);
        assert!(check_covers(&*m, 2_460_000.0, 2_460_010.0).is_ok());
        assert!(check_covers(&*m, 0.0, 1.0).is_err());
    }

    #[cfg(feature = "de440")]
    #[test]
    fn de440_rejects_garbage() {
        assert_eq!(
            de440_from_bytes(vec![0; 2048]).err().unwrap().code,
            "ephemeris-unavailable"
        );
    }
}
