//! Random-access byte sources the DAF reader pulls records from.
//!
//! The reader never needs the whole file: it reads the file record, the
//! summary and name records, and then individual data records on demand.
//! Anything that can serve "these bytes at this offset" works, so the same
//! parser runs over a file, any `Read + Seek` stream, or a buffer already in
//! memory (the planned WASM build fetches the kernel into memory).

use std::io::{self, Read, Seek, SeekFrom};

/// A random-access source of bytes.
pub trait ByteSource {
    /// Fill `buf` with the bytes starting at `offset`. A source that ends
    /// before `buf` is full returns [`io::ErrorKind::UnexpectedEof`].
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()>;
}

/// Adapts any `Read + Seek` stream (a [`std::fs::File`], an
/// [`io::Cursor`], ...) to [`ByteSource`].
#[derive(Debug)]
pub struct ReaderSource<R>(pub R);

impl<R: Read + Seek> ByteSource for ReaderSource<R> {
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        self.0.seek(SeekFrom::Start(offset))?;
        self.0.read_exact(buf)
    }
}

/// Serves bytes from memory: a `&[u8]`, `Vec<u8>`, `Arc<[u8]>`, ...
#[derive(Debug)]
pub struct MemorySource<T>(pub T);

impl<T: AsRef<[u8]>> ByteSource for MemorySource<T> {
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let data = self.0.as_ref();
        let start = usize::try_from(offset).ok().filter(|&s| s <= data.len());
        match start.and_then(|s| data.get(s..s.checked_add(buf.len())?)) {
            Some(src) => {
                buf.copy_from_slice(src);
                Ok(())
            }
            None => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "read past the end of the in-memory SPK data",
            )),
        }
    }
}
