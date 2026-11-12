//! Error type for SPK reading and evaluation.

use std::fmt;
use std::io;

/// Everything that can go wrong opening or evaluating an SPK file.
#[derive(Debug)]
pub enum SpkError {
    /// Reading the underlying source failed (including a truncated file,
    /// reported as [`io::ErrorKind::UnexpectedEof`]).
    Io(io::Error),
    /// The file record does not carry a DAF identification word.
    NotDaf,
    /// The file is a DAF, but not an SPK (`ND = 2`, `NI = 6`).
    NotSpk {
        /// The identification word found, e.g. `DAF/CK`.
        id_word: String,
        /// Number of double-precision summary components.
        nd: usize,
        /// Number of integer summary components.
        ni: usize,
    },
    /// The binary file format (for example `VAX-GFLT`) is not IEEE.
    UnsupportedFormat(String),
    /// The file structure is inconsistent.
    Corrupt(String),
    /// A segment uses an SPK data type this reader does not evaluate.
    UnsupportedType {
        /// Index of the segment in [`crate::Spk::segments`].
        segment: usize,
        /// SPK data type number.
        data_type: i32,
    },
    /// A segment's reference frame is not J2000/ICRF (NAIF frame 1), so its
    /// vectors cannot be chained with the others.
    UnsupportedFrame {
        /// Index of the segment in [`crate::Spk::segments`].
        segment: usize,
        /// NAIF frame ID.
        frame: i32,
    },
    /// The body has segments, but none covers the requested epoch.
    NoCoverage {
        /// NAIF body ID.
        body: i32,
        /// Requested epoch, TDB seconds past J2000.
        et: f64,
    },
    /// No chain of segments connects the target to the center at this epoch.
    NoPath {
        /// NAIF ID of the target body.
        target: i32,
        /// NAIF ID of the center body.
        center: i32,
        /// Requested epoch, TDB seconds past J2000.
        et: f64,
    },
}

impl fmt::Display for SpkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "SPK read error: {e}"),
            Self::NotDaf => write!(f, "not a NAIF DAF file"),
            Self::NotSpk { id_word, nd, ni } => write!(
                f,
                "DAF file is not an SPK (id word {id_word:?}, ND={nd}, NI={ni})"
            ),
            Self::UnsupportedFormat(fmt_name) => {
                write!(f, "unsupported DAF binary format {fmt_name:?}")
            }
            Self::Corrupt(msg) => write!(f, "corrupt SPK file: {msg}"),
            Self::UnsupportedType { segment, data_type } => write!(
                f,
                "segment {segment} uses SPK data type {data_type}; only types 2 and 3 are supported"
            ),
            Self::UnsupportedFrame { segment, frame } => write!(
                f,
                "segment {segment} uses frame {frame}; only J2000/ICRF (frame 1) is supported"
            ),
            Self::NoCoverage { body, et } => {
                write!(f, "no SPK segment for body {body} covers ET {et} s")
            }
            Self::NoPath { target, center, et } => write!(
                f,
                "no SPK segment chain connects body {target} to center {center} at ET {et} s"
            ),
        }
    }
}

impl std::error::Error for SpkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for SpkError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
