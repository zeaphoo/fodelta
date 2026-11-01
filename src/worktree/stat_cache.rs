//! `.fodelta/stat-cache`: a pure cache of file metadata to object ids.
//!
//! The cache never decides what a commit contains; it only lets `status` and
//! `diff` skip re-hashing files whose metadata is unchanged.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::odb::ObjectId;
use crate::util::atomic::write_atomic;
use crate::{FodeltaError, Result};

const MAGIC: &[u8; 8] = b"FDLTSTAT";
const VERSION: u16 = 1;
const CHECKSUM_LEN: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStat {
    pub size: u64,
    pub mtime_sec: i64,
    pub mtime_nsec: u32,
    pub inode: u64,
    pub mode: u32,
}

impl FileStat {
    fn mtime_ns(self) -> u64 {
        let seconds = u64::try_from(self.mtime_sec).unwrap_or(0);
        seconds
            .saturating_mul(1_000_000_000)
            .saturating_add(u64::from(self.mtime_nsec))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedStat {
    oid: ObjectId,
    stat: FileStat,
}

#[derive(Debug, Default, Clone)]
pub struct StatCache {
    written_at_ns: u64,
    entries: BTreeMap<Vec<u8>, CachedStat>,
}

impl StatCache {
    pub fn load(path: &Path) -> Self {
        Self::try_load(path).unwrap_or_default()
    }

    pub fn try_load(path: &Path) -> Result<Self> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.into()),
        };
        Self::decode(&bytes)
    }

    pub fn save(&mut self, path: &Path) -> Result<()> {
        self.written_at_ns = now_ns();
        let bytes = self.encode()?;
        write_atomic(path, &bytes, true)?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return the cached object id if the file metadata still matches and the
    /// timestamp is not inside the racy window.
    pub fn lookup(&self, path: &[u8], stat: FileStat) -> Option<ObjectId> {
        let cached = self.entries.get(path)?;
        if cached.stat != stat {
            return None;
        }
        if stat.mtime_ns() >= self.written_at_ns {
            return None;
        }
        Some(cached.oid)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn insert(&mut self, path: Vec<u8>, stat: FileStat, oid: ObjectId) {
        self.entries.insert(path, CachedStat { oid, stat });
    }

    pub fn remove(&mut self, path: &[u8]) {
        self.entries.remove(path);
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&[u8]) -> bool) {
        self.entries.retain(|path, _| keep(path));
    }

    pub fn entries(&self) -> impl Iterator<Item = (&[u8], ObjectId)> {
        self.entries
            .iter()
            .map(|(path, cached)| (path.as_slice(), cached.oid))
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let count = u32::try_from(self.entries.len())
            .map_err(|_| FodeltaError::Config("stat-cache has too many entries".to_string()))?;
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&self.written_at_ns.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        for (path, cached) in &self.entries {
            let len = u16::try_from(path.len())
                .map_err(|_| FodeltaError::Config("stat-cache path is too long".to_string()))?;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(path);
            out.extend_from_slice(cached.oid.as_bytes());
            out.extend_from_slice(&cached.stat.size.to_le_bytes());
            out.extend_from_slice(&cached.stat.mtime_sec.to_le_bytes());
            out.extend_from_slice(&cached.stat.mtime_nsec.to_le_bytes());
            out.extend_from_slice(&cached.stat.inode.to_le_bytes());
            out.extend_from_slice(&cached.stat.mode.to_le_bytes());
            out.push(0);
        }
        let checksum = blake3::hash(&out);
        out.extend_from_slice(&checksum.as_bytes()[..CHECKSUM_LEN]);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < MAGIC.len() + 2 + 8 + 4 + CHECKSUM_LEN {
            return Err(corrupt("stat-cache is too short"));
        }
        let (body, checksum) = bytes.split_at(bytes.len() - CHECKSUM_LEN);
        if blake3::hash(body).as_bytes()[..CHECKSUM_LEN] != checksum[..] {
            return Err(corrupt("stat-cache checksum mismatch"));
        }
        let mut cursor = 0usize;
        if body.len() < MAGIC.len() || &body[..MAGIC.len()] != MAGIC {
            return Err(corrupt("stat-cache magic mismatch"));
        }
        cursor += MAGIC.len();
        let version = read_u16(body, &mut cursor)?;
        if version != VERSION {
            return Err(corrupt(format!("unsupported stat-cache version {version}")));
        }
        let written_at_ns = read_u64(body, &mut cursor)?;
        let count = read_u32(body, &mut cursor)? as usize;
        if count > body.len() / 60 {
            return Err(corrupt("stat-cache entry count exceeds payload"));
        }
        let mut entries = BTreeMap::new();
        let mut previous: Option<Vec<u8>> = None;
        for _ in 0..count {
            let len = read_u16(body, &mut cursor)? as usize;
            let path = body
                .get(cursor..cursor + len)
                .ok_or_else(|| corrupt("stat-cache path is truncated"))?
                .to_vec();
            cursor += len;
            if let Some(prev) = &previous
                && prev >= &path
            {
                return Err(corrupt("stat-cache paths are not strictly sorted"));
            }
            previous = Some(path.clone());
            let oid_bytes = body
                .get(cursor..cursor + 32)
                .ok_or_else(|| corrupt("stat-cache object id is truncated"))?;
            let mut oid = [0u8; 32];
            oid.copy_from_slice(oid_bytes);
            cursor += 32;
            let size = read_u64(body, &mut cursor)?;
            let mtime_sec = read_i64(body, &mut cursor)?;
            let mtime_nsec = read_u32(body, &mut cursor)?;
            let inode = read_u64(body, &mut cursor)?;
            let mode = read_u32(body, &mut cursor)?;
            let flags = *body
                .get(cursor)
                .ok_or_else(|| corrupt("stat-cache flags are truncated"))?;
            cursor += 1;
            if flags != 0 {
                return Err(corrupt("stat-cache has unknown flags"));
            }
            entries.insert(
                path,
                CachedStat {
                    oid: ObjectId::from_bytes(oid),
                    stat: FileStat {
                        size,
                        mtime_sec,
                        mtime_nsec,
                        inode,
                        mode,
                    },
                },
            );
        }
        if cursor != body.len() {
            return Err(corrupt("stat-cache has trailing bytes"));
        }
        Ok(Self {
            written_at_ns,
            entries,
        })
    }
}

