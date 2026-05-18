use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::error::Error;
use crate::model::key::{Fingerprint, KeyName, KeyPair, PublicKey};
use crate::ports::key_store::KeyStore;
use crate::Result;

pub struct InMemoryKeyStore {
    keys: Mutex<HashMap<KeyName, KeyPair>>,
}

impl InMemoryKeyStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            keys: Mutex::new(HashMap::new()),
        }
    }
}
impl Default for InMemoryKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyStore for InMemoryKeyStore {
    fn ensure_dir(&self) -> Result<PathBuf> {
        Ok(PathBuf::from("/in-memory"))
    }

    fn generate(&self, name: &KeyName) -> Result<KeyPair> {
        let pair = KeyPair {
            name: name.clone(),
            public: PublicKey(format!("ssh-ed25519 FAKE {name}")),
            private: format!("-----BEGIN FAKE KEY-----\n{name}\n-----END FAKE KEY-----\n"),
        };
        self.keys
            .lock()
            .expect("mutex")
            .insert(name.clone(), pair.clone());
        Ok(pair)
    }

    fn read_public(&self, name: &KeyName) -> Result<PublicKey> {
        self.keys
            .lock()
            .expect("mutex")
            .get(name)
            .map(|kp| kp.public.clone())
            .ok_or_else(|| Error::KeyOrphaned { name: name.clone() })
    }

    fn private_path(&self, name: &KeyName) -> Result<PathBuf> {
        if !self.exists(name) {
            return Err(Error::KeyOrphaned { name: name.clone() });
        }
        Ok(PathBuf::from(format!("/in-memory/{name}.pem")))
    }

    fn exists(&self, name: &KeyName) -> bool {
        self.keys.lock().expect("mutex").contains_key(name)
    }

    fn delete(&self, name: &KeyName) -> Result<()> {
        self.keys.lock().expect("mutex").remove(name);
        Ok(())
    }

    fn fingerprint(&self, name: &KeyName) -> Result<Fingerprint> {
        let pub_text = self.read_public(name)?;
        let h = crc32fast::hash(pub_text.0.as_bytes());
        Ok(Fingerprint(format!("fake-md5:{h:08x}")))
    }
}
