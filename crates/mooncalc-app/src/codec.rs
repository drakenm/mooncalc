//! Base64 encoding for binary payloads (map cells) carried in JSON.
//!
//! RFC 4648 section 4: standard alphabet, `=` padding. Hand-written to avoid a
//! dependency for a few dozen lines; the TypeScript side decodes with `atob`.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode `bytes` as padded standard base64.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = Vec::with_capacity(bytes.len().div_ceil(3) * 4);
    let (chunks, remainder) = bytes.as_chunks::<3>();
    for c in chunks {
        let n = (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]);
        out.extend_from_slice(&[
            ALPHABET[(n >> 18) as usize & 63],
            ALPHABET[(n >> 12) as usize & 63],
            ALPHABET[(n >> 6) as usize & 63],
            ALPHABET[n as usize & 63],
        ]);
    }
    match *remainder {
        [a] => {
            let n = u32::from(a) << 16;
            out.extend_from_slice(&[
                ALPHABET[(n >> 18) as usize & 63],
                ALPHABET[(n >> 12) as usize & 63],
                b'=',
                b'=',
            ]);
        }
        [a, b] => {
            let n = (u32::from(a) << 16) | (u32::from(b) << 8);
            out.extend_from_slice(&[
                ALPHABET[(n >> 18) as usize & 63],
                ALPHABET[(n >> 12) as usize & 63],
                ALPHABET[(n >> 6) as usize & 63],
                b'=',
            ]);
        }
        _ => {}
    }
    // Every byte pushed comes from ALPHABET or is '=', so this is ASCII.
    String::from_utf8(out).expect("base64 output is ASCII")
}

/// Decode padded standard base64 (whitespace is not allowed). `None` if the
/// input is malformed.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    fn sextet(c: u8) -> Option<u32> {
        ALPHABET.iter().position(|&a| a == c).map(|i| i as u32)
    }
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let quads = bytes.len() / 4;
    for (q, chunk) in bytes.as_chunks::<4>().0.iter().enumerate() {
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && q + 1 != quads) {
            return None;
        }
        let mut n = 0u32;
        for &c in &chunk[..4 - pad] {
            n = (n << 6) | sextet(c)?;
        }
        n <<= 6 * pad as u32;
        let b = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&b[..3 - pad]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test vectors from RFC 4648 section 10.
    #[test]
    fn rfc4648_vectors() {
        let cases = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (input, expected) in cases {
            assert_eq!(base64_encode(input.as_bytes()), expected, "input {input:?}");
            assert_eq!(base64_decode(expected).unwrap(), input.as_bytes());
        }
    }

    #[test]
    fn high_bytes_use_the_full_alphabet() {
        assert_eq!(base64_encode(&[0xff, 0xff, 0xff]), "////");
        assert_eq!(base64_encode(&[0xfb, 0xef, 0xbe]), "++++");
        assert_eq!(base64_encode(&[0, 0, 0, 255]), "AAAA/w==");
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(base64_decode(&base64_encode(&all)).unwrap(), all);
    }

    #[test]
    fn malformed_input_is_rejected() {
        for bad in ["A", "AB=C", "A===", "Zg==Zg==", "Zm9!"] {
            assert!(base64_decode(bad).is_none(), "{bad}");
        }
    }
}
