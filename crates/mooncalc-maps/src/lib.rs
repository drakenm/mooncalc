//! World crescent-visibility maps.
//!
//! * [`grid`]: a latitude/longitude grid of per-cell zone indices.
//! * [`engine`]: fills a grid by evaluating a criterion at each cell on its
//!   local evening, with a continuous value field for smooth contours, and
//!   the earliest-sighting scan.
//! * [`projection`]: map projections (equirectangular first, orthographic
//!   "globe" later).
//! * [`basemap`]: embedded coastline / country-border data (Natural Earth,
//!   public domain).
//! * [`render`]: turns a grid + base map into RGBA pixels or SVG.

pub mod basemap;
pub mod engine;
pub mod grid;
pub mod projection;
pub mod render;
