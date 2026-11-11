//! SPK segments, type 2/3 evaluation and the target/center chain.
//!
//! Reference: NAIF *SPK Required Reading* (`spk.req`, SPICE Toolkit N0067),
//! sections "Segments—The Fundamental SPK Building Blocks", "Type 2:
//! Chebyshev (position only)" and "Type 3: Chebyshev (position and
//! velocity)", and the SPICELIB routines `SPKR02`/`SPKE02`/`SPKE03`.

use std::sync::Mutex;

use crate::cheb;
use crate::daf::{self, ByteOrder, FileRecord};
use crate::error::SpkError;
use crate::naif;
use crate::source::{ByteSource, MemorySource, ReaderSource};

/// One SPK segment summary.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    /// Segment identifier from the name record, e.g. `DE-0440LE-0440`.
    pub name: String,
    /// NAIF ID of the body whose state the segment gives.
    pub target: i32,
    /// NAIF ID of the body it is relative to.
    pub center: i32,
    /// NAIF ID of the reference frame (1 = J2000/ICRF).
    pub frame: i32,
    /// SPK data type (2 and 3 are evaluated by this crate).
    pub data_type: i32,
    /// Start of coverage, TDB seconds past J2000.
    pub start_et: f64,
    /// End of coverage, TDB seconds past J2000.
    pub end_et: f64,
    /// First double-precision word address of the segment data (1-based).
    pub start_addr: u32,
    /// Last double-precision word address of the segment data (1-based).
    pub end_addr: u32,
}

impl Segment {
    /// Whether the segment's coverage includes `et` (endpoints inclusive).
    pub fn covers(&self, et: f64) -> bool {
        self.start_et <= et && et <= self.end_et
    }
}

/// A position (km) and velocity (km/s).
pub type StateVector = ([f64; 3], [f64; 3]);

/// Records cached per segment. Consecutive queries at nearby epochs (a time
/// scan, or the same epoch for every point of a map grid) hit one or two
/// records, so a few slots suffice.
const CACHE_SLOTS: usize = 4;

/// The directory at the end of a type 2/3 segment (`SPKR02`).
#[derive(Clone, Copy, Debug)]
struct ChebDirectory {
    /// Start of the first record's interval (ET).
    init: f64,
    /// Length of each record's interval (s).
    intlen: f64,
    /// Doubles per record.
    rsize: usize,
    /// Number of records.
    n: usize,
    /// Coefficients per component.
    ncoef: usize,
}

#[derive(Default)]
struct SegmentCache {
    dir: Option<ChebDirectory>,
    records: [Option<(usize, Box<[f64]>)>; CACHE_SLOTS],
    next_slot: usize,
}

struct Inner<'a> {
    src: Box<dyn ByteSource + Send + 'a>,
    caches: Vec<SegmentCache>,
}

/// An open SPK file.
///
/// Opening reads only the file record and the summary/name records; data
/// records are read on demand and cached, so opening a 114 MB DE440 costs a
/// few kilobytes of I/O. Evaluation takes `&self` (the source and cache sit
/// behind a mutex), so an `Spk<'static>` can be shared across threads.
///
/// ```no_run
/// use mooncalc_spk::{Spk, naif, time};
/// # fn main() -> Result<(), mooncalc_spk::SpkError> {
/// let spk = Spk::open("ephemeris/de440s.bsp")?;
/// let et = time::jd_tdb_to_et(2_460_000.5);
/// let moon = spk.position(naif::MOON, naif::EARTH, et)?; // km, ICRF
/// # Ok(()) }
/// ```
pub struct Spk<'a> {
    file_record: FileRecord,
    segments: Vec<Segment>,
    inner: Mutex<Inner<'a>>,
}

impl std::fmt::Debug for Spk<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spk")
            .field("file_record", &self.file_record)
            .field("segments", &self.segments)
            .finish_non_exhaustive()
    }
}

