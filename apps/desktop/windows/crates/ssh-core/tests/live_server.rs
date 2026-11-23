//! Live end-to-end validation against a real Linux server (阶段 1 验收).
//!
//! Driven by environment variables; skips silently when unset:
//!   SPARROWSHELL_LIVE_HOST      e.g. 192.168.1.10
//!   SPARROWSHELL_LIVE_PORT      default 22
//!   SPARROWSHELL_LIVE_USER      e.g. root
//!   SPARROWSHELL_LIVE_PASSWORD  password for the account
//!
//! Run: cargo test -p sparrowshell-ssh-core --test live_server -- --nocapture

use sparrowshell_domain::{AuthType, ServerInput, SessionId, SessionSnapshot};
use sparrowshell_secrets::{CredentialRef, CredentialStore, MemoryCredentialStore};
use sparrowshell_sftp_core::RemoteFileKind;
use sparrowshell_ssh_core::{EventSink, SessionManager, TerminalEvent};
use sparrowshell_storage::Repository;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
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

#[tokio::test(flavor = "multi_thread")]
async fn live_password_server_full_roundtrip() {
    let host = match std::env::var("SPARROWSHELL_LIVE_HOST") {
        Ok(host) if !host.is_empty() => host,
        _ => {
            eprintln!("SPARROWSHELL_LIVE_HOST 未设置，跳过真机验收");
            return;
        }
    };
    let port: u16 = std::env::var("SPARROWSHELL_LIVE_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(22);
    let username = std::env::var("SPARROWSHELL_LIVE_USER").expect("SPARROWSHELL_LIVE_USER");
    let password = std::env::var("SPARROWSHELL_LIVE_PASSWORD").expect("SPARROWSHELL_LIVE_PASSWORD");

    let repository = Arc::new(Mutex::new(Repository::in_memory().unwrap()));
    let server = repository
        .lock()
        .unwrap()
        .upsert_server(ServerInput {
            id: None,
            folder_id: None,
            name: "live".into(),
            host,
            port,
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

    let (sender, mut receiver) = mpsc::channel(256);
    let manager = SessionManager::new(
        repository,
        credentials,
        Arc::new(TestEvents {
            sender: Mutex::new(sender),
        }),
    );
    let session_id = SessionId::new();

    // 1. 首次连接触发 TOFU 主机密钥确认
    let first = manager.connect(server.id, session_id).await.unwrap();
    let challenge = first
        .host_key_challenge
        .expect("首次连接必须要求确认主机密钥");
    println!("TOFU 主机密钥指纹: {}", challenge.fingerprint);

    // 2. 信任主机密钥后建立会话
    let connected = manager
        .trust_host_key(server.id, session_id, false)
        .await
        .unwrap();
    let session = connected.session.expect("信任后必须建立 PTY 会话");
    let generation = session.target.generation;
    println!("会话已建立: generation={generation}");

    // 3. 终端输入命令并读到回显输出
    manager
        .terminal_write(session_id, generation, b"echo SPARROW_WINDOWS_PHASE1_OK\n".to_vec())
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut output = Vec::new();
    while Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(5), receiver.recv()).await {
            Ok(Some(event)) => output.extend_from_slice(&event.data),
            _ => {}
        }
        if String::from_utf8_lossy(&output).contains("SPARROW_WINDOWS_PHASE1_OK") {
            break;
        }
    }
    let echoed = String::from_utf8_lossy(&output);
    assert!(
        echoed.contains("SPARROW_WINDOWS_PHASE1_OK"),
        "终端输出应包含命令回显，实际: {echoed}"
    );
    println!("终端回显正常（{} 字节）", output.len());

    // 4. 监控快照：CPU / 内存 / 磁盘 / 进程
    let snapshot = manager
        .monitor_snapshot(session_id, generation)
        .await
        .unwrap();
    println!(
        "监控快照: {} {} ({}), CPU {:.1}%, 内存 {}/{} MiB, 磁盘 {} 项, 进程 {} 个",
        snapshot.hostname,
        snapshot.os_name,
        snapshot.kernel,
        snapshot.cpu_usage_percent,
        snapshot.memory_used_bytes / 1024 / 1024,
        snapshot.memory_total_bytes / 1024 / 1024,
        snapshot.disks.len(),
        snapshot.processes.len()
    );
    assert!(!snapshot.hostname.is_empty());
    assert!(snapshot.memory_total_bytes > 0);

    // 5. SFTP 列目录
    let root = manager
        .list_remote_directory(session_id, generation, Some("/".into()))
        .await
        .unwrap();
    assert!(!root.entries.is_empty());
    let names: Vec<_> = root.entries.iter().map(|entry| entry.name.clone()).collect();
    println!("根目录条目: {}", names.join(", "));

    // 6. SFTP 新建 / 写入 / 读取 / 删除 远程文件
    let test_dir = "/tmp";
    let test_name = "sparrowshell_windows_phase1.txt";
    let test_path = "/tmp/sparrowshell_windows_phase1.txt";
    let content = "SparrowShell Windows 阶段1验收 ✓";
    manager
        .create_remote_file(session_id, generation, test_dir.into(), test_name.into())
        .await
        .unwrap();
    manager
        .write_remote_text_file(
            session_id,
            generation,
            test_path.into(),
            content.into(),
        )
        .await
        .unwrap();
    let read_back = manager
        .read_remote_text_file(session_id, generation, test_path.into())
        .await
        .unwrap();
    assert_eq!(read_back.trim(), content);
    println!("远程文件读写一致: {test_path}");
    manager
        .remove_remote_entry(session_id, generation, test_path.into(), RemoteFileKind::File)
        .await
        .unwrap();

    // 7. 断开并清理
    manager.disconnect(session_id).await.unwrap();
    println!("阶段 1 真机验收全部通过 ✓");
}

/// 跳板机链路：目标服务器经跳板机（jump host）建立会话。
/// 环境变量：
///   SPARROWSHELL_LIVE_JUMP_HOST / _PORT / _USER / _PASSWORD   跳板机
///   SPARROWSHELL_LIVE_HOST / _PORT / _USER / _PASSWORD        目标服务器
#[tokio::test(flavor = "multi_thread")]
async fn live_jump_host_roundtrip() {
    let target_host = match std::env::var("SPARROWSHELL_LIVE_HOST") {
        Ok(host) if !host.is_empty() => host,
        _ => {
            eprintln!("SPARROWSHELL_LIVE_HOST 未设置，跳过跳板机验收");
            return;
        }
    };
    let jump_host = std::env::var("SPARROWSHELL_LIVE_JUMP_HOST").expect("SPARROWSHELL_LIVE_JUMP_HOST");
    let parse_port = |key: &str| -> u16 {
        std::env::var(key)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(22)
    };
    let target_port = parse_port("SPARROWSHELL_LIVE_PORT");
    let jump_port = parse_port("SPARROWSHELL_LIVE_JUMP_PORT");
    let target_user = std::env::var("SPARROWSHELL_LIVE_USER").expect("SPARROWSHELL_LIVE_USER");
    let target_password =
        std::env::var("SPARROWSHELL_LIVE_PASSWORD").expect("SPARROWSHELL_LIVE_PASSWORD");
    let jump_user =
        std::env::var("SPARROWSHELL_LIVE_JUMP_USER").expect("SPARROWSHELL_LIVE_JUMP_USER");
    let jump_password =
        std::env::var("SPARROWSHELL_LIVE_JUMP_PASSWORD").expect("SPARROWSHELL_LIVE_JUMP_PASSWORD");

    let repository = Arc::new(Mutex::new(Repository::in_memory().unwrap()));
    let credentials = Arc::new(MemoryCredentialStore::default());

    // 先登记跳板机
    let jump = repository
        .lock()
        .unwrap()
        .upsert_server(ServerInput {
            id: None,
            folder_id: None,
            name: "jump".into(),
            host: jump_host,
            port: jump_port,
            username: jump_user,
            auth_type: AuthType::Password,
            private_key_path_hint: None,
            jump_server_id: None,
        })
        .unwrap();
    let jump_ref = CredentialRef::new_ssh();
    credentials
        .save(&jump_ref, Zeroizing::new(jump_password))
        .unwrap();
    repository
        .lock()
        .unwrap()
        .set_credential_ref(jump.id, Some(jump_ref.as_str().into()))
        .unwrap();

    // 目标服务器引用跳板机
    let target = repository
        .lock()
        .unwrap()
        .upsert_server(ServerInput {
            id: None,
            folder_id: None,
            name: "target".into(),
            host: target_host,
            port: target_port,
            username: target_user,
            auth_type: AuthType::Password,
            private_key_path_hint: None,
            jump_server_id: Some(jump.id),
        })
        .unwrap();
    let target_ref = CredentialRef::new_ssh();
    credentials
        .save(&target_ref, Zeroizing::new(target_password))
        .unwrap();
    repository
        .lock()
        .unwrap()
        .set_credential_ref(target.id, Some(target_ref.as_str().into()))
        .unwrap();

    let (sender, mut receiver) = mpsc::channel(256);
    let manager = SessionManager::new(
        repository,
        credentials,
        Arc::new(TestEvents {
            sender: Mutex::new(sender),
        }),
    );
    let session_id = SessionId::new();

    // 跳板链路上的每台主机都需要各自完成 TOFU 确认
    let mut result = manager.connect(target.id, session_id).await.unwrap();
    let mut challenges = 0;
    while result.session.is_none() {
        let challenge = result
            .host_key_challenge
            .expect("会话未建立时必须给出主机密钥挑战");
        challenges += 1;
        println!(
            "TOFU #{}: {}:{} 指纹 {}",
            challenges, challenge.host, challenge.port, challenge.fingerprint
        );
        assert!(challenges <= 4, "主机密钥挑战次数异常");
        result = manager
            .trust_host_key(target.id, session_id, false)
            .await
            .unwrap();
    }
    let session = result.session.unwrap();
    let generation = session.target.generation;
    println!("跳板机会话已建立（共 {} 次 TOFU 确认）", challenges);

    // 终端回环验证命令真的落在目标服务器上
    manager
        .terminal_write(session_id, generation, b"hostname\n".to_vec())
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut output = Vec::new();
    while Instant::now() < deadline {
        if let Ok(Some(event)) =
            tokio::time::timeout(Duration::from_secs(5), receiver.recv()).await
        {
            output.extend_from_slice(&event.data);
        }
        let text = String::from_utf8_lossy(&output);
        if text.lines().filter(|line| !line.trim().is_empty()).count() >= 2 {
            break;
        }
    }
    let echoed = String::from_utf8_lossy(&output);
    println!("目标服务器终端输出:\n{echoed}");
    assert!(echoed.contains("hostname"), "终端应回显已发送的命令");

    // 监控快照验证目标机数据可读
    let snapshot = manager
        .monitor_snapshot(session_id, generation)
        .await
        .unwrap();
    println!(
        "目标服务器: {} {} ({}), CPU {:.1}%, 内存 {}/{} MiB",
        snapshot.hostname,
        snapshot.os_name,
        snapshot.kernel,
        snapshot.cpu_usage_percent,
        snapshot.memory_used_bytes / 1024 / 1024,
        snapshot.memory_total_bytes / 1024 / 1024,
    );
    assert!(snapshot.memory_total_bytes > 0);

    manager.disconnect(session_id).await.unwrap();
    println!("跳板机链路验收通过 ✓");
}
