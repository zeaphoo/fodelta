//! Lower-case hexadecimal encoding used for object ids.

use std::fmt::Write as _;

pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing to a String cannot fail.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn decode(input: &str) -> Option<Vec<u8>> {
    if !input.len().is_multiple_of(2) {
        return None;
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(input.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let data = [0u8, 1, 15, 16, 127, 128, 255];
        assert_eq!(decode(&encode(&data)).unwrap(), data);
    }

    #[test]
    fn rejects_odd_length_and_non_hex() {
        assert!(decode("abc").is_none());
        assert!(decode("zz").is_none());
    }
}