impl<'a> Spk<'a> {
    /// Parse an SPK from any random-access byte source.
    pub fn from_source(source: impl ByteSource + Send + 'a) -> Result<Self, SpkError> {
        let mut src: Box<dyn ByteSource + Send + 'a> = Box::new(source);
        let rec = daf::read_record(src.as_mut(), 1)?;
        let file_record = daf::parse_file_record(&rec)?;

        let is_spk_word = file_record.id_word == "DAF/SPK" || file_record.id_word == "NAIF/DAF";
        if !is_spk_word || file_record.nd != 2 || file_record.ni != 6 {
            return Err(SpkError::NotSpk {
                id_word: file_record.id_word.clone(),
                nd: file_record.nd,
                ni: file_record.ni,
            });
        }

        let segments = daf::read_summaries(src.as_mut(), &file_record)?
            .into_iter()
            .map(|s| {
                let addr = |v: i32| {
                    u32::try_from(v)
                        .ok()
                        .filter(|&a| a > 0)
                        .ok_or_else(|| SpkError::Corrupt(format!("bad segment address {v}")))
                };
                let seg = Segment {
                    target: s.ints[0],
                    center: s.ints[1],
                    frame: s.ints[2],
                    data_type: s.ints[3],
                    start_et: s.doubles[0],
                    end_et: s.doubles[1],
                    start_addr: addr(s.ints[4])?,
                    end_addr: addr(s.ints[5])?,
                    name: s.name,
                };
                if seg.end_addr < seg.start_addr {
                    return Err(SpkError::Corrupt(format!(
                        "segment {:?} ends before it starts",
                        seg.name
                    )));
                }
                Ok(seg)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let caches = segments.iter().map(|_| SegmentCache::default()).collect();
        Ok(Self {
            file_record,
            segments,
            inner: Mutex::new(Inner { src, caches }),
        })
    }

    /// Parse an SPK from a `Read + Seek` stream such as a [`std::fs::File`].
    pub fn from_reader<R>(reader: R) -> Result<Self, SpkError>
    where
        R: std::io::Read + std::io::Seek + Send + 'a,
    {
        Self::from_source(ReaderSource(reader))
    }

    /// Parse an SPK held in memory: `&[u8]`, `Vec<u8>`, `Arc<[u8]>`, ...
    pub fn from_bytes<T>(bytes: T) -> Result<Self, SpkError>
    where
        T: AsRef<[u8]> + Send + 'a,
    {
        Self::from_source(MemorySource(bytes))
    }

    /// The DAF file record.
    pub fn file_record(&self) -> &FileRecord {
        &self.file_record
    }

    /// Byte order of the file.
    pub fn byte_order(&self) -> ByteOrder {
        self.file_record.byte_order
    }

    /// Every segment, in file order.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// The span from the earliest start to the latest end of the segments
    /// for `target`, or `None` if it has none. Assumes that span has no gaps,
    /// which holds for the JPL DE files (one segment per body).
    pub fn coverage(&self, target: i32) -> Option<(f64, f64)> {
        self.segments
            .iter()
            .filter(|s| s.target == target)
            .map(|s| (s.start_et, s.end_et))
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
    }

    /// The intersection of [`Spk::coverage`] over `bodies`, or `None` if a
    /// body has no segments or the spans do not overlap. The solar system
    /// barycentre (0) is the root of the tree and needs no segment.
    pub fn common_coverage(&self, bodies: &[i32]) -> Option<(f64, f64)> {
        let mut out = (f64::NEG_INFINITY, f64::INFINITY);
        for &b in bodies {
            if b == naif::SOLAR_SYSTEM_BARYCENTER {
                continue;
            }
            let (s, e) = self.coverage(b)?;
            out = (out.0.max(s), out.1.min(e));
        }
        (out.0 <= out.1 && out.0.is_finite()).then_some(out)
    }

    /// The segment that applies to `target` at `et`: the last one in the
    /// file whose coverage includes `et` (later segments take precedence, as
    /// in SPICE's `SPKSFS`).
    pub fn find_segment(&self, target: i32, et: f64) -> Option<usize> {
        self.segments
            .iter()
            .rposition(|s| s.target == target && s.covers(et))
    }

    /// Evaluate one segment at `et`: position (km) and velocity (km/s) of
    /// its target relative to its center, in its frame. `et` may lie
    /// slightly outside the segment's coverage; the nearest record is then
    /// extrapolated, as SPICE does.
    pub fn segment_state(&self, index: usize, et: f64) -> Result<StateVector, SpkError> {
        let mut inner = self.lock();
        self.eval(&mut inner, index, et, true)
    }

    /// Geometric position of `target` relative to `center` at `et` (TDB
    /// seconds past J2000), in km in the J2000/ICRF frame, chaining through
    /// the segment tree (for example Moon 301 → Earth–Moon barycentre 3 ←
    /// Earth 399).
    pub fn position(&self, target: i32, center: i32, et: f64) -> Result<[f64; 3], SpkError> {
        self.chain(target, center, et, false).map(|s| s.0)
    }

    /// Geometric position (km) and velocity (km/s) of `target` relative to
    /// `center` at `et`, in the J2000/ICRF frame. See [`Spk::position`].
    pub fn state(&self, target: i32, center: i32, et: f64) -> Result<StateVector, SpkError> {
        self.chain(target, center, et, true)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner<'a>> {
        // The cache stays consistent even if a panic interrupted a holder.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Segment indices leading from `body` toward the root at `et`, with
    /// the body reached after each step.
    fn path_to_root(&self, body: i32, et: f64) -> Result<Vec<(usize, i32)>, SpkError> {
        let mut path = Vec::new();
        let mut b = body;
        while let Some(i) = self.find_segment(b, et) {
            b = self.segments[i].center;
            path.push((i, b));
            if path.len() > 64 {
                return Err(SpkError::Corrupt(format!(
                    "segment chain from body {body} does not terminate"
                )));
            }
        }
        Ok(path)
    }

    fn chain(
        &self,
        target: i32,
        center: i32,
        et: f64,
        want_velocity: bool,
    ) -> Result<StateVector, SpkError> {
        let zero = ([0.0; 3], [0.0; 3]);
        if target == center {
            return Ok(zero);
        }
        let up_t = self.path_to_root(target, et)?;
        let up_c = self.path_to_root(center, et)?;

        // Bodies visited from each end, starting with the end itself.
        let bodies_t: Vec<i32> = std::iter::once(target)
            .chain(up_t.iter().map(|p| p.1))
            .collect();
        let bodies_c: Vec<i32> = std::iter::once(center)
            .chain(up_c.iter().map(|p| p.1))
            .collect();

        let Some((nt, nc)) = bodies_t
            .iter()
            .enumerate()
            .find_map(|(i, b)| bodies_c.iter().position(|c| c == b).map(|j| (i, j)))
        else {
            for (body, up) in [(target, &up_t), (center, &up_c)] {
                if up.is_empty() && self.coverage(body).is_some() {
                    return Err(SpkError::NoCoverage { body, et });
                }
            }
            return Err(SpkError::NoPath { target, center, et });
        };

        let mut inner = self.lock();
        let mut acc = zero;
        for (steps, sign) in [(&up_t[..nt], 1.0), (&up_c[..nc], -1.0)] {
            for &(i, _) in steps {
                let (p, v) = self.eval(&mut inner, i, et, want_velocity)?;
                for k in 0..3 {
                    acc.0[k] += sign * p[k];
                    acc.1[k] += sign * v[k];
                }
            }
        }
        Ok(acc)
    }

    fn eval(
        &self,
        inner: &mut Inner<'a>,
        index: usize,
        et: f64,
        want_velocity: bool,
    ) -> Result<StateVector, SpkError> {
        let seg = self
            .segments
            .get(index)
            .ok_or_else(|| SpkError::Corrupt(format!("segment index {index} out of range")))?;
        let components = match seg.data_type {
            2 => 3,
            3 => 6,
            t => {
                return Err(SpkError::UnsupportedType {
                    segment: index,
                    data_type: t,
                });
            }
        };
        if seg.frame != naif::FRAME_J2000 {
            return Err(SpkError::UnsupportedFrame {
                segment: index,
                frame: seg.frame,
            });
        }
        let order = self.file_record.byte_order;
        let Inner { src, caches } = inner;
        let cache = &mut caches[index];

        let dir = match cache.dir {
            Some(d) => d,
            None => {
                let d = read_directory(src.as_mut(), order, seg, components)?;
                cache.dir = Some(d);
                d
            }
        };

        // SPKR02: record = int((et - init) / intlen), clamped to the table.
        let r = ((et - dir.init) / dir.intlen).floor();
        let recno = if r.is_nan() || r < 0.0 {
            0
        } else {
            (r as usize).min(dir.n - 1)
        };

        let slot = match cache
            .records
            .iter()
            .position(|s| matches!(s, Some((n, _)) if *n == recno))
        {
            Some(slot) => slot,
            None => {
                let addr = seg.start_addr as usize + recno * dir.rsize;
                let data = daf::read_doubles(src.as_mut(), order, addr as u32, dir.rsize)?;
                let slot = cache.next_slot;
                cache.records[slot] = Some((recno, data.into_boxed_slice()));
                cache.next_slot = (slot + 1) % CACHE_SLOTS;
                slot
            }
        };
        let rec = &cache.records[slot].as_ref().expect("slot just filled").1;

        let (mid, radius) = (rec[0], rec[1]);
        if radius.is_nan() || radius <= 0.0 {
            return Err(SpkError::Corrupt(format!(
                "record {recno} of segment {index} has radius {radius}"
            )));
        }
        let s = (et - mid) / radius;
        let coefs = |c: usize| &rec[2 + c * dir.ncoef..2 + (c + 1) * dir.ncoef];

        let mut pos = [0.0; 3];
        let mut vel = [0.0; 3];
        for k in 0..3 {
            if components == 3 && want_velocity {
                let (p, dp) = cheb::value_and_derivative(coefs(k), s);
                pos[k] = p;
                vel[k] = dp / radius;
            } else {
                pos[k] = cheb::value(coefs(k), s);
                if components == 6 && want_velocity {
                    vel[k] = cheb::value(coefs(k + 3), s);
                }
            }
        }
        Ok((pos, vel))
    }
}

fn read_directory(
    src: &mut dyn ByteSource,
    order: ByteOrder,
    seg: &Segment,
    components: usize,
) -> Result<ChebDirectory, SpkError> {
    let len = (seg.end_addr - seg.start_addr + 1) as usize;
    if len < 4 {
        return Err(SpkError::Corrupt(format!(
            "segment {:?} is too short",
            seg.name
        )));
    }
    let d = daf::read_doubles(src, order, seg.end_addr - 3, 4)?;
    let (init, intlen, rsize, n) = (d[0], d[1], d[2], d[3]);
    let bad = |what: &str| SpkError::Corrupt(format!("segment {:?}: {what}", seg.name));
    let whole = |x: f64| x >= 1.0 && x.fract() == 0.0 && x < 1e12;
    if intlen.is_nan() || intlen <= 0.0 || !init.is_finite() {
        return Err(bad("invalid INIT/INTLEN"));
    }
    if !whole(rsize) || !whole(n) {
        return Err(bad("invalid RSIZE/N"));
    }
    let (rsize, n) = (rsize as usize, n as usize);
    if rsize < 2 + components || (rsize - 2) % components != 0 {
        return Err(bad("RSIZE does not fit the data type"));
    }
    if n.checked_mul(rsize).and_then(|x| x.checked_add(4)) != Some(len) {
        return Err(bad("record count does not match the segment length"));
    }
    Ok(ChebDirectory {
        init,
        intlen,
        rsize,
        n,
        ncoef: (rsize - 2) / components,
    })
}

#[cfg(not(target_arch = "wasm32"))]
impl Spk<'static> {
    /// Open an SPK file by path. Data records are read from the file on
    /// demand.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, SpkError> {
        Self::from_reader(std::fs::File::open(path)?)
    }

    /// Read an entire SPK file into memory and parse it. Faster for heavy
    /// random access, at the cost of the file's size in RAM.
    pub fn open_in_memory(path: impl AsRef<std::path::Path>) -> Result<Self, SpkError> {
        Self::from_bytes(std::fs::read(path)?)
    }
}
