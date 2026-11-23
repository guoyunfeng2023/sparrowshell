use sparrowshell_domain::{AuthType, ServerInput, SessionId, SessionSnapshot, TransferId};
use sparrowshell_secrets::{CredentialRef, CredentialStore, MemoryCredentialStore};
use sparrowshell_sftp_core::{TransferEvent, TransferState};
use sparrowshell_ssh_core::{EventSink, SessionManager, TerminalEvent};
use sparrowshell_storage::Repository;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

struct TestEvents {
    terminal: mpsc::UnboundedSender<TerminalEvent>,
    transfers: mpsc::UnboundedSender<TransferEvent>,
}

impl EventSink for TestEvents {
    fn terminal(&self, event: TerminalEvent) {
        let _ = self.terminal.send(event);
    }

    fn session(&self, _event: SessionSnapshot) {}

    fn transfer(&self, event: TransferEvent) {
        let _ = self.transfers.send(event);
    }
}

struct Workbench {
    manager: SessionManager,
    session_id: SessionId,
    generation: u64,
    terminal: mpsc::UnboundedReceiver<TerminalEvent>,
    transfers: mpsc::UnboundedReceiver<TransferEvent>,
}

impl Workbench {
    async fn connect() -> Self {
        let repository = Arc::new(std::sync::Mutex::new(Repository::in_memory().unwrap()));
        let server = repository
            .lock()
            .unwrap()
            .upsert_server(ServerInput {
                id: None,
                folder_id: None,
                name: "fault-injection".into(),
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
        let (terminal_sender, terminal) = mpsc::unbounded_channel();
        let (transfer_sender, transfers) = mpsc::unbounded_channel();
        let manager = SessionManager::new(
            repository,
            credentials,
            Arc::new(TestEvents {
                terminal: terminal_sender,
                transfers: transfer_sender,
            }),
        );
        let session_id = SessionId::new();
        manager.connect(server.id, session_id).await.unwrap();
        let connected = manager
            .trust_host_key(server.id, session_id, false)
            .await
            .unwrap();
        let generation = connected.session.unwrap().target.generation;
        Self {
            manager,
            session_id,
            generation,
            terminal,
            transfers,
        }
    }

    async fn terminal_roundtrip(&mut self, label: &str) {
        let marker = format!("SPARROW_{label}_{}", uuid::Uuid::new_v4().simple());
        let expected = format!("\u{1e}{marker}");
        self.manager
            .terminal_write(
                self.session_id,
                self.generation,
                format!("printf '\\036%s\\n' '{marker}'\n").into_bytes(),
            )
            .await
            .unwrap();
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(30), async {
            while !String::from_utf8_lossy(&bytes).contains(&expected) {
                let event = self.terminal.recv().await.expect("terminal stream closed");
                assert_eq!(event.target.session_id, self.session_id);
                assert_eq!(event.target.generation, self.generation);
                bytes.extend(event.data);
            }
        })
        .await
        .expect("terminal stopped responding during an auxiliary-operation fault");
    }

    async fn large_terminal_output_does_not_block_auxiliary_channels(&mut self) {
        let monitor = {
            let manager = self.manager.clone();
            let session_id = self.session_id;
            let generation = self.generation;
            tokio::spawn(async move { manager.monitor_snapshot(session_id, generation).await })
        };
        let listing = {
            let manager = self.manager.clone();
            let session_id = self.session_id;
            let generation = self.generation;
            tokio::spawn(async move {
                manager
                    .list_remote_directory(session_id, generation, None)
                    .await
            })
        };
        let marker = format!("SPARROW_LARGE_{}", uuid::Uuid::new_v4().simple());
        let expected = format!("\u{1e}{marker}");
        self.manager
            .terminal_write(
                self.session_id,
                self.generation,
                format!(
                    "head -c 4194304 /dev/zero | tr '\\000' x; printf '\\036%s\\n' '{marker}'\n"
                )
                .into_bytes(),
            )
            .await
            .unwrap();
        let mut received = 0_usize;
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(45), async {
            while !String::from_utf8_lossy(&bytes).contains(&expected) {
                let event = self.terminal.recv().await.expect("terminal stream closed");
                received = received.saturating_add(event.data.len());
                bytes.extend(event.data);
                if bytes.len() > 256 * 1024 {
                    let drain = bytes.len() - 64 * 1024;
                    bytes.drain(..drain);
                }
            }
        })
        .await
        .expect("large PTY output blocked the terminal");
        assert!(received >= 4 * 1024 * 1024);
        monitor.await.unwrap().unwrap();
        listing.await.unwrap().unwrap();
        self.terminal_roundtrip("AFTER_LARGE_OUTPUT").await;
    }

    async fn wait_for_transfer(&mut self, id: TransferId) -> TransferEvent {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let event = self.transfers.recv().await.expect("transfer stream closed");
                if event.transfer_id == id
                    && matches!(
                        event.state,
                        TransferState::Completed | TransferState::Failed | TransferState::Cancelled
                    )
                {
                    return event;
                }
            }
        })
        .await
        .expect("SFTP transfer did not reach a terminal state")
    }

    async fn disconnect(self) {
        self.manager.disconnect(self.session_id).await.unwrap();
    }
}

fn local_payload() -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/fault-injection");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(format!("payload-{}.bin", uuid::Uuid::new_v4().simple()))
}

/// Run only through tests/fault-injection/run.sh. Every scenario uses the
/// disposable container, never a developer's real Linux server.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the isolated fault-injection OpenSSH container"]
async fn auxiliary_faults_do_not_break_the_real_terminal() {
    let scenario = std::env::var("SPARROWSHELL_FAULT_SCENARIO")
        .expect("run through tests/fault-injection/run.sh");
    let mut workbench = Workbench::connect().await;
    workbench.terminal_roundtrip("BEFORE_FAULT").await;

    match scenario.as_str() {
        "latency" | "packet-loss" => {
            let monitor = {
                let manager = workbench.manager.clone();
                let session_id = workbench.session_id;
                let generation = workbench.generation;
                tokio::spawn(async move { manager.monitor_snapshot(session_id, generation).await })
            };
            workbench.terminal_roundtrip("DURING_NETWORK_FAULT").await;
            // Latency/loss may make a sample time out; it must never fabricate a success.
            if let Ok(snapshot) = monitor.await.unwrap() {
                assert_eq!(snapshot.target.generation, workbench.generation);
            }
        }
        "large-output" => {
            workbench
                .large_terminal_output_does_not_block_auxiliary_channels()
                .await;
        }
        "disk-full" => {
            let payload = local_payload();
            std::fs::write(&payload, vec![b'x'; 256 * 1024]).unwrap();
            let remote = format!(
                "/home/sparrow/full/fault-{}.bin",
                uuid::Uuid::new_v4().simple()
            );
            let transfer = workbench
                .manager
                .start_upload(
                    workbench.session_id,
                    workbench.generation,
                    payload.to_string_lossy().into_owned(),
                    remote,
                    false,
                )
                .await
                .unwrap();
            let result = workbench.wait_for_transfer(transfer).await;
            assert_eq!(result.state, TransferState::Failed);
            assert!(result.error_code.is_some());
            std::fs::remove_file(payload).unwrap();
            workbench.terminal_roundtrip("AFTER_DISK_FULL").await;
        }
        other => panic!("unknown fault scenario: {other}"),
    }

    workbench.terminal_roundtrip("AFTER_FAULT").await;
    workbench.disconnect().await;
}
