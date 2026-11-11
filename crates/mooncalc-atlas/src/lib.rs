//! Embedded world city atlas with search, for choosing observer locations.
//!
//! * [`Atlas::builtin`] holds the ~34 000 places with population ≥ 15 000
//!   from the GeoNames `cities15000` dump (with country, admin-1 and time-zone
//!   data), converted by `tools/atlas/build_atlas.py` into
//!   `assets/atlas/cities.tsv` and embedded with `include_str!`.
//! * [`Atlas::search`] is case- and diacritic-insensitive, understands
//!   `City, Country` input and ranks by match quality, then population.
//! * [`Atlas::nearest`] finds the closest city to a point (bucket grid; cheap
//!   enough for mouse-hover use).
//! * [`tz`]: IANA time zones with their full rules (DST, history) and fixed
//!   offsets ([`Zone`]): the UTC offset at any instant, local ↔ UT with DST
//!   gaps and folds, local day bounds. Every [`City::timezone`] is a valid
//!   [`Zone::iana`] name.
//! * [`UserAtlas`] holds up to [`MAX_USER_LOCATIONS`] user-defined locations,
//!   stored as JSON; [`Atlas::search_with_user`] merges them into searches.
//!
//! The crate uses no threads or file I/O outside the native-only
//! [`UserAtlas::load`]/[`UserAtlas::save`] helpers, so it builds for
//! `wasm32-unknown-unknown`.
//!
//! Data: GeoNames (<https://www.geonames.org/>), licensed CC BY 4.0. Display
//! [`ATTRIBUTION`] wherever atlas data is shown. Time-zone rules: the IANA
//! Time Zone Database, public domain ([`tz::TZ_ATTRIBUTION`]).

mod atlas;
mod fold;
mod fold_table;
pub mod tz;
mod user;

pub use atlas::{Atlas, City, Country, MatchQuality, ParseError, distance_km};
pub use fold::fold;
pub use tz::{LocalTime, Zone};
pub use user::{MAX_USER_LOCATIONS, Place, UserAtlas, UserAtlasError, UserLocation};

/// Credit line required by the GeoNames licence (CC BY 4.0).
pub const ATTRIBUTION: &str = "City data © GeoNames (https://www.geonames.org/), \
licensed under Creative Commons Attribution 4.0 (CC BY 4.0, \
https://creativecommons.org/licenses/by/4.0/). Converted and abridged for MoonCalc.";

#[cfg(test)]
mod tests {
    #[test]
    fn attribution_names_source_and_licence() {
        assert!(super::ATTRIBUTION.contains("GeoNames"));
        assert!(super::ATTRIBUTION.contains("https://www.geonames.org/"));
        assert!(super::ATTRIBUTION.contains("CC BY 4.0"));
    }
}
