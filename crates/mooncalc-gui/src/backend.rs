//! Ephemeris backend selection.
//!
//! The Meeus/ELP backend (from `mooncalc-core`) is always available. The JPL
//! DE440 backend (from `mooncalc-spk`) exists only when the GUI is compiled
//! with `--features de440`, and needs a user-supplied `.bsp` file.
//!
//! **Extension point:** [`create_ephemeris`] is where the GUI obtains an
//! `Arc<dyn Ephemeris>` for computations. It returns an error until the
//! core-foundation (Meeus) and spk (DE440) packages provide their backends;
//! wire them in there and every feature picks them up.

use std::sync::Arc;

use mooncalc_core::Ephemeris;
use serde::{Deserialize, Serialize};

/// The user's choice of ephemeris backend (persisted).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BackendChoice {
    /// Meeus, *Astronomical Algorithms*: VSOP87 Sun, ELP-2000/82 Moon.
    #[default]
    Meeus,
    /// JPL Development Ephemeris DE440/DE440s, read from a `.bsp` file.
    De440,
}

impl BackendChoice {
    /// Every backend, whether compiled in or not.
    pub const ALL: [BackendChoice; 2] = [BackendChoice::Meeus, BackendChoice::De440];

    pub fn label(self) -> &'static str {
        match self {
            BackendChoice::Meeus => "Meeus (VSOP87 / ELP-2000)",
            BackendChoice::De440 => "JPL DE440",
        }
    }

    /// Whether this backend is compiled into this build.
    pub fn is_compiled_in(self) -> bool {
        match self {
            BackendChoice::Meeus => true,
            BackendChoice::De440 => cfg!(feature = "de440"),
        }
    }

    /// The backend actually used: a persisted `De440` choice falls back to
    /// Meeus in a build without the `de440` feature.
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

/// One-line description of the active backend, for the status bar.
pub fn status_text(choice: BackendChoice, de440_path: &str) -> String {
    match choice.effective() {
        BackendChoice::Meeus => format!("Ephemeris: {}", BackendChoice::Meeus.label()),
        BackendChoice::De440 => match de440_path_problem(de440_path) {
            None => {
                let name = std::path::Path::new(de440_path.trim())
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                format!("Ephemeris: JPL DE440 ({name})")
            }
            Some(problem) => format!("Ephemeris: JPL DE440 — {problem}"),
        },
    }
}

/// **Extension point.** Build the ephemeris for the given choice.
///
/// Implement the `Meeus` arm with the core crate's backend and the `De440`
/// arm (behind `#[cfg(feature = "de440")]`) with `mooncalc-spk`'s reader.
pub fn create_ephemeris(
    choice: BackendChoice,
    de440_path: &str,
) -> Result<Arc<dyn Ephemeris>, String> {
    match choice.effective() {
        BackendChoice::Meeus => {
            Err("The Meeus ephemeris backend is not wired into the GUI yet".into())
        }
        BackendChoice::De440 => match de440_path_problem(de440_path) {
            Some(problem) => Err(format!("DE440: {problem}")),
            None => Err("The DE440 backend is not wired into the GUI yet".into()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeus_is_always_available() {
        assert!(available().contains(&BackendChoice::Meeus));
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
}
