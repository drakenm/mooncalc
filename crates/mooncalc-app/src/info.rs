//! Application information.

use crate::dto::AppInfo;
use crate::ephemeris::{self, Ephemerides};

/// Product name shown in the UI.
pub const APP_NAME: &str = "MoonCalc";

/// Static application information plus the state of the session's
/// ephemerides. `runtime` names the hosting frontend.
pub fn app_info(runtime: &str, ephemerides: &Ephemerides) -> AppInfo {
    AppInfo {
        name: APP_NAME.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        runtime: runtime.into(),
        ephemeris_backends: ephemerides.backends(),
        default_ephemeris: ephemeris::DEFAULT.into(),
        attributions: vec![
            mooncalc_atlas::ATTRIBUTION.into(),
            crate::basemap::ATTRIBUTION.into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_default_backend() {
        let info = app_info("test", &Ephemerides::new());
        assert!(
            info.ephemeris_backends
                .iter()
                .any(|b| b.id == info.default_ephemeris && b.available && b.ready)
        );
        assert!(info.attributions.iter().any(|a| a.contains("GeoNames")));
    }
}
