//! Ephemeris backend selection.
//!
//! The Meeus backend (from `mooncalc-core`) is always available, with the
//! truncated VSOP87D Sun series (default, fast) or the full series. The JPL
//! DE440 backend (from `mooncalc-spk`) exists only when the GUI is compiled
//! with `--features de440`, and needs a user-supplied `.bsp` file.
//!
//! [`resolve`] turns the persisted choice into an [`ActiveEphemeris`]: one
//! shared `Arc<dyn Ephemeris>` that every tab and background job uses. It is
//! cached per (choice, path), so calling it every frame is cheap and every
//! caller gets the same instance. A DE440 file that cannot be opened falls
//! back to Meeus with a [`ActiveEphemeris::notice`] explaining why.

use std::sync::{Arc, Mutex};

use mooncalc_core::{Ephemeris, MeeusEphemeris, SunSeries};
use serde::{Deserialize, Serialize};

/// The user's choice of ephemeris backend (persisted).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BackendChoice {
    /// Meeus, *Astronomical Algorithms*: truncated VSOP87D Sun, ELP-2000/82 Moon.
    #[default]
    Meeus,
    /// Meeus with the full VSOP87D Earth series for the Sun (slower).
    MeeusFull,
    /// JPL Development Ephemeris DE440/DE440s, read from a `.bsp` file.
    De440,
}

impl BackendChoice {
    /// Every backend, whether compiled in or not.
    pub const ALL: [BackendChoice; 3] = [
        BackendChoice::Meeus,
        BackendChoice::MeeusFull,
        BackendChoice::De440,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BackendChoice::Meeus => "Meeus (VSOP87 truncated / ELP-2000)",
            BackendChoice::MeeusFull => "Meeus (VSOP87 full / ELP-2000)",
            BackendChoice::De440 => "JPL DE440",
        }
    }

    /// Short label for the status bar and reports.
    pub fn short_label(self) -> &'static str {
        match self {
            BackendChoice::Meeus => "Meeus",
            BackendChoice::MeeusFull => "Meeus (full VSOP87)",
            BackendChoice::De440 => "JPL DE440",
        }
    }

    /// Whether this backend is compiled into this build.
    pub fn is_compiled_in(self) -> bool {
        match self {
            BackendChoice::Meeus | BackendChoice::MeeusFull => true,
            BackendChoice::De440 => cfg!(feature = "de440"),
        }
    }

    /// The backend actually requested in this build: a persisted `De440`
    /// choice falls back to Meeus in a build without the `de440` feature.
    pub fn effective(self) -> BackendChoice {
        if self.is_compiled_in() {
            self
        } else {
            BackendChoice::Meeus
        }
    }
}

/// Backends selectable in this build.
pub fn available() -> Vec<BackendChoice> {
    BackendChoice::ALL
        .into_iter()
        .filter(|b| b.is_compiled_in())
        .collect()
}

/// Problems with the DE440 file path, for display next to the input.
pub fn de440_path_problem(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        return Some("No .bsp file selected".into());
    }
    let p = std::path::Path::new(path);
    if !p.is_file() {
        return Some("File not found".into());
    }
    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsp")) {
        return Some("Expected a .bsp (SPK) file".into());
    }
    None
}

/// The ephemeris in use, shared by every tab and job.
#[derive(Clone)]
pub struct ActiveEphemeris {
    pub eph: Arc<dyn Ephemeris>,
    /// The backend actually in use (Meeus after a DE440 fallback).
    pub used: BackendChoice,
    /// Why the requested backend is not the one in use, if it isn't.
    pub notice: Option<String>,
}

impl std::fmt::Debug for ActiveEphemeris {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveEphemeris")
            .field("name", &self.eph.name())
            .field("used", &self.used)
            .field("notice", &self.notice)
            .finish()
    }
}

impl ActiveEphemeris {
    /// Covered UT date range as `YYYY-MM-DD` strings.
    pub fn coverage_text(&self) -> String {
        let (a, b) = self.eph.valid_range();
        format!("{} to {}", format_jd_date(a), format_jd_date(b))
    }

    /// `Some(message)` when `jd_ut` (± `margin_days`) lies outside the
    /// ephemeris coverage.
    pub fn coverage_problem(&self, jd_ut: f64, margin_days: f64) -> Option<String> {
        let (a, b) = self.eph.valid_range();
        // TT − UT is at most minutes in DE440's range; a day of margin
        // covers it along with the engine's tabulation window.
        (jd_ut - margin_days < a || jd_ut + margin_days > b).then(|| {
            format!(
                "{} is outside the {} coverage ({})",
                format_jd_date(jd_ut),
                self.used.short_label(),
                self.coverage_text()
            )
        })
    }

    /// One-line description for the status bar.
    pub fn status_text(&self) -> String {
        let mut s = format!("Ephemeris: {}", self.used.short_label());
        if self.used == BackendChoice::De440 {
            s.push_str(&format!(" ({})", self.coverage_text()));
        }
        if self.notice.is_some() {
            s.push_str(" (fallback)");
        }
        s
    }
}

