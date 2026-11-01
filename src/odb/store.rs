//! Loose object storage: zstd compressed files under `objects/xx/yyyy...`.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::odb::object::{self, OID_LEN, ObjectId, ObjectKind};
use crate::util::atomic::{TempPath, sync_dir, write_atomic_with};
use crate::{FodeltaError, Result};

pub struct ObjectStore {
    root: PathBuf,
    compression: i32,
    max_object_size: u64,
    fsync: bool,
}

impl ObjectStore {
    pub fn new(root: PathBuf, compression: i32, max_object_size: u64, fsync: bool) -> Self {
        Self {
            root,
            compression,
            max_object_size,
            fsync,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path_for(&self, oid: ObjectId) -> PathBuf {
        let hex = oid.to_hex();
        self.root.join(&hex[..2]).join(&hex[2..])
    }

    pub fn exists(&self, oid: ObjectId) -> bool {
        self.path_for(oid).is_file()
    }

    pub fn write(&self, kind: ObjectKind, payload: &[u8]) -> Result<ObjectId> {
        if payload.len() as u64 > self.max_object_size {
            return Err(FodeltaError::DataTooLarge {
                size: payload.len() as u64,
                limit: self.max_object_size,
            });
        }
        let bytes = object::encode(kind, payload);
        let oid = object::hash(&bytes);
        if self.exists(oid) {
            return Ok(oid);
        }

        let path = self.path_for(oid);
        let compression = self.compression;
        write_atomic_with(&path, self.fsync, |file| {
            let mut encoder = zstd::stream::write::Encoder::new(file, compression)?;
            encoder.include_checksum(true)?;
            encoder.write_all(&bytes)?;
            encoder.finish()?;
            Ok(())
        })?;
        Ok(oid)
    }

    pub fn write_blob(&self, payload: &[u8]) -> Result<ObjectId> {
        self.write(ObjectKind::Blob, payload)
    }

    pub fn read(&self, oid: ObjectId) -> Result<(ObjectKind, Vec<u8>)> {
        let path = self.path_for(oid);
        if !path.is_file() {
            return Err(FodeltaError::ObjectNotFound { oid: oid.to_hex() });
        }

        let compressed = fs::read(&path).map_err(|error| FodeltaError::CorruptObject {
            oid: oid.to_hex(),
            reason: format!("cannot read object file: {error}"),
        })?;
        let decoder = zstd::stream::read::Decoder::new(compressed.as_slice()).map_err(|error| {
            FodeltaError::CorruptObject {
                oid: oid.to_hex(),
                reason: format!("cannot start zstd decoder: {error}"),
            }
        })?;
        let mut bytes = Vec::new();
        decoder
            .take(self.max_object_size + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| FodeltaError::CorruptObject {
                oid: oid.to_hex(),
                reason: format!("cannot decompress object: {error}"),
            })?;
        if bytes.len() as u64 > self.max_object_size {
            return Err(FodeltaError::ObjectTooLarge {
                oid: oid.to_hex(),
                limit: self.max_object_size,
            });
        }
        if object::hash(&bytes) != oid {
            return Err(FodeltaError::CorruptObject {
                oid: oid.to_hex(),
                reason: "content hash does not match object id".to_string(),
            });
        }
        object::decode(&bytes)
    }

    /// Hash a file without writing an object. Used by `status` and `diff`.
    pub fn hash_file(&self, source: &Path) -> Result<ObjectId> {
        let metadata = fs::metadata(source)?;
        let header = format!("blob {}\n", metadata.len());
        let mut hasher = blake3::Hasher::new();
        hasher.update(header.as_bytes());
        let mut reader = BufReader::new(File::open(source)?);
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(ObjectId::from_bytes(*hasher.finalize().as_bytes()))
    }

    pub fn read_blob(&self, oid: ObjectId) -> Result<Vec<u8>> {
        let (kind, payload) = self.read(oid)?;
        if kind != ObjectKind::Blob {
            return Err(FodeltaError::CorruptObject {
                oid: oid.to_hex(),
                reason: format!("expected blob, found {}", kind.as_str()),
            });
        }
        Ok(payload)
    }

    /// Hash and store a file without loading it fully into memory.
    pub fn write_file(&self, source: &Path) -> Result<ObjectId> {
        let metadata = fs::metadata(source)?;
        if metadata.len() > self.max_object_size {
            return Err(FodeltaError::FileTooLarge {
                path: source.to_path_buf(),
                size: metadata.len(),
                limit: self.max_object_size,
            });
        }

        let header = format!("blob {}\n", metadata.len());
        let mut hasher = blake3::Hasher::new();
        hasher.update(header.as_bytes());

        fs::create_dir_all(&self.root)?;
        let temp = TempPath::new_in(&self.root)?;
        {
            let file = File::create(temp.path())?;
            let writer = BufWriter::new(file);
            let mut encoder = zstd::stream::write::Encoder::new(writer, self.compression)?;
            encoder.include_checksum(true)?;
            encoder.write_all(header.as_bytes())?;

            let mut reader = BufReader::new(File::open(source)?);
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = reader.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                encoder.write_all(&buffer[..read])?;
            }

            let writer = encoder.finish()?;
            let file = writer.into_inner().map_err(|error| error.into_error())?;
            if self.fsync {
                file.sync_all()?;
            }
        }

        let oid = ObjectId::from_bytes(*hasher.finalize().as_bytes());
        let destination = self.path_for(oid);
        if destination.is_file() {
            return Ok(oid);
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        temp.persist(&destination)?;
        if self.fsync {
            sync_dir(&self.root)?;
        }
        Ok(oid)
    }

    pub fn iter_oids(&self) -> Result<Vec<ObjectId>> {
        let mut oids = Vec::new();
        let root = match fs::read_dir(&self.root) {
            Ok(root) => root,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(oids),
            Err(error) => return Err(error.into()),
        };
        for prefix in root {
            let prefix = prefix?;
            if !prefix.file_type()?.is_dir() {
                continue;
            }
            let Some(prefix_name) = prefix.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if prefix_name.len() != 2 {
                continue;
            }
            for entry in fs::read_dir(prefix.path())? {
                let entry = entry?;
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if name.len() != OID_LEN * 2 - 2 {
                    continue;
                }
                let hex = format!("{prefix_name}{name}");
                if let Ok(oid) = ObjectId::from_hex(&hex) {
                    oids.push(oid);
                }
            }
        }
        oids.sort();
        Ok(oids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &Path) -> ObjectStore {
        ObjectStore::new(dir.join("objects"), 3, 1024 * 1024, false)
    }

    #[test]
    fn blob_roundtrip_and_dedup() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let oid = store.write_blob(b"hello").unwrap();
        assert_eq!(store.write_blob(b"hello").unwrap(), oid);
        assert_eq!(store.read_blob(oid).unwrap(), b"hello");
        assert_eq!(store.iter_oids().unwrap(), vec![oid]);

        let mut files = 0;
        for prefix in fs::read_dir(store.root()).unwrap() {
            let prefix = prefix.unwrap();
            files += fs::read_dir(prefix.path()).unwrap().count();
        }
        assert_eq!(files, 1);
    }

    #[test]
    fn corrupt_object_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let oid = store.write_blob(b"hello world").unwrap();
        let path = store.path_for(oid);
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        fs::write(&path, bytes).unwrap();
        assert!(store.read_blob(oid).is_err());
    }

    #[test]
    fn write_file_streams_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let source = dir.path().join("input.bin");
        let data: Vec<u8> = (0..300_000).map(|i| (i % 251) as u8).collect();
        fs::write(&source, &data).unwrap();
        let oid = store.write_file(&source).unwrap();
        assert_eq!(store.read_blob(oid).unwrap(), data);
    }

    #[test]
    fn rejects_oversized_blob() {
        let dir = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(dir.path().join("objects"), 3, 4, false);
        let err = store.write_blob(b"hello").unwrap_err();
        assert!(matches!(err, FodeltaError::DataTooLarge { .. }));
    }
}
