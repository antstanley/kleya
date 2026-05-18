use crate::model::key::{Fingerprint, KeyName, KeyPair, PublicKey};
use crate::Result;
use std::path::PathBuf;

pub trait KeyStore: Send + Sync {
    fn ensure_dir(&self) -> Result<PathBuf>;
    fn generate(&self, name: &KeyName) -> Result<KeyPair>;
    fn read_public(&self, name: &KeyName) -> Result<PublicKey>;
    fn private_path(&self, name: &KeyName) -> Result<PathBuf>;
    fn exists(&self, name: &KeyName) -> bool;
    fn delete(&self, name: &KeyName) -> Result<()>;

    /// EC2-style MD5 fingerprint of the DER-encoded SPKI of the public key,
    /// formatted as colon-separated lowercase hex (`aa:bb:cc:...`). Must equal
    /// what AWS returns from `DescribeKeyPairs` for an imported key.
    fn fingerprint(&self, name: &KeyName) -> Result<Fingerprint>;
}
