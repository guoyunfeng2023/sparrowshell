use std::collections::HashMap;
use std::sync::Mutex;
use zeroize::{Zeroize, Zeroizing};

#[cfg(debug_assertions)]
pub const KEYCHAIN_SERVICE: &str = "com.sparrowshell.desktop.development";
#[cfg(not(debug_assertions))]
pub const KEYCHAIN_SERVICE: &str = "com.sparrowshell.desktop";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialRef(String);

impl CredentialRef {
    pub fn new_ssh() -> Self {
        Self(format!("ssh/{}", uuid::Uuid::new_v4()))
    }
    pub fn new_account() -> Self {
        Self(format!("account/{}", uuid::Uuid::new_v4()))
    }
    pub fn parse(value: String) -> Result<Self, SecretError> {
        let valid = value.split_once('/').is_some_and(|(kind, id)| {
            matches!(kind, "ssh" | "account" | "license" | "license-device")
                && uuid::Uuid::parse_str(id).is_ok()
        });
        if !valid {
            return Err(SecretError::InvalidReference);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub trait CredentialStore: Send + Sync {
    fn save(&self, reference: &CredentialRef, secret: Zeroizing<String>)
        -> Result<(), SecretError>;
    fn load(&self, reference: &CredentialRef) -> Result<Zeroizing<String>, SecretError>;
    fn delete(&self, reference: &CredentialRef) -> Result<(), SecretError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("credential not found")]
    NotFound,
    #[error("invalid local credential reference")]
    InvalidReference,
    #[error("operating system credential store is unavailable")]
    Unavailable,
    #[error("credential operation failed")]
    OperationFailed,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy)]
pub struct KeychainStore;

#[cfg(target_os = "macos")]
impl KeychainStore {
    /// Platform entry point: macOS secrets live in the Keychain, no on-disk directory needed.
    pub fn for_app(_directory: &std::path::Path) -> Self {
        Self
    }
}

#[cfg(target_os = "macos")]
impl CredentialStore for KeychainStore {
    fn save(
        &self,
        reference: &CredentialRef,
        secret: Zeroizing<String>,
    ) -> Result<(), SecretError> {
        security_framework::passwords::set_generic_password(
            KEYCHAIN_SERVICE,
            reference.as_str(),
            secret.as_bytes(),
        )
        .map_err(|_| SecretError::OperationFailed)
    }
    fn load(&self, reference: &CredentialRef) -> Result<Zeroizing<String>, SecretError> {
        let mut bytes = security_framework::passwords::get_generic_password(
            KEYCHAIN_SERVICE,
            reference.as_str(),
        )
        .map_err(|error| {
            if error.code() == -25300 {
                SecretError::NotFound
            } else {
                SecretError::OperationFailed
            }
        })?;
        let value = String::from_utf8(bytes.clone()).map_err(|_| SecretError::OperationFailed)?;
        bytes.zeroize();
        Ok(Zeroizing::new(value))
    }
    fn delete(&self, reference: &CredentialRef) -> Result<(), SecretError> {
        security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, reference.as_str())
            .map_err(|error| {
                if error.code() == -25300 {
                    SecretError::NotFound
                } else {
                    SecretError::OperationFailed
                }
            })
    }
}

/// Windows DPAPI (CurrentUser) backed credential store.
///
/// Each credential is protected with `CryptProtectData` under the current
/// Windows user and persisted as one file per reference inside
/// `%LOCALAPPDATA%\SparrowShell\secure`. Blobs can only be decrypted by the
/// same Windows user on the same machine; the directory is app-private.
#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
pub struct DpapiStore {
    dir: std::path::PathBuf,
}

#[cfg(target_os = "windows")]
impl DpapiStore {
    pub fn new(dir: impl Into<std::path::PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Platform entry point: store DPAPI blobs under the app's `secure` directory.
    pub fn for_app(directory: &std::path::Path) -> Self {
        Self::new(directory.join("secure"))
    }

    fn path(&self, reference: &CredentialRef) -> std::path::PathBuf {
        let name = reference.as_str().replace(['/', '\\'], "-");
        self.dir.join(format!("{name}.secret"))
    }
}

#[cfg(target_os = "windows")]
mod dpapi {
    use super::SecretError;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    use windows_sys::Win32::Foundation::LocalFree;

