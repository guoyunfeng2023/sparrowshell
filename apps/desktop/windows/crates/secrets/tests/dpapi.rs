#![cfg(target_os = "windows")]

use sparrowshell_secrets::{CredentialRef, CredentialStore, DpapiStore, SecretError};
use zeroize::Zeroizing;

#[test]
fn dpapi_store_roundtrips_and_deletes_secrets() {
    let directory = std::env::temp_dir().join(format!("sparrowshell-dpapi-test-{}", uuid::Uuid::new_v4()));
    let store = DpapiStore::new(&directory);
    let reference = CredentialRef::new_ssh();
    let secret = "dpapi-test-secret-never-log";

    store
        .save(&reference, Zeroizing::new(secret.to_owned()))
        .unwrap();
    assert_eq!(store.load(&reference).unwrap().as_str(), secret);

    // On-disk content must be DPAPI-encrypted, never plaintext.
    let raw = std::fs::read(directory.join(format!(
        "{}.secret",
        reference.as_str().replace('/', "-")
    )))
    .unwrap();
    assert_ne!(raw, secret.as_bytes());
    assert!(String::from_utf8_lossy(&raw).find(secret).is_none());

    store.delete(&reference).unwrap();
    assert!(matches!(
        store.load(&reference),
        Err(SecretError::NotFound)
    ));
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn dpapi_store_reports_missing_references() {
    let directory = std::env::temp_dir().join(format!("sparrowshell-dpapi-test-{}", uuid::Uuid::new_v4()));
    let store = DpapiStore::new(&directory);
    let reference = CredentialRef::new_account();
    assert!(matches!(
        store.load(&reference),
        Err(SecretError::NotFound)
    ));
}
