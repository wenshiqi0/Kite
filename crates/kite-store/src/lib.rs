//! Atomic public application storage. Private keys never belong in this file.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub herdr_path: Option<String>,
    pub stream_host: Option<String>,
    pub sunshine_port: u16,
}

impl Host {
    pub fn address(&self) -> String {
        address(&self.host, self.port)
    }
}

pub fn address(host: &str, port: u16) -> String {
    format!("{}:{port}", host.trim().to_lowercase())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedServer {
    pub certificate_pem: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Data {
    pub version: u32,
    pub hosts: Vec<Host>,
    pub selected_host: Option<String>,
    pub trusted_keys: BTreeMap<String, String>,
    pub paired_servers: BTreeMap<String, PairedServer>,
    pub completed_imports: BTreeSet<String>,
    pub device_id: String,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            version: 1,
            hosts: Vec::new(),
            selected_host: None,
            trusted_keys: BTreeMap::new(),
            paired_servers: BTreeMap::new(),
            completed_imports: BTreeSet::new(),
            device_id: String::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid public store: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported store version {0}")]
    UnsupportedVersion(u32),
    #[error("conflicting trusted keys for {0}")]
    ConflictingTrust(String),
    #[error("invalid legacy address {0}")]
    InvalidAddress(String),
}

pub struct Store {
    path: PathBuf,
    data: Data,
}

impl Store {
    pub fn open(directory: &Path) -> Result<Self, StoreError> {
        fs::create_dir_all(directory)?;
        let path = directory.join("hosts.json");
        let data: Data = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => return Err(e.into()),
        };
        if data.version != 1 {
            return Err(StoreError::UnsupportedVersion(data.version));
        }
        Ok(Self { path, data })
    }

    pub fn data(&self) -> &Data {
        &self.data
    }

    /// Commit a complete replacement before exposing it to readers.
    pub fn commit(&mut self, data: Data) -> Result<(), StoreError> {
        let directory = self.path.parent().expect("store always has a parent");
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer_pretty(&mut file, &data)?;
        file.flush()?;
        file.as_file().sync_all()?;
        file.persist(&self.path).map_err(|e| e.error)?;
        File::open(directory)?.sync_all()?;
        self.data = data;
        Ok(())
    }

    /// Native M1 data is imported once, atomically, without regenerating identity.
    pub fn import_legacy(
        &mut self,
        source: &str,
        host: Option<Host>,
        keys: &[(String, String)],
    ) -> Result<(), StoreError> {
        if self.data.completed_imports.contains(source) {
            return Ok(());
        }
        let mut next = self.data.clone();
        for (old_address, key) in keys {
            let (host, port) = old_address
                .rsplit_once(':')
                .ok_or_else(|| StoreError::InvalidAddress(old_address.clone()))?;
            let port = port
                .parse::<u16>()
                .ok()
                .filter(|p| *p != 0)
                .ok_or_else(|| StoreError::InvalidAddress(old_address.clone()))?;
            let normalized = address(host, port);
            if let Some(existing) = next.trusted_keys.get(&normalized)
                && !same_key(existing, key)
            {
                return Err(StoreError::ConflictingTrust(normalized));
            }
            next.trusted_keys.insert(normalized, key.clone());
        }
        if let Some(host) = host
            && !next
                .hosts
                .iter()
                .any(|h| h.address() == host.address() && h.username == host.username)
        {
            next.selected_host.get_or_insert_with(|| host.id.clone());
            next.hosts.push(host);
        }
        next.completed_imports.insert(source.to_owned());
        self.commit(next)
    }
}

fn same_key(a: &str, b: &str) -> bool {
    a.split_whitespace()
        .take(2)
        .eq(b.split_whitespace().take(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> Host {
        Host {
            id: "legacy-1".into(),
            host: "Example.LOCAL".into(),
            port: 22,
            username: "kite".into(),
            herdr_path: None,
            stream_host: None,
            sunshine_port: 47989,
        }
    }

    #[test]
    fn migrates_every_pin_and_host_once_and_survives_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let keys = vec![
            ("EXAMPLE.local:022".into(), "ssh-ed25519 AAAA first".into()),
            ("other.local:2222".into(), "ssh-ed25519 BBBB second".into()),
        ];
        store
            .import_legacy("android-m1", Some(host()), &keys)
            .unwrap();
        store
            .import_legacy("android-m1", Some(host()), &keys)
            .unwrap();
        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(reopened.data().hosts.len(), 1);
        assert_eq!(reopened.data().trusted_keys.len(), 2);
        assert_eq!(reopened.data().trusted_keys["example.local:22"], keys[0].1);
        assert!(reopened.data().completed_imports.contains("android-m1"));
    }

    #[test]
    fn conflicting_normalized_pins_leave_the_previous_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        store.commit(Data::default()).unwrap();
        let before = fs::read(dir.path().join("hosts.json")).unwrap();
        let keys = vec![
            ("host:22".into(), "ssh-ed25519 AAAA".into()),
            ("HOST:022".into(), "ssh-ed25519 BBBB".into()),
        ];
        assert!(matches!(
            store.import_legacy("android-m1", Some(host()), &keys),
            Err(StoreError::ConflictingTrust(_))
        ));
        assert_eq!(fs::read(dir.path().join("hosts.json")).unwrap(), before);
        assert!(store.data().completed_imports.is_empty());
    }

    #[test]
    fn corrupt_data_is_never_replaced_by_an_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hosts.json"), b"{corrupt").unwrap();
        assert!(Store::open(dir.path()).is_err());
        assert_eq!(
            fs::read(dir.path().join("hosts.json")).unwrap(),
            b"{corrupt"
        );
    }
}
