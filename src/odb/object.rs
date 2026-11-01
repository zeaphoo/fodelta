//! Object framing and content addressing.
//!
//! Every object is encoded as `<type> <length>\n<payload>` and addressed by the
//! BLAKE3 hash of those bytes. The header binds the type to the payload so a
//! tree can never be read as a blob.

use std::fmt;
use std::str::FromStr;

use crate::util::hex;
use crate::{FodeltaError, Result};

pub const OID_LEN: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId([u8; OID_LEN]);

impl ObjectId {
    pub const ZERO: Self = Self([0u8; OID_LEN]);

    pub const fn from_bytes(bytes: [u8; OID_LEN]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; OID_LEN] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        hex::encode(&self.0)
    }

    /// Ten character abbreviation used in human readable output.
    pub fn short(self) -> String {
        self.to_hex()[..10].to_string()
    }

    pub fn from_hex(input: &str) -> Result<Self> {
        if input.len() != OID_LEN * 2 {
            return Err(FodeltaError::InvalidObject {
                reason: format!(
                    "object id must be {} hex characters, got {}",
                    OID_LEN * 2,
                    input.len()
                ),
            });
        }
        let bytes = hex::decode(input).ok_or_else(|| FodeltaError::InvalidObject {
            reason: format!("object id is not valid hex: {input}"),
        })?;
        let mut array = [0u8; OID_LEN];
        array.copy_from_slice(&bytes);
        Ok(Self(array))
    }

    /// Resolve a short hex prefix against a candidate set.
    pub fn resolve_prefix(prefix: &str, candidates: &[ObjectId]) -> Result<Self> {
        if prefix.len() < 6 || prefix.len() > OID_LEN * 2 {
            return Err(FodeltaError::InvalidObject {
                reason: format!("object id prefix must be 6-64 hex characters: {prefix}"),
            });
        }
        let lower = prefix.to_ascii_lowercase();
        if !lower.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(FodeltaError::InvalidObject {
                reason: format!("object id prefix is not valid hex: {prefix}"),
            });
        }
        let matches: Vec<ObjectId> = candidates
            .iter()
            .copied()
            .filter(|oid| oid.to_hex().starts_with(&lower))
            .collect();
        match matches.as_slice() {
            [oid] => Ok(*oid),
            [] => Err(FodeltaError::InvalidObject {
                reason: format!("no object matches prefix {prefix}"),
            }),
            many => Err(FodeltaError::InvalidObject {
                reason: format!(
                    "ambiguous object prefix {prefix}: {}",
                    many.iter()
                        .map(|oid| oid.short())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }),
        }
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.short())
    }
}

impl FromStr for ObjectId {
    type Err = FodeltaError;

    fn from_str(s: &str) -> Result<Self> {
        Self::from_hex(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Blob,
    Tree,
    Commit,
}

impl ObjectKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blob => "blob",
            Self::Tree => "tree",
            Self::Commit => "commit",
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes {
            b"blob" => Some(Self::Blob),
            b"tree" => Some(Self::Tree),
            b"commit" => Some(Self::Commit),
            _ => None,
        }
    }
}

pub fn encode(kind: ObjectKind, payload: &[u8]) -> Vec<u8> {
    let header = format!("{} {}\n", kind.as_str(), payload.len());
    let mut bytes = Vec::with_capacity(header.len() + payload.len());
    bytes.extend_from_slice(header.as_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

pub fn decode(bytes: &[u8]) -> Result<(ObjectKind, Vec<u8>)> {
    let newline = bytes
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or_else(|| FodeltaError::InvalidObject {
            reason: "object header is missing a newline".to_string(),
        })?;
    let header = &bytes[..newline];
    let separator = header
        .iter()
        .position(|byte| *byte == b' ')
        .ok_or_else(|| FodeltaError::InvalidObject {
            reason: "object header is missing a type separator".to_string(),
        })?;
    let (kind_bytes, len_bytes) = header.split_at(separator);
    let len_bytes = &len_bytes[1..];
    let kind = ObjectKind::from_bytes(kind_bytes).ok_or_else(|| FodeltaError::InvalidObject {
        reason: format!(
            "unknown object type '{}'",
            String::from_utf8_lossy(kind_bytes)
        ),
    })?;
    let len_text = std::str::from_utf8(len_bytes).map_err(|_| FodeltaError::InvalidObject {
        reason: "object length is not valid UTF-8".to_string(),
    })?;
    let len: usize = len_text.parse().map_err(|_| FodeltaError::InvalidObject {
        reason: format!("object length is not a number: {len_text}"),
    })?;
    let payload = &bytes[newline + 1..];
    if payload.len() != len {
        return Err(FodeltaError::InvalidObject {
            reason: format!(
                "object length mismatch: header says {len}, payload is {}",
                payload.len()
            ),
        });
    }
    Ok((kind, payload.to_vec()))
}

pub fn hash(bytes: &[u8]) -> ObjectId {
    ObjectId::from_bytes(*blake3::hash(bytes).as_bytes())
}

/// Hash an in-memory blob without writing it.
pub fn blob_hash(payload: &[u8]) -> ObjectId {
    hash(&encode(ObjectKind::Blob, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_roundtrip() {
        let payload = b"hello";
        let bytes = encode(ObjectKind::Blob, payload);
        assert!(bytes.starts_with(b"blob 5\n"));
        let (kind, decoded) = decode(&bytes).unwrap();
        assert_eq!(kind, ObjectKind::Blob);
        assert_eq!(decoded, payload);
    }

    #[test]
    fn rejects_type_and_length_mismatch() {
        assert!(decode(b"nope 1\nx").is_err());
        assert!(decode(b"blob 2\nx").is_err());
        assert!(decode(b"blob 1").is_err());
        assert!(decode(b"blob abc\nx").is_err());
    }

    #[test]
    fn object_id_hex_roundtrip() {
        let oid = ObjectId::from_bytes([0xab; 32]);
        assert_eq!(ObjectId::from_hex(&oid.to_hex()).unwrap(), oid);
        assert_eq!(oid.short().len(), 10);
    }

    #[test]
    fn prefix_resolution_rejects_ambiguity() {
        let a = ObjectId::from_bytes([0x11; 32]);
        let b = ObjectId::from_bytes([0x11; 32]);
        let err = ObjectId::resolve_prefix("111111", &[a, b]).unwrap_err();
        assert!(err.to_string().contains("ambiguous"));
    }
}
