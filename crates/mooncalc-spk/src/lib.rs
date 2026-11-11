//! Reader for JPL SPICE SPK ephemeris files such as DE440 and DE440s.
//!
//! The crate parses the NAIF DAF container ([`daf`], after *DAF Required
//! Reading*), lists its segments and evaluates SPK data types 2 (Chebyshev
//! position) and 3 (Chebyshev position and velocity) after *SPK Required
//! Reading*. [`Spk::position`] and [`Spk::state`] chain segments through the
//! barycentre tree, so any covered body can be referred to any other.
//!
//! Results are **geometric** (no light time or aberration), in km and km/s,
//! in the J2000/ICRF frame of the DE files. Epochs are ET: TDB seconds past
//! J2000 (see [`time`]).
//!
//! The parser runs over any [`ByteSource`]: a `Read + Seek` stream, or
//! bytes in memory ([`Spk::from_bytes`]), so it builds for
//! `wasm32-unknown-unknown`. The path helpers [`Spk::open`] and
//! [`Spk::open_in_memory`] are only available off WASM.
//!
//! The apparent-place [`mooncalc_core::Ephemeris`] backend is planned in
//! [`de440`].

pub mod daf;
pub mod de440;
pub mod error;
pub mod naif;
pub mod source;
pub mod time;

mod cheb;
mod spk;

pub use daf::{ByteOrder, FileRecord};
pub use error::SpkError;
pub use source::{ByteSource, MemorySource, ReaderSource};
pub use spk::{Segment, Spk, StateVector};
