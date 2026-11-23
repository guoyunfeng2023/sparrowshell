use sparrowshell_domain::{AuthType, ServerInput, SessionId, SessionSnapshot};
use sparrowshell_secrets::{CredentialRef, CredentialStore, MemoryCredentialStore};
use sparrowshell_sftp_core::RemoteFileKind;
use sparrowshell_ssh_core::{EventSink, SessionManager, TerminalEvent};
use sparrowshell_storage::Repository;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

struct TestEvents;
impl EventSink for TestEvents {
    fn terminal(&self, _event: TerminalEvent) {}
    fn session(&self, _event: SessionSnapshot) {}
}

/// Explicit opt-in live verification. All destructive operations are confined
/// to one unique directory under /tmp on the configured disposable test host.
#[tokio::test]
#[ignore = "requires explicit SPARROWSHELL_TEST_SSH_* environment variables"]
async fn quick_remove_recurses_and_treats_special_names_as_literal_paths() {
    let host = std::env::var("SPARROWSHELL_TEST_SSH_HOST").unwrap();
    let port = std::env::var("SPARROWSHELL_TEST_SSH_PORT").unwrap_or_else(|_| "22".into());
    let username = std::env::var("SPARROWSHELL_TEST_SSH_USERNAME").unwrap();
    let password = std::env::var("SPARROWSHELL_TEST_SSH_PASSWORD").unwrap();
    let repository = Arc::new(Mutex::new(Repository::in_memory().unwrap()));
    let server = repository
        .lock()
        .unwrap()
        .upsert_server(ServerInput {
            id: None,
            folder_id: None,
            name: "quick-remove-live".into(),
            host,
            port: port.parse().unwrap(),
            username,
            auth_type: AuthType::Password,
            private_key_path_hint: None,
            jump_server_id: None,
        })
        .unwrap();
    let credentials = Arc::new(MemoryCredentialStore::default());
    let reference = CredentialRef::new_ssh();
    credentials
        .save(&reference, Zeroizing::new(password))
        .unwrap();
    repository
        .lock()
        .unwrap()
        .set_credential_ref(server.id, Some(reference.as_str().into()))
        .unwrap();
    let manager = SessionManager::new(repository, credentials, Arc::new(TestEvents));
    let session_id = SessionId::new();
    manager.connect(server.id, session_id).await.unwrap();
    let connected = manager
        .trust_host_key(server.id, session_id, false)
        .await
        .unwrap();
    let generation = connected.session.unwrap().target.generation;

    let base_name = format!("sparrowshell-quick-it-{}", uuid::Uuid::new_v4().simple());
    let base = format!("/tmp/{base_name}");
    manager
        .create_remote_directory(session_id, generation, "/tmp".into(), base_name)
        .await
        .unwrap();
    let special_name = "nested ' $HOME ;";
    let special = format!("{base}/{special_name}");
    manager
        .create_remote_directory(session_id, generation, base.clone(), special_name.into())
        .await
        .unwrap();
    manager
        .create_remote_directory(session_id, generation, special.clone(), "child".into())
        .await
        .unwrap();
    manager
        .remove_remote_entry(
            session_id,
            generation,
            special.clone(),
            RemoteFileKind::Directory,
        )
        .await
        .unwrap_err();
    manager
        .quick_remove_remote_entry(
            session_id,
            generation,
            special.clone(),
            RemoteFileKind::File,
        )
        .await
        .unwrap_err();
    manager
        .quick_remove_remote_entry(
            session_id,
            generation,
            special.clone(),
            RemoteFileKind::Directory,
        )
        .await
        .unwrap();
    manager
        .create_remote_directory(
            session_id,
            generation,
            base.clone(),
            "remaining-child".into(),
        )
        .await
        .unwrap();
    manager
        .quick_remove_remote_entry(
            session_id,
            generation,
            base.clone(),
            RemoteFileKind::Directory,
        )
        .await
        .unwrap();
    let listing = manager
        .list_remote_directory(session_id, generation, Some("/tmp".into()))
        .await
        .unwrap();
    assert!(listing.entries.iter().all(|entry| entry.path != base));
    manager.close_session(session_id).await.unwrap();
}
