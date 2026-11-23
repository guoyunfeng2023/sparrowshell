#![cfg(target_os = "macos")]

use sparrowshell_secrets::{CredentialRef, CredentialStore, KeychainStore};
use zeroize::Zeroizing;

#[test]
#[ignore = "writes and removes one disposable item in the current user's Keychain"]
fn real_keychain_roundtrip_and_cleanup() {
    let store = KeychainStore;
    let reference = CredentialRef::new_ssh();
    store
        .save(
            &reference,
            Zeroizing::new("disposable-keychain-integration-only".into()),
        )
        .unwrap();
    assert_eq!(
        store.load(&reference).unwrap().as_str(),
        "disposable-keychain-integration-only"
    );
    store.delete(&reference).unwrap();
    assert!(store.load(&reference).is_err());
}
