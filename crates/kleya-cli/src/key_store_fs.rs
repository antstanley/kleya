use std::fs;
use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::PathBuf;

use md5::{Digest, Md5};

use kleya_core::{
    config::KeysCfg,
    model::key::{Fingerprint, KeyName, KeyPair, PublicKey},
    ports::key_store::KeyStore,
    Error, Result,
};
use ssh_key::{rand_core::OsRng, Algorithm, PrivateKey};

const DIR_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;

pub struct FsKeyStore {
    dir: PathBuf,
    default_key: String,
}

impl FsKeyStore {
    pub fn from_config(cfg: &KeysCfg) -> Result<Self> {
        let dir = PathBuf::from(shellexpand::tilde(&cfg.dir).to_string());
        Ok(Self {
            dir,
            default_key: cfg.default_key_name.clone(),
        })
    }
    #[must_use]
    pub fn default_key_name(&self) -> &str {
        &self.default_key
    }

    fn path_for(&self, name: &KeyName) -> PathBuf {
        self.dir.join(format!("{name}.pem"))
    }

    fn assert_dir_mode(&self) -> Result<()> {
        let md = fs::metadata(&self.dir)?;
        let mode = md.permissions().mode() & 0o777;
        if mode != DIR_MODE {
            return Err(Error::ConfigInvalid {
                reason: format!("{} mode is {mode:o} not {DIR_MODE:o}", self.dir.display()),
            });
        }
        Ok(())
    }

    #[allow(clippy::unused_self)]
    fn assert_file_mode(&self, p: &PathBuf) -> Result<()> {
        let md = fs::metadata(p)?;
        let mode = md.permissions().mode() & 0o777;
        if mode != FILE_MODE {
            return Err(Error::ConfigInvalid {
                reason: format!("{} mode is {mode:o} not {FILE_MODE:o}", p.display()),
            });
        }
        Ok(())
    }
}

impl KeyStore for FsKeyStore {
    fn ensure_dir(&self) -> Result<PathBuf> {
        fs::create_dir_all(&self.dir)?;
        fs::set_permissions(&self.dir, fs::Permissions::from_mode(DIR_MODE))?;
        self.assert_dir_mode()?;
        Ok(self.dir.clone())
    }

    fn generate(&self, name: &KeyName) -> Result<KeyPair> {
        self.ensure_dir()?;
        let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).map_err(|e| {
            Error::ConfigInvalid {
                reason: format!("ed25519: {e}"),
            }
        })?;
        let private =
            key.to_openssh(ssh_key::LineEnding::LF)
                .map_err(|e| Error::ConfigInvalid {
                    reason: format!("openssh: {e}"),
                })?;
        let public = key
            .public_key()
            .to_openssh()
            .map_err(|e| Error::ConfigInvalid {
                reason: format!("openssh: {e}"),
            })?;
        let path = self.path_for(name);
        let mut f = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(FILE_MODE)
            .open(&path)?;
        f.write_all(private.as_bytes())?;
        self.assert_file_mode(&path)?;
        Ok(KeyPair {
            name: name.clone(),
            public: PublicKey(public),
            private: private.to_string(),
        })
    }

    fn read_public(&self, name: &KeyName) -> Result<PublicKey> {
        let path = self.path_for(name);
        self.assert_file_mode(&path)?;
        let text = fs::read_to_string(&path)?;
        let key = PrivateKey::from_openssh(&text).map_err(|e| Error::ConfigInvalid {
            reason: format!("openssh: {e}"),
        })?;
        let pub_text = key
            .public_key()
            .to_openssh()
            .map_err(|e| Error::ConfigInvalid {
                reason: format!("openssh: {e}"),
            })?;
        Ok(PublicKey(pub_text))
    }

    fn private_path(&self, name: &KeyName) -> Result<PathBuf> {
        let path = self.path_for(name);
        if !path.exists() {
            return Err(Error::KeyOrphaned { name: name.clone() });
        }
        self.assert_file_mode(&path)?;
        Ok(path)
    }

    fn exists(&self, name: &KeyName) -> bool {
        self.path_for(name).exists()
    }
    fn delete(&self, name: &KeyName) -> Result<()> {
        let p = self.path_for(name);
        if p.exists() {
            fs::remove_file(p)?;
        }
        Ok(())
    }

    fn fingerprint(&self, name: &KeyName) -> Result<Fingerprint> {
        // EC2 (for ImportKeyPair-imported keys) returns MD5 of the DER-encoded
        // SubjectPublicKeyInfo as colon-separated lowercase hex. For Ed25519
        // the SPKI is a fixed 44-byte structure: 12-byte header + 32-byte key.
        let path = self.path_for(name);
        self.assert_file_mode(&path)?;
        let text = fs::read_to_string(&path)?;
        let priv_key = PrivateKey::from_openssh(&text).map_err(|e| Error::ConfigInvalid {
            reason: format!("openssh: {e}"),
        })?;
        let ed =
            priv_key
                .public_key()
                .key_data()
                .ed25519()
                .ok_or_else(|| Error::ConfigInvalid {
                    reason: "only Ed25519 keys are supported".into(),
                })?;
        // SPKI for Ed25519: SEQUENCE { SEQUENCE { OID 1.3.101.112 }, BIT STRING <32 bytes> }
        // 30 2A 30 05 06 03 2B 65 70 03 21 00 <pubkey>
        let mut der = Vec::with_capacity(44);
        der.extend_from_slice(&[
            0x30, 0x2A, 0x30, 0x05, 0x06, 0x03, 0x2B, 0x65, 0x70, 0x03, 0x21, 0x00,
        ]);
        der.extend_from_slice(ed.as_ref());
        assert_eq!(der.len(), 44, "Ed25519 SPKI is 44 bytes");
        let digest = Md5::digest(&der);
        let hexstr = hex::encode(digest);
        assert_eq!(hexstr.len(), 32, "md5 hex is 32 chars");
        let mut out = String::with_capacity(47);
        for (i, c) in hexstr.chars().enumerate() {
            if i > 0 && i % 2 == 0 {
                out.push(':');
            }
            out.push(c);
        }
        assert_eq!(out.len(), 47, "colon-formatted fingerprint is 47 chars");
        Ok(Fingerprint(out))
    }
}
