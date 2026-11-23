use sparrowshell_domain::SessionSnapshot;
use sparrowshell_domain::{AuthType, ServerInput, SessionId};
use sparrowshell_secrets::{CredentialRef, CredentialStore, MemoryCredentialStore};
use sparrowshell_ssh_core::{EventSink, SessionManager, TerminalEvent};
use sparrowshell_storage::Repository;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

struct TestEvents {
    sender: Mutex<mpsc::Sender<TerminalEvent>>,
}
impl EventSink for TestEvents {
    fn terminal(&self, event: TerminalEvent) {
        let _ = self.sender.lock().unwrap().try_send(event);
    }
    fn session(&self, _event: SessionSnapshot) {}
}

/// Run explicitly with the disposable tests/openssh container on port 2222.
#[tokio::test]
#[ignore = "requires the disposable OpenSSH integration container"]
async fn password_tofu_pty_roundtrip_and_stale_generation() {
    let repository = Arc::new(Mutex::new(Repository::in_memory().unwrap()));
    let server = repository
        .lock()
        .unwrap()
        .upsert_server(ServerInput {
            id: None,
            folder_id: None,
            name: "integration".into(),
            host: "127.0.0.1".into(),
            port: 2222,
            username: "sparrow".into(),
            auth_type: AuthType::Password,
            private_key_path_hint: None,
            jump_server_id: None,
        })
        .unwrap();
    let credentials = Arc::new(MemoryCredentialStore::default());
    let reference = CredentialRef::new_ssh();
    credentials
        .save(
            &reference,
            Zeroizing::new("sparrow-integration-only".into()),
        )
        .unwrap();
    repository
        .lock()
        .unwrap()
        .set_credential_ref(server.id, Some(reference.as_str().into()))
        .unwrap();
    let (sender, mut receiver) = mpsc::channel(128);
    let manager = SessionManager::new(
        repository,
        credentials,
        Arc::new(TestEvents {
            sender: Mutex::new(sender),
        }),
    );
    let session_id = SessionId::new();

    let first = manager.connect(server.id, session_id).await.unwrap();
    let challenge = first
        .host_key_challenge
        .expect("first connection must require explicit TOFU");
    assert!(challenge.fingerprint.starts_with("SHA256:"));
    assert!(first.session.is_none());
    let connected = manager
        .trust_host_key(server.id, session_id, false)
        .await
        .unwrap();
    let session = connected
        .session
        .expect("trusted connection must open a real PTY");
    assert_eq!(session.target.generation, 1);

    manager
        .terminal_write(session_id, 0, b"echo must-not-run\n".to_vec())
        .await
        .unwrap_err();
    manager
        .terminal_write(session_id, 1, b"printf 'SPARROW_PTY_OK\\n'\n".to_vec())
        .await
        .unwrap();
    let mut bytes = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !String::from_utf8_lossy(&bytes).contains("SPARROW_PTY_OK") {
            let event = receiver.recv().await.expect("terminal event stream closed");
            assert_eq!(event.target, session.target);
            assert_eq!(event.channel_id, session.channel_id.unwrap());
            bytes.extend(event.data);
        }
    })
    .await
    .expect("real PTY output timed out");
    manager
        .terminal_resize(session_id, 1, 100, 40)
        .await
        .unwrap();
    manager
        .terminal_write(session_id, 1, b"exit\n".to_vec())
        .await
        .unwrap();
    manager.disconnect(session_id).await.unwrap();
}
