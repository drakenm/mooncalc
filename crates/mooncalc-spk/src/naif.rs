//! NAIF integer IDs used by the JPL planetary ephemerides
//! (*NAIF IDs Required Reading*, `naif_ids.req`).

/// Solar System barycentre.
pub const SOLAR_SYSTEM_BARYCENTER: i32 = 0;
/// Mercury barycentre.
pub const MERCURY_BARYCENTER: i32 = 1;
/// Venus barycentre.
pub const VENUS_BARYCENTER: i32 = 2;
/// Earth–Moon barycentre.
pub const EARTH_MOON_BARYCENTER: i32 = 3;
/// Mars barycentre.
pub const MARS_BARYCENTER: i32 = 4;
/// Jupiter barycentre.
pub const JUPITER_BARYCENTER: i32 = 5;
/// Saturn barycentre.
pub const SATURN_BARYCENTER: i32 = 6;
/// Uranus barycentre.
pub const URANUS_BARYCENTER: i32 = 7;
/// Neptune barycentre.
pub const NEPTUNE_BARYCENTER: i32 = 8;
/// Pluto barycentre.
pub const PLUTO_BARYCENTER: i32 = 9;
/// The Sun.
pub const SUN: i32 = 10;
/// Mercury.
pub const MERCURY: i32 = 199;
/// Venus.
pub const VENUS: i32 = 299;
/// The Moon.
pub const MOON: i32 = 301;
/// The Earth.
pub const EARTH: i32 = 399;

/// The J2000 inertial frame. For the DE4xx ephemerides it is realised as
/// the ICRF (Park et al., "The JPL Planetary and Lunar Ephemerides DE440
/// and DE441", *AJ* 161:105, 2021).
pub const FRAME_J2000: i32 = 1;
