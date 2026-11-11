//! Coordinate types and frame transformations.
//!
//! The types are fixed (other crates depend on them). The transformation
//! functions are implemented by the core-foundation work package.

/// An observer's position on the Earth (WGS84 geodetic).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoLocation {
    pub lat_deg: f64,
    /// East-positive.
    pub lon_deg: f64,
    pub elevation_m: f64,
}

impl GeoLocation {
    pub const fn new(lat_deg: f64, lon_deg: f64, elevation_m: f64) -> Self {
        Self { lat_deg, lon_deg, elevation_m }
    }
}

/// Equatorial coordinates (frame stated by the producer; ephemerides return
/// apparent coordinates referred to the true equator and equinox of date).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Equatorial {
    /// Right ascension, radians in `[0, 2π)`.
    pub ra: f64,
    /// Declination, radians.
    pub dec: f64,
    /// Distance from the reference point (geocentre or observer), km.
    pub distance_km: f64,
}

/// Ecliptic coordinates (of date unless stated otherwise).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ecliptic {
    pub lon: f64,
    pub lat: f64,
    pub distance_km: f64,
}

/// Horizontal coordinates for an observer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Horizontal {
    /// Altitude above the horizon, radians.
    pub alt: f64,
    /// Azimuth from north through east, radians in `[0, 2π)`.
    pub az: f64,
}