fn format_jd_date(jd: f64) -> String {
    let c = mooncalc_core::Instant::from_jd_ut(jd).to_calendar();
    format!("{:04}-{:02}-{:02}", c.year, c.month, c.day)
}

#[derive(Clone, PartialEq, Eq)]
struct CacheKey {
    choice: BackendChoice,
    path: String,
}

static CACHE: Mutex<Option<(CacheKey, ActiveEphemeris)>> = Mutex::new(None);

/// The shared ephemeris for the given choice, falling back to Meeus (with a
/// notice) when DE440 is unavailable. Cached: cheap to call every frame.
pub fn resolve(choice: BackendChoice, de440_path: &str) -> ActiveEphemeris {
    let key = CacheKey {
        choice: choice.effective(),
        path: if choice.effective() == BackendChoice::De440 {
            de440_path.trim().to_owned()
        } else {
            String::new()
        },
    };
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((k, active)) = cache.as_ref()
        && *k == key
    {
        return active.clone();
    }
    let active = build(key.choice, &key.path);
    *cache = Some((key, active.clone()));
    active
}

/// Forget the cached ephemeris, so the next [`resolve`] reopens the file.
pub fn reload() {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn meeus(series: SunSeries, used: BackendChoice, notice: Option<String>) -> ActiveEphemeris {
    ActiveEphemeris {
        eph: Arc::new(MeeusEphemeris::with_sun_series(series)),
        used,
        notice,
    }
}

fn build(choice: BackendChoice, path: &str) -> ActiveEphemeris {
    match choice {
        BackendChoice::Meeus => meeus(SunSeries::Truncated, BackendChoice::Meeus, None),
        BackendChoice::MeeusFull => meeus(SunSeries::Full, BackendChoice::MeeusFull, None),
        BackendChoice::De440 => match open_de440(path) {
            Ok(eph) => ActiveEphemeris {
                eph,
                used: BackendChoice::De440,
                notice: None,
            },
            Err(e) => meeus(
                SunSeries::Truncated,
                BackendChoice::Meeus,
                Some(format!("DE440 unavailable ({e}); using Meeus instead")),
            ),
        },
    }
}

#[cfg(feature = "de440")]
fn open_de440(path: &str) -> Result<Arc<dyn Ephemeris>, String> {
    if let Some(problem) = de440_path_problem(path) {
        return Err(problem);
    }
    mooncalc_spk::de440::De440Ephemeris::open(path)
        .map(|e| Arc::new(e) as Arc<dyn Ephemeris>)
        .map_err(|e| e.to_string())
}

#[cfg(not(feature = "de440"))]
fn open_de440(_path: &str) -> Result<Arc<dyn Ephemeris>, String> {
    Err("this build has no DE440 support".into())
}

/// One-line description of the active backend, for the status bar.
pub fn status_text(choice: BackendChoice, de440_path: &str) -> String {
    resolve(choice, de440_path).status_text()
}

/// Build the ephemeris for the given choice, without falling back: an error
/// when DE440 is chosen and its file cannot be opened. Shares the instance
/// cached by [`resolve`].
pub fn create_ephemeris(
    choice: BackendChoice,
    de440_path: &str,
) -> Result<Arc<dyn Ephemeris>, String> {
    let active = resolve(choice, de440_path);
    match active.notice {
        Some(notice) => Err(notice),
        None => Ok(active.eph),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeus_is_always_available() {
        assert!(available().contains(&BackendChoice::Meeus));
        assert!(available().contains(&BackendChoice::MeeusFull));
        assert_eq!(BackendChoice::Meeus.effective(), BackendChoice::Meeus);
    }

    #[test]
    fn de440_availability_follows_the_feature() {
        assert_eq!(
            available().contains(&BackendChoice::De440),
            cfg!(feature = "de440")
        );
        let expected = if cfg!(feature = "de440") {
            BackendChoice::De440
        } else {
            BackendChoice::Meeus
        };
        assert_eq!(BackendChoice::De440.effective(), expected);
    }

    #[test]
    fn de440_path_validation() {
        assert!(de440_path_problem("  ").is_some());
        assert!(de440_path_problem("definitely/not/here.bsp").is_some());
        // Cargo.toml exists but is not a .bsp file.
        let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        assert_eq!(
            de440_path_problem(manifest).as_deref(),
            Some("Expected a .bsp (SPK) file")
        );
    }

    #[test]
    fn missing_de440_falls_back_to_meeus_with_a_notice() {
        let a = resolve(BackendChoice::De440, "definitely/not/here.bsp");
        assert_eq!(a.used, BackendChoice::Meeus);
        // Without the feature a De440 choice is simply Meeus (no notice).
        assert_eq!(a.notice.is_some(), cfg!(feature = "de440"));
        assert_eq!(
            create_ephemeris(BackendChoice::De440, "definitely/not/here.bsp").is_err(),
            cfg!(feature = "de440")
        );
        let m = resolve(BackendChoice::Meeus, "");
        assert!(m.notice.is_none());
        assert!(create_ephemeris(BackendChoice::Meeus, "").is_ok());
        assert!(m.coverage_problem(2_460_000.5, 2.0).is_none());
        assert!(m.coverage_problem(1.0e9, 2.0).is_some());
    }
}
