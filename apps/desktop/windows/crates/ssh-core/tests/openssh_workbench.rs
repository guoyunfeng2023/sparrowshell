use sparrowshell_domain::{AuthType, ServerInput, SessionId, SessionSnapshot};
use sparrowshell_execution::{
    CommandExecutor, CommandProposal, CommandRequest, CommandSource, ExecutionBroker,
    ExecutionDecision, ExecutionMode,
};
use sparrowshell_secrets::{CredentialRef, CredentialStore, MemoryCredentialStore};
use sparrowshell_sftp_core::RemoteFileKind;
use sparrowshell_ssh_core::{EventSink, ProcessSignal, SessionManager, SshError, TerminalEvent};
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
async fn real_monitor_process_and_sftp_workbench() {
    let repository = Arc::new(Mutex::new(Repository::in_memory().unwrap()));
    let server = repository
        .lock()
        .unwrap()
        .upsert_server(ServerInput {
            id: None,
            folder_id: None,
            name: "workbench-integration".into(),
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
    let (sender, _receiver) = mpsc::channel(128);
    let manager = SessionManager::new(
        repository,
        credentials,
        Arc::new(TestEvents {
            sender: Mutex::new(sender),
        }),
    );
    let session_id = SessionId::new();
    manager.connect(server.id, session_id).await.unwrap();
    let connected = manager
        .trust_host_key(server.id, session_id, false)
        .await
        .unwrap();
    let session = connected.session.unwrap();
    let generation = session.target.generation;

    manager.monitor_snapshot(session_id, 0).await.unwrap_err();
    let snapshot = manager
        .monitor_snapshot(session_id, generation)
        .await
        .unwrap();
    assert!(!snapshot.hostname.is_empty());
    assert!(snapshot.memory_total_bytes > 0);
    assert!(!snapshot.disks.is_empty());
    assert!(!snapshot.processes.is_empty());
    assert_eq!(snapshot.target, session.target);

    let broker = ExecutionBroker::new();
    broker.activate_target(session.target).unwrap();
    let decision = broker
        .submit(
            CommandRequest {
                source: CommandSource::Ai,
                target: session.target,
                proposal: CommandProposal {
                    command: "printf 'AI_STDOUT_OK'; printf 'AI_STDERR_OK' >&2".into(),
                    model_risk: None,
                },
            },
            ExecutionMode::AutoAll,
        )
        .unwrap();
    let ExecutionDecision::Execute(permit) = decision else {
        panic!("AutoAll must not create hidden approval");
    };
    let output = manager
        .execute(&permit, "printf 'AI_STDOUT_OK'; printf 'AI_STDERR_OK' >&2")
        .await
        .unwrap();
    assert_eq!(output.stdout, "AI_STDOUT_OK");
    assert_eq!(output.stderr, "AI_STDERR_OK");
    assert_eq!(output.exit_status, Some(0));

    let home = manager
        .list_remote_directory(session_id, generation, None)
        .await
        .unwrap();
    let name = format!("sparrowshell-it-{}", uuid::Uuid::new_v4().simple());
    manager
        .create_remote_directory(session_id, generation, home.path.clone(), name.clone())
        .await
        .unwrap();
    let path = format!("{}/{name}", home.path.trim_end_matches('/'));
    let listing = manager
        .list_remote_directory(session_id, generation, Some(path.clone()))
        .await
        .unwrap();
    assert!(listing.entries.is_empty());
    manager
        .create_remote_directory(session_id, generation, path.clone(), "nested".into())
        .await
        .unwrap();
    manager
        .rename_remote_entry(
            session_id,
            generation,
            format!("{path}/nested"),
            "nested-renamed".into(),
            RemoteFileKind::Directory,
        )
        .await
        .unwrap();
    let renamed = manager
        .list_remote_directory(session_id, generation, Some(path.clone()))
        .await
        .unwrap();
    assert_eq!(renamed.entries[0].name, "nested-renamed");
    manager
        .move_remote_entry(
            session_id,
            generation,
            format!("{path}/nested-renamed"),
            home.path.clone(),
            RemoteFileKind::Directory,
        )
        .await
        .unwrap();
    let moved = manager
        .list_remote_directory(session_id, generation, Some(path.clone()))
        .await
        .unwrap();
    assert!(moved.entries.is_empty());
    let moved_path = format!("{}/nested-renamed", home.path.trim_end_matches('/'));
    manager
        .remove_remote_entry(
            session_id,
            generation,
            moved_path.clone(),
            RemoteFileKind::File,
        )
        .await
        .unwrap_err();
    manager
        .remove_remote_entry(
            session_id,
            generation,
            moved_path,
            RemoteFileKind::Directory,
        )
        .await
        .unwrap();
    let quick_name = "quick ' $HOME ;";
    manager
        .create_remote_directory(session_id, generation, path.clone(), quick_name.into())
        .await
        .unwrap();
    let quick_path = format!("{path}/{quick_name}");
    manager
        .create_remote_directory(session_id, generation, quick_path.clone(), "nested".into())
        .await
        .unwrap();
    manager
        .remove_remote_entry(
            session_id,
            generation,
            quick_path.clone(),
            RemoteFileKind::Directory,
        )
        .await
        .unwrap_err();
    manager
        .quick_remove_remote_entry(
            session_id,
            generation,
            quick_path.clone(),
            RemoteFileKind::File,
        )
        .await
        .unwrap_err();
    manager
        .quick_remove_remote_entry(
            session_id,
            generation,
            quick_path.clone(),
            RemoteFileKind::Directory,
        )
        .await
        .unwrap();
    let after_quick_remove = manager
        .list_remote_directory(session_id, generation, Some(path.clone()))
        .await
        .unwrap();
    assert!(after_quick_remove
        .entries
        .iter()
        .all(|entry| entry.path != quick_path));
    manager
        .remove_remote_entry(session_id, generation, path, RemoteFileKind::Directory)
        .await
        .unwrap();

    manager
        .terminal_write(session_id, generation, b"sleep 120 &\n".to_vec())
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let with_sleep = manager
        .monitor_snapshot(session_id, generation)
        .await
        .unwrap();
    let process = with_sleep
        .processes
        .iter()
        .find(|process| process.command.starts_with("sleep 120"))
        .expect("the real background process must be visible");
    let error = manager
        .terminate_process(
            session_id,
            generation,
            process.pid,
            "invalid but safe identity".into(),
            ProcessSignal::Terminate,
        )
        .await
        .unwrap_err();
    assert!(matches!(error, SshError::ProcessIdentityChanged));
    manager
        .terminate_process(
            session_id,
            generation,
            process.pid,
            process.start_id.clone(),
            ProcessSignal::Terminate,
        )
        .await
        .unwrap();
    manager.disconnect(session_id).await.unwrap();
}
