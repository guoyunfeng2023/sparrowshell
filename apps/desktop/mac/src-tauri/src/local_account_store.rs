use sparrowshell_secrets::{CredentialRef, CredentialStore, KeychainStore, SecretError};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use zeroize::{Zeroize, Zeroizing};

/// Stores account tokens inside the app's private data directory instead of Keychain.
/// Reads never fall back to Keychain; legacy entries are only cleaned up on save or sign-out.
#[derive(Debug, Clone)]
pub struct LocalAccountStore<S = KeychainStore> {
    path: PathBuf,
    legacy: S,
}

impl LocalAccountStore<KeychainStore> {
    pub fn new(directory: &Path) -> Self {
        Self {
            path: directory.join("account-tokens.json"),
            legacy: KeychainStore,
        }
    }
}

impl<S: CredentialStore> LocalAccountStore<S> {
    fn validate(reference: &CredentialRef) -> Result<(), SecretError> {
        if reference.as_str().starts_with("account/") {
            Ok(())
        } else {
            Err(SecretError::InvalidReference)
        }
    }

    fn read_local(&self) -> Result<Option<Zeroizing<String>>, SecretError> {
        match fs::read(&self.path) {
            Ok(mut bytes) => {
                let value =
                    String::from_utf8(bytes.clone()).map_err(|_| SecretError::OperationFailed)?;
                bytes.zeroize();
                Ok(Some(Zeroizing::new(value)))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(SecretError::OperationFailed),
        }
    }

    fn write_local(&self, secret: &Zeroizing<String>) -> Result<(), SecretError> {
        let parent = self.path.parent().ok_or(SecretError::Unavailable)?;
        fs::create_dir_all(parent).map_err(|_| SecretError::OperationFailed)?;
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
        let write_result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(&temporary)
                .map_err(|_| SecretError::OperationFailed)?;
            file.write_all(secret.as_bytes())
                .map_err(|_| SecretError::OperationFailed)?;
            file.sync_all().map_err(|_| SecretError::OperationFailed)?;
            fs::rename(&temporary, &self.path).map_err(|_| SecretError::OperationFailed)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))
                    .map_err(|_| SecretError::OperationFailed)?;
            }
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
    }
}

impl<S: CredentialStore> CredentialStore for LocalAccountStore<S> {
    fn save(
        &self,
        reference: &CredentialRef,
        secret: Zeroizing<String>,
    ) -> Result<(), SecretError> {
        Self::validate(reference)?;
        self.write_local(&secret)?;
        let _ = self.legacy.delete(reference);
        Ok(())
    }

    fn load(&self, reference: &CredentialRef) -> Result<Zeroizing<String>, SecretError> {
        Self::validate(reference)?;
        self.read_local()?.ok_or(SecretError::NotFound)
    }

    fn delete(&self, reference: &CredentialRef) -> Result<(), SecretError> {
        Self::validate(reference)?;
        let local = match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(SecretError::NotFound)
            }
            Err(_) => return Err(SecretError::OperationFailed),
        };
        let legacy = self.legacy.delete(reference);
        match (local, legacy) {
            (Err(SecretError::NotFound), Err(SecretError::NotFound)) => Err(SecretError::NotFound),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sparrowshell_secrets::MemoryCredentialStore;

    fn reference() -> CredentialRef {
        CredentialRef::parse("account/5ed3dd15-b59c-4d69-9ef4-10ad22d83a54".into()).unwrap()
    }

    fn store() -> (PathBuf, LocalAccountStore<MemoryCredentialStore>) {
        let directory = std::env::temp_dir().join(format!(
            "sparrowshell-local-account-test-{}",
            uuid::Uuid::new_v4()
        ));
        let store = LocalAccountStore {
            path: directory.join("account-tokens.json"),
            legacy: MemoryCredentialStore::default(),
        };
        (directory, store)
    }

    #[test]
    fn account_tokens_roundtrip_locally_without_touching_legacy_store() {
        let (directory, store) = store();
        let reference = reference();
        store
            .save(&reference, Zeroizing::new("local-token-payload".into()))
            .unwrap();
        assert_eq!(
            store.load(&reference).unwrap().as_str(),
            "local-token-payload"
        );
        assert!(matches!(
            store.legacy.load(&reference),
            Err(SecretError::NotFound)
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&store.path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        store.delete(&reference).unwrap();
        assert!(matches!(store.load(&reference), Err(SecretError::NotFound)));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn missing_local_tokens_never_fall_back_to_legacy_keychain() {
        let (directory, store) = store();
        let reference = reference();
        store
            .legacy
            .save(&reference, Zeroizing::new("legacy-token-payload".into()))
            .unwrap();
        assert!(matches!(store.load(&reference), Err(SecretError::NotFound)));
        assert!(!store.path.exists());
        assert_eq!(
            store.legacy.load(&reference).unwrap().as_str(),
            "legacy-token-payload"
        );
        let _ = fs::remove_dir_all(directory);
    }
}
