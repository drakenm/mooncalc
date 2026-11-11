//! NAIF Double precision Array File (DAF) container parsing.
//!
//! Implements the layout described in NAIF's *DAF Required Reading*
//! (`daf.req`, SPICE Toolkit N0067): a file of 1024-byte records, the first
//! of which is the file record; a doubly linked list of summary records, each
//! immediately followed by its name record; and arrays addressed by 1-based
//! double-precision word addresses.

use crate::error::SpkError;
use crate::source::ByteSource;

/// Bytes in one DAF record.
pub const RECORD_BYTES: usize = 1024;
/// Double-precision words in one DAF record.
pub const RECORD_WORDS: usize = RECORD_BYTES / 8;

/// The FTP validation string NAIF writes into every binary DAF since
/// Toolkit N0052. A damaged copy means the file went through a text-mode
/// transfer and its binary contents are corrupt (*DAF Required Reading*,
/// "Detecting Corrupted Files").
const FTP_STRING: &[u8; 28] = b"FTPSTR:\r:\n:\r\n:\r\x00:\x81:\x10\xce:ENDFTP";
const FTP_OFFSET: usize = 699;

/// Byte order of the numbers in a binary DAF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteOrder {
    /// `LTL-IEEE`: little-endian IEEE 754 (PCs, the usual case).
    Little,
    /// `BIG-IEEE`: big-endian IEEE 754.
    Big,
}

impl ByteOrder {
    pub(crate) fn f64(self, b: &[u8]) -> f64 {
        let a: [u8; 8] = b[..8].try_into().expect("8 bytes");
        match self {
            Self::Little => f64::from_le_bytes(a),
            Self::Big => f64::from_be_bytes(a),
        }
    }

    pub(crate) fn i32(self, b: &[u8]) -> i32 {
        let a: [u8; 4] = b[..4].try_into().expect("4 bytes");
        match self {
            Self::Little => i32::from_le_bytes(a),
            Self::Big => i32::from_be_bytes(a),
        }
    }
}

/// The contents of a DAF file record (record 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRecord {
    /// Identification word, e.g. `DAF/SPK` (trailing blanks removed).
    pub id_word: String,
    /// Number of double-precision components in each array summary.
    pub nd: usize,
    /// Number of integer components in each array summary.
    pub ni: usize,
    /// Internal file name (trailing blanks removed), e.g. `NIO2SPK`.
    pub internal_name: String,
    /// Record number of the first summary record.
    pub forward: u32,
    /// Record number of the last summary record.
    pub backward: u32,
    /// First free double-precision word address.
    pub free: u32,
    /// Byte order of the binary data.
    pub byte_order: ByteOrder,
}

impl FileRecord {
    /// Size of one array summary in double-precision words:
    /// `ND + (NI + 1) / 2`.
    pub fn summary_words(&self) -> usize {
        self.nd + self.ni.div_ceil(2)
    }
}

fn ascii_trimmed(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .trim_end_matches([' ', '\0'])
        .to_string()
}

fn plausible_nd_ni(nd: i32, ni: i32) -> bool {
    (0..=124).contains(&nd) && (2..=250).contains(&ni) && nd + (ni + 1) / 2 <= 125
}

/// Parse and validate the file record.
pub(crate) fn parse_file_record(rec: &[u8; RECORD_BYTES]) -> Result<FileRecord, SpkError> {
    let id_word = ascii_trimmed(&rec[0..8]);
    if !(id_word.starts_with("DAF/") || id_word == "NAIF/DAF") {
        return Err(SpkError::NotDaf);
    }

    let fmt = ascii_trimmed(&rec[88..96]);
    let byte_order = match fmt.as_str() {
        "LTL-IEEE" => ByteOrder::Little,
        "BIG-IEEE" => ByteOrder::Big,
        // Files written before the format string existed: infer the order
        // from whichever reading gives sane ND/NI values.
        "" => {
            let le = (
                ByteOrder::Little.i32(&rec[8..]),
                ByteOrder::Little.i32(&rec[12..]),
            );
            let be = (
                ByteOrder::Big.i32(&rec[8..]),
                ByteOrder::Big.i32(&rec[12..]),
            );
            if plausible_nd_ni(le.0, le.1) {
                ByteOrder::Little
            } else if plausible_nd_ni(be.0, be.1) {
                ByteOrder::Big
            } else {
                return Err(SpkError::Corrupt(
                    "cannot determine the byte order from ND/NI".into(),
                ));
            }
        }
        other => return Err(SpkError::UnsupportedFormat(other.to_string())),
    };

    let nd = byte_order.i32(&rec[8..]);
    let ni = byte_order.i32(&rec[12..]);
    if !plausible_nd_ni(nd, ni) {
        return Err(SpkError::Corrupt(format!("invalid ND={nd}, NI={ni}")));
    }

    let ftp = &rec[FTP_OFFSET..FTP_OFFSET + FTP_STRING.len()];
    if ftp.starts_with(b"FTPSTR:") && ftp != FTP_STRING {
        return Err(SpkError::Corrupt(
            "FTP validation string is damaged (was the file transferred in text mode?)".into(),
        ));
    }

    let word = |at: usize| -> Result<u32, SpkError> {
        u32::try_from(byte_order.i32(&rec[at..]))
            .map_err(|_| SpkError::Corrupt(format!("negative record pointer at byte {at}")))
    };

    Ok(FileRecord {
        id_word,
        nd: nd as usize,
        ni: ni as usize,
        internal_name: ascii_trimmed(&rec[16..76]),
        forward: word(76)?,
        backward: word(80)?,
        free: word(84)?,
        byte_order,
    })
}

