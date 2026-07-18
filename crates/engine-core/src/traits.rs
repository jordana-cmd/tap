//! Platform I/O boundaries (final spec §0 Rule 1): engine-core never touches
//! browser APIs; `engine-web` implements these traits against WebGPU/OPFS/
//! fetch/`performance.now()`. Deliberately minimal declarations — they
//! establish the boundary and will grow as detection and rendering land.

use std::collections::HashMap;

/// Time source (`performance.now()` in the browser).
pub trait Clock {
    fn now_ms(&self) -> f64;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    #[error("storage backend failure: {0}")]
    Backend(String),
}

/// Key-value persistence (OPFS-backed SQLite WAL in the browser).
pub trait Storage {
    fn put(&mut self, key: &str, bytes: &[u8]) -> Result<(), StorageError>;
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetworkError {
    #[error("network transport failure: {0}")]
    Transport(String),
}

/// Request transport (fetch/WS in the browser).
pub trait Network {
    fn post(&self, path: &str, body: &[u8]) -> Result<Vec<u8>, NetworkError>;
}

/// Render surface descriptor (WebGPU/WebGL canvas in the browser).
pub trait Surface {
    /// Surface size in device pixels.
    fn size(&self) -> (u32, u32);
}

// ---- Stub implementations (tests and native harnesses) ----

/// Clock stub returning a fixed timestamp.
#[derive(Debug, Clone, Copy, Default)]
pub struct StubClock(pub f64);

impl Clock for StubClock {
    fn now_ms(&self) -> f64 {
        self.0
    }
}

/// In-memory storage stub.
#[derive(Debug, Default)]
pub struct MemStorage(HashMap<String, Vec<u8>>);

impl Storage for MemStorage {
    fn put(&mut self, key: &str, bytes: &[u8]) -> Result<(), StorageError> {
        self.0.insert(key.to_owned(), bytes.to_vec());
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.0.get(key).cloned())
    }
}

/// Network stub that refuses every request — engine-core makes no network
/// calls; anything reaching this in a test is a bug.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopNetwork;

impl Network for NoopNetwork {
    fn post(&self, _path: &str, _body: &[u8]) -> Result<Vec<u8>, NetworkError> {
        Err(NetworkError::Transport(
            "engine-core stubs have no network".to_owned(),
        ))
    }
}

/// Surface stub with a fixed size.
#[derive(Debug, Clone, Copy)]
pub struct FixedSurface(pub u32, pub u32);

impl Surface for FixedSurface {
    fn size(&self) -> (u32, u32) {
        (self.0, self.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_storage_roundtrip_and_missing_key() {
        let mut storage = MemStorage::default();
        storage.put("doc/1", b"hello").unwrap();
        assert_eq!(storage.get("doc/1").unwrap(), Some(b"hello".to_vec()));
        assert_eq!(storage.get("doc/2").unwrap(), None);
    }

    #[test]
    fn stub_clock_and_surface_are_fixed() {
        assert_eq!(StubClock(1234.5).now_ms(), 1234.5);
        assert_eq!(FixedSurface(800, 600).size(), (800, 600));
    }

    #[test]
    fn noop_network_always_errors() {
        assert!(NoopNetwork.post("/api/v1/anything", b"").is_err());
    }
}
