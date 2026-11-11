//! World crescent-visibility maps.
//!
//! * [`grid`]: a latitude/longitude grid of per-cell zone indices (filled by
//!   evaluating a criterion at each cell; the evaluation engine lives in
//!   [`grid`] too, see that module).
//! * [`projection`]: map projections (equirectangular first, orthographic
//!   "globe" later).
//! * [`basemap`]: embedded coastline / country-border data (Natural Earth,
//!   public domain).
//! * [`render`]: turns a grid + base map into RGBA pixels or SVG.

pub mod basemap;
pub mod grid;
pub mod projection;
pub mod render;
