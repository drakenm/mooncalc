//! Test helpers: an independent minimal SPK writer (after *DAF Required
//! Reading* and *SPK Required Reading*) and a byte-counting reader.

#![allow(dead_code)]

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A type 2 or 3 segment to write.
pub struct SegSpec {
    pub name: &'static str,
    pub target: i32,
    pub center: i32,
    pub frame: i32,
    pub data_type: i32,
    pub start_et: f64,
    pub end_et: f64,
    pub init: f64,
    pub intlen: f64,
    /// Each record: `[mid, radius, coefficients...]`, all the same length.
    pub records: Vec<Vec<f64>>,
}

struct Writer {
    big: bool,
    buf: Vec<u8>,
}

impl Writer {
    fn f64_at(&mut self, at: usize, v: f64) {
        let b = if self.big {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        };
        self.buf[at..at + 8].copy_from_slice(&b);
    }
    fn i32_at(&mut self, at: usize, v: i32) {
        let b = if self.big {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        };
        self.buf[at..at + 4].copy_from_slice(&b);
    }
}

const REC: usize = 1024;

/// Build an SPK image. Summaries are split into summary records holding at
/// most `per_record` entries each, so small values exercise the linked list.
pub fn build_spk(segs: &[SegSpec], big: bool, per_record: usize) -> Vec<u8> {
    let groups = segs.len().div_ceil(per_record).max(1);
    // Record 1: file record; then (summary, name) pairs; then data.
    let data_start_rec = 2 + 2 * groups;
    let mut w = Writer {
        big,
        buf: vec![0u8; (data_start_rec - 1) * REC],
    };

    // Data arrays.
    let mut addrs = Vec::new();
    for s in segs {
        let start_addr = w.buf.len() / 8 + 1;
        let rsize = s.records[0].len();
        let mut words: Vec<f64> = s.records.iter().flatten().copied().collect();
        words.extend([s.init, s.intlen, rsize as f64, s.records.len() as f64]);
        for v in words {
            let at = w.buf.len();
            w.buf.extend([0u8; 8]);
            w.f64_at(at, v);
        }
        addrs.push((start_addr as i32, (w.buf.len() / 8) as i32));
    }
    let free = w.buf.len() / 8 + 1;
    // Pad to a whole record.
    w.buf.resize(w.buf.len().div_ceil(REC) * REC, 0);

    // File record.
    w.buf[0..8].copy_from_slice(b"DAF/SPK ");
    w.i32_at(8, 2);
    w.i32_at(12, 6);
    w.buf[16..76].fill(b' ');
    w.buf[16..27].copy_from_slice(b"SYNTHETIC-1");
    w.i32_at(76, 2);
    w.i32_at(80, (2 + 2 * (groups - 1)) as i32);
    w.i32_at(84, free as i32);
    w.buf[88..96].copy_from_slice(if big { b"BIG-IEEE" } else { b"LTL-IEEE" });
    w.buf[699..727].copy_from_slice(b"FTPSTR:\r:\n:\r\n:\r\x00:\x81:\x10\xce:ENDFTP");

    // Summary and name records.
    for g in 0..groups {
        let srec = (2 + 2 * g - 1) * REC;
        let nrec = srec + REC;
        let next = if g + 1 < groups { 2 + 2 * (g + 1) } else { 0 };
        let prev = if g > 0 { 2 + 2 * (g - 1) } else { 0 };
        let chunk: Vec<usize> = (g * per_record..((g + 1) * per_record).min(segs.len())).collect();
        w.f64_at(srec, next as f64);
        w.f64_at(srec + 8, prev as f64);
        w.f64_at(srec + 16, chunk.len() as f64);
        w.buf[nrec..nrec + REC].fill(b' ');
        for (k, &i) in chunk.iter().enumerate() {
            let s = &segs[i];
            let base = srec + 24 + k * 40;
            w.f64_at(base, s.start_et);
            w.f64_at(base + 8, s.end_et);
            let ints = [
                s.target,
                s.center,
                s.frame,
                s.data_type,
                addrs[i].0,
                addrs[i].1,
            ];
            for (j, v) in ints.into_iter().enumerate() {
                w.i32_at(base + 16 + 4 * j, v);
            }
            let name = s.name.as_bytes();
            w.buf[nrec + k * 40..nrec + k * 40 + name.len()].copy_from_slice(name);
        }
    }
    w.buf
}

/// A `Read + Seek` wrapper that counts the bytes read through it.
pub struct CountingReader<R> {
    pub inner: R,
    pub count: Arc<AtomicU64>,
}

impl<R> CountingReader<R> {
    pub fn new(inner: R) -> (Self, Arc<AtomicU64>) {
        let count = Arc::new(AtomicU64::new(0));
        (
            Self {
                inner,
                count: count.clone(),
            },
            count,
        )
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

impl<R: Seek> Seek for CountingReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}