fn corrupt(reason: impl Into<String>) -> FodeltaError {
    FodeltaError::RepositoryCorrupt(reason.into())
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0)
}

fn read_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16> {
    let slice = bytes
        .get(*cursor..*cursor + 2)
        .ok_or_else(|| corrupt("stat-cache is truncated"))?;
    *cursor += 2;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32> {
    let slice = bytes
        .get(*cursor..*cursor + 4)
        .ok_or_else(|| corrupt("stat-cache is truncated"))?;
    *cursor += 4;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64> {
    let slice = bytes
        .get(*cursor..*cursor + 8)
        .ok_or_else(|| corrupt("stat-cache is truncated"))?;
    *cursor += 8;
    let mut array = [0u8; 8];
    array.copy_from_slice(slice);
    Ok(u64::from_le_bytes(array))
}

fn read_i64(bytes: &[u8], cursor: &mut usize) -> Result<i64> {
    Ok(read_u64(bytes, cursor)? as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat(mtime_sec: i64) -> FileStat {
        FileStat {
            size: 10,
            mtime_sec,
            mtime_nsec: 0,
            inode: 7,
            mode: 0o100644,
        }
    }

    fn cache() -> StatCache {
        let mut cache = StatCache::default();
        cache.insert(b"a.txt".to_vec(), stat(100), ObjectId::from_bytes([1; 32]));
        cache.written_at_ns = 200 * 1_000_000_000;
        cache
    }

    #[test]
    fn lookup_matches_stat_and_respects_racy_window() {
        let cache = cache();
        assert_eq!(
            cache.lookup(b"a.txt", stat(100)),
            Some(ObjectId::from_bytes([1; 32]))
        );
        assert_eq!(cache.lookup(b"a.txt", stat(101)), None);
        assert_eq!(cache.lookup(b"b.txt", stat(100)), None);
        assert_eq!(cache.lookup(b"a.txt", stat(200)), None);
    }

    #[test]
    fn roundtrip_and_corruption() {
        let cache = cache();
        let bytes = cache.encode().unwrap();
        let decoded = StatCache::decode(&bytes).unwrap();
        assert_eq!(decoded.written_at_ns, cache.written_at_ns);
        assert_eq!(
            decoded.lookup(b"a.txt", stat(100)),
            Some(ObjectId::from_bytes([1; 32]))
        );

        let mut broken = bytes.clone();
        broken[20] ^= 0xff;
        assert!(StatCache::decode(&broken).is_err());

        let truncated = &bytes[..bytes.len() - 1];
        assert!(StatCache::decode(truncated).is_err());
    }

    #[test]
    fn save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stat-cache");
        let mut cache = cache();
        cache.save(&path).unwrap();
        let loaded = StatCache::try_load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(loaded.lookup(b"a.txt", stat(100)).is_some());
    }
}
