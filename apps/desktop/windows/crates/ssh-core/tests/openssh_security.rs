use sparrowshell_domain::{AuthType, ServerInput, SessionId, SessionSnapshot};
use sparrowshell_secrets::{CredentialRef, CredentialStore, MemoryCredentialStore};
use sparrowshell_ssh_core::{EventSink, SessionManager, SshError, TerminalEvent};
use sparrowshell_storage::Repository;
use std::sync::Arc;
use zeroize::Zeroizing;

struct NoEvents;
impl EventSink for NoEvents {
    fn terminal(&self, _event: TerminalEvent) {}
    fn session(&self, _event: SessionSnapshot) {}
}

fn input(
    name: &str,
    auth_type: AuthType,
    jump_server_id: Option<sparrowshell_domain::ServerId>,
) -> ServerInput {
    ServerInput {
        id: None,
        folder_id: None,
        name: name.into(),
        host: "127.0.0.1".into(),
        port: 2222,
        username: "sparrow".into(),
        auth_type,
        private_key_path_hint: None,
        jump_server_id,
    }
}

fn local_secret(
    store: &MemoryCredentialStore,
    repository: &Repository,
    server_id: sparrowshell_domain::ServerId,
    value: &str,
) {
    let reference = CredentialRef::new_ssh();
    store
        .save(&reference, Zeroizing::new(value.into()))
        .unwrap();
    repository
        .set_credential_ref(server_id, Some(reference.as_str().into()))
        .unwrap();
}

#[tokio::test]
#[ignore = "requires the disposable OpenSSH integration container"]
async fn private_key_through_jump_host_opens_target_pty() {
    let repository = Arc::new(std::sync::Mutex::new(Repository::in_memory().unwrap()));
    let jump = repository
        .lock()
        .unwrap()
        .upsert_server(input("jump", AuthType::Password, None))
        .unwrap();
    let key_path = std::fs::canonicalize(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/openssh/runtime/id_ed25519"
    ))
    .expect("generate the disposable test key first");
    let target = repository
        .lock()
        .unwrap()
        .upsert_server(ServerInput {
            // The target address is resolved from inside the jump container, not from the Mac.
            port: 22,
            private_key_path_hint: Some(key_path.to_string_lossy().into_owned()),
            ..input("target", AuthType::PrivateKey, Some(jump.id))
        })
        .unwrap();
    let credentials = Arc::new(MemoryCredentialStore::default());
    local_secret(
        &credentials,
        &repository.lock().unwrap(),
        jump.id,
        "sparrow-integration-only",
    );
    local_secret(
        &credentials,
        &repository.lock().unwrap(),
        target.id,
        "sparrow-key-passphrase-only",
    );
    let manager = SessionManager::new(repository, credentials, Arc::new(NoEvents));

    let jump_session = SessionId::new();
    assert!(manager
        .connect(jump.id, jump_session)
        .await
        .unwrap()
        .host_key_challenge
        .is_some());
    manager
        .trust_host_key(jump.id, jump_session, false)
        .await
        .unwrap();
    let target_session = SessionId::new();
    assert!(manager
        .connect(target.id, target_session)
        .await
        .unwrap()
        .host_key_challenge
        .is_some());
    let result = manager
        .trust_host_key(target.id, target_session, false)
        .await
        .unwrap();
    let session = result
        .session
        .expect("private key through jump host must authenticate");
    assert_eq!(session.target.server_id, target.id);
    manager.disconnect(jump_session).await.unwrap();
    manager.disconnect(target_session).await.unwrap();
}

#[tokio::test]
#[ignore = "requires the disposable OpenSSH integration container"]
async fn changed_host_key_is_blocked_and_needs_explicit_replacement() {
    let repository = Arc::new(std::sync::Mutex::new(Repository::in_memory().unwrap()));
    let server = repository
        .lock()
        .unwrap()
        .upsert_server(input("fingerprint", AuthType::Password, None))
        .unwrap();
    repository
        .lock()
        .unwrap()
        .trust_host_key(
            "127.0.0.1",
            2222,
            &sparrowshell_storage::HostKeyRecord {
                algorithm: "ssh-ed25519".into(),
                public_key: "deliberately-wrong-test-key".into(),
                fingerprint: "SHA256:wrong-test-key".into(),
            },
        )
        .unwrap();
    let manager = SessionManager::new(
        repository,
        Arc::new(MemoryCredentialStore::default()),
        Arc::new(NoEvents),
    );
    let session_id = SessionId::new();
    let result = manager.connect(server.id, session_id).await.unwrap();
    assert_eq!(
        result
            .host_key_challenge
            .unwrap()
            .previous_fingerprint
            .as_deref(),
        Some("SHA256:wrong-test-key")
    );
    assert!(matches!(
        manager.trust_host_key(server.id, session_id, false).await,
        Err(SshError::HostKeyReplacementRequired)
    ));
}