/// One array summary with its name, as stored in the file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RawSummary {
    pub doubles: Vec<f64>,
    pub ints: Vec<i32>,
    pub name: String,
}

/// Read one 1024-byte record (record numbers are 1-based).
pub(crate) fn read_record(
    src: &mut dyn ByteSource,
    recno: u32,
) -> Result<[u8; RECORD_BYTES], SpkError> {
    if recno == 0 {
        return Err(SpkError::Corrupt("record number 0".into()));
    }
    let mut buf = [0u8; RECORD_BYTES];
    src.read_exact_at(u64::from(recno - 1) * RECORD_BYTES as u64, &mut buf)?;
    Ok(buf)
}

/// Walk the summary-record list and collect every array summary in file
/// order. Only summary and name records are read.
pub(crate) fn read_summaries(
    src: &mut dyn ByteSource,
    fr: &FileRecord,
) -> Result<Vec<RawSummary>, SpkError> {
    let ss = fr.summary_words();
    let nc = 8 * ss;
    let max_per_record = (RECORD_WORDS - 3) / ss;
    let order = fr.byte_order;

    let mut out = Vec::new();
    let mut recno = fr.forward;
    let mut visited = 0usize;
    while recno != 0 {
        visited += 1;
        // Far more summary records than any real file has: assume a cycle.
        if visited > 1_000_000 {
            return Err(SpkError::Corrupt("summary record list has a cycle".into()));
        }
        let srec = read_record(src, recno)?;
        let nrec = read_record(src, recno + 1)?;

        let next = order.f64(&srec[0..]);
        let nsum = order.f64(&srec[16..]);
        if !(next >= 0.0 && next.fract() == 0.0 && next < f64::from(u32::MAX)) {
            return Err(SpkError::Corrupt(format!(
                "bad next-record pointer {next} in summary record {recno}"
            )));
        }
        if !(nsum >= 0.0 && nsum.fract() == 0.0 && nsum <= max_per_record as f64) {
            return Err(SpkError::Corrupt(format!(
                "bad summary count {nsum} in summary record {recno}"
            )));
        }

        for i in 0..nsum as usize {
            let base = 24 + i * ss * 8;
            let doubles = (0..fr.nd)
                .map(|k| order.f64(&srec[base + 8 * k..]))
                .collect();
            let ibase = base + fr.nd * 8;
            let ints = (0..fr.ni)
                .map(|k| order.i32(&srec[ibase + 4 * k..]))
                .collect();
            let name = ascii_trimmed(&nrec[i * nc..(i + 1) * nc]);
            out.push(RawSummary {
                doubles,
                ints,
                name,
            });
        }
        recno = next as u32;
    }
    Ok(out)
}

/// Read `n` doubles starting at the 1-based word address `addr`.
pub(crate) fn read_doubles(
    src: &mut dyn ByteSource,
    order: ByteOrder,
    addr: u32,
    n: usize,
) -> Result<Vec<f64>, SpkError> {
    if addr == 0 {
        return Err(SpkError::Corrupt("word address 0".into()));
    }
    let mut buf = vec![0u8; n * 8];
    src.read_exact_at(u64::from(addr - 1) * 8, &mut buf)?;
    Ok(buf
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| order.f64(c))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ftp_string_fills_bytes_699_to_727() {
        assert_eq!(FTP_OFFSET + FTP_STRING.len(), 727);
    }

    #[test]
    fn rejects_non_daf() {
        let rec = [b' '; RECORD_BYTES];
        assert!(matches!(parse_file_record(&rec), Err(SpkError::NotDaf)));
    }

    #[test]
    fn infers_byte_order_without_format_string() {
        for big in [false, true] {
            let mut rec = [0u8; RECORD_BYTES];
            rec[0..8].copy_from_slice(b"DAF/SPK ");
            let put = |rec: &mut [u8; RECORD_BYTES], at: usize, v: i32| {
                let b = if big {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                };
                rec[at..at + 4].copy_from_slice(&b);
            };
            put(&mut rec, 8, 2);
            put(&mut rec, 12, 6);
            put(&mut rec, 76, 2);
            put(&mut rec, 80, 2);
            put(&mut rec, 84, 300);
            let fr = parse_file_record(&rec).unwrap();
            let want = if big {
                ByteOrder::Big
            } else {
                ByteOrder::Little
            };
            assert_eq!(fr.byte_order, want);
            assert_eq!((fr.nd, fr.ni, fr.forward, fr.free), (2, 6, 2, 300));
        }
    }

    #[test]
    fn rejects_vax_format() {
        let mut rec = [0u8; RECORD_BYTES];
        rec[0..8].copy_from_slice(b"DAF/SPK ");
        rec[88..96].copy_from_slice(b"VAX-GFLT");
        assert!(matches!(
            parse_file_record(&rec),
            Err(SpkError::UnsupportedFormat(_))
        ));
    }
}
