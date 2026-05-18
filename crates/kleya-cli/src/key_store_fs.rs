use std::path::PathBuf;

use kleya_core::{
    config::KeysCfg,
    model::key::{Fingerprint, KeyName, KeyPair, PublicKey},
    ports::key_store::KeyStore,
    Result,
};

pub struct FsKeyStore {
    pub dir: PathBuf,
}

impl FsKeyStore {
    pub fn from_config(_cfg: &KeysCfg) -> Result<Self> {
        Ok(Self {
            dir: PathBuf::from(shellexpand::tilde("~/.config/kleya/keys").to_string()),
        })
    }
}

impl KeyStore for FsKeyStore {
    fn ensure_dir(&self) -> Result<PathBuf> {
        Ok(self.dir.clone())
    }
    fn generate(&self, _name: &KeyName) -> Result<KeyPair> {
        Err(kleya_core::Error::ConfigInvalid {
            reason: "FsKeyStore::generate not yet implemented (Task 20)".into(),
        })
    }
    fn read_public(&self, _name: &KeyName) -> Result<PublicKey> {
        Err(kleya_core::Error::ConfigInvalid {
            reason: "FsKeyStore::read_public not yet implemented (Task 20)".into(),
        })
    }
    fn private_path(&self, name: &KeyName) -> Result<PathBuf> {
        Ok(self.dir.join(format!("{name}.pem")))
    }
    fn exists(&self, name: &KeyName) -> bool {
        self.dir.join(format!("{name}.pem")).exists()
    }
    fn delete(&self, _name: &KeyName) -> Result<()> {
        Ok(())
    }
    fn fingerprint(&self, _name: &KeyName) -> Result<Fingerprint> {
        Err(kleya_core::Error::ConfigInvalid {
            reason: "FsKeyStore::fingerprint not yet implemented (Task 20)".into(),
        })
    }
}