    pub fn protect(data: &[u8]) -> Result<Vec<u8>, SecretError> {
        let mut input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptProtectData(
                &mut input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(SecretError::Unavailable);
        }
        let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }
            .to_vec();
        unsafe { LocalFree(output.pbData as _) };
        Ok(bytes)
    }

    pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>, SecretError> {
        let mut input = CRYPT_INTEGER_BLOB {
            cbData: blob.len() as u32,
            pbData: blob.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptUnprotectData(
                &mut input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(SecretError::OperationFailed);
        }
        let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }
            .to_vec();
        unsafe { LocalFree(output.pbData as _) };
        Ok(bytes)
    }
}

#[cfg(target_os = "windows")]
impl CredentialStore for DpapiStore {
    fn save(
        &self,
        reference: &CredentialRef,
        secret: Zeroizing<String>,
    ) -> Result<(), SecretError> {
        std::fs::create_dir_all(&self.dir).map_err(|_| SecretError::Unavailable)?;
        let blob = dpapi::protect(secret.as_bytes())?;
        std::fs::write(self.path(reference), blob).map_err(|_| SecretError::OperationFailed)
    }
    fn load(&self, reference: &CredentialRef) -> Result<Zeroizing<String>, SecretError> {
        let blob = match std::fs::read(self.path(reference)) {
            Ok(blob) => blob,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(SecretError::NotFound)
            }
            Err(_) => return Err(SecretError::OperationFailed),
        };
        let mut bytes = dpapi::unprotect(&blob)?;
        let value = String::from_utf8(bytes.clone()).map_err(|_| SecretError::OperationFailed)?;
        bytes.zeroize();
        Ok(Zeroizing::new(value))
    }
    fn delete(&self, reference: &CredentialRef) -> Result<(), SecretError> {
        match std::fs::remove_file(self.path(reference)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(SecretError::NotFound),
            Err(_) => Err(SecretError::OperationFailed),
        }
    }
}

/// The platform's default credential store: Keychain on macOS, DPAPI files on Windows.
#[cfg(target_os = "macos")]
pub type PlatformCredentialStore = KeychainStore;
#[cfg(target_os = "windows")]
pub type PlatformCredentialStore = DpapiStore;

#[derive(Debug, Default)]
pub struct MemoryCredentialStore {
    values: Mutex<HashMap<String, Zeroizing<String>>>,
}

impl CredentialStore for MemoryCredentialStore {
    fn save(
        &self,
        reference: &CredentialRef,
        secret: Zeroizing<String>,
    ) -> Result<(), SecretError> {
        self.values
            .lock()
            .map_err(|_| SecretError::Unavailable)?
            .insert(reference.as_str().to_owned(), secret);
        Ok(())
    }
    fn load(&self, reference: &CredentialRef) -> Result<Zeroizing<String>, SecretError> {
        self.values
            .lock()
            .map_err(|_| SecretError::Unavailable)?
            .get(reference.as_str())
            .cloned()
            .ok_or(SecretError::NotFound)
    }
    fn delete(&self, reference: &CredentialRef) -> Result<(), SecretError> {
        self.values
            .lock()
            .map_err(|_| SecretError::Unavailable)?
            .remove(reference.as_str())
            .map(|_| ())
            .ok_or(SecretError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_roundtrip_and_delete_do_not_leak_secret_in_errors() {
        let store = MemoryCredentialStore::default();
        let reference = CredentialRef::new_ssh();
        let canary = "unique-test-secret-never-log";
        store
            .save(&reference, Zeroizing::new(canary.to_owned()))
            .unwrap();
        assert_eq!(store.load(&reference).unwrap().as_str(), canary);
        store.delete(&reference).unwrap();
        let error = store.load(&reference).unwrap_err().to_string();
        assert!(!error.contains(canary));
        assert!(!error.contains(reference.as_str()));
    }

    #[test]
    fn reference_is_not_an_arbitrary_local_file_path() {
        assert!(CredentialRef::parse(CredentialRef::new_ssh().as_str().to_owned()).is_ok());
        assert!(CredentialRef::parse("ssh/../../secret".into()).is_err());
        assert!(CredentialRef::parse("/etc/passwd".into()).is_err());
    }

    #[test]
    fn keychain_namespace_matches_build_profile() {
        #[cfg(debug_assertions)]
        assert_eq!(KEYCHAIN_SERVICE, "com.sparrowshell.desktop.development");
        #[cfg(not(debug_assertions))]
        assert_eq!(KEYCHAIN_SERVICE, "com.sparrowshell.desktop");
    }
}
