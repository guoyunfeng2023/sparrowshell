use super::{join, kind, validate_name, validate_path, RemoteFileKind};
use russh_sftp::{client::SftpSession, protocol::OpenFlags};
use serde::{Deserialize, Serialize};
use sparrowshell_domain::{TargetRef, TransferId};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

const CHUNK_BYTES: usize = 128 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    Upload,
    Download,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransferEvent {
    pub transfer_id: TransferId,
    pub target: TargetRef,
    pub direction: TransferDirection,
    pub source: String,
    pub destination: String,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub bytes_per_second: u64,
    pub state: TransferState,
    pub error_code: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TransferError {
    #[error("目标已存在，未覆盖")]
    AlreadyExists,
    #[error("传输已取消")]
    Cancelled,
    #[error("路径或文件类型无效")]
    InvalidPath,
    #[error("传输失败，临时文件已尽力清理")]
    OperationFailed,
}

pub async fn upload<F>(
    session: &SftpSession,
    local_path: &str,
    remote_path: &str,
    overwrite: bool,
    cancelled: Arc<AtomicBool>,
    mut progress: F,
) -> Result<u64, TransferError>
where
    F: FnMut(u64, u64),
{
    validate_path(remote_path).map_err(|_| TransferError::InvalidPath)?;
    let local = local_file(local_path).await?;
    let total = local.metadata.len();
    progress(0, total);
    check_cancelled(&cancelled)?;
    if let Ok(metadata) = session.symlink_metadata(remote_path).await {
        if !overwrite {
            return Err(TransferError::AlreadyExists);
        }
        if kind(metadata.file_type()) != RemoteFileKind::File {
            return Err(TransferError::InvalidPath);
        }
    }
    let temporary = remote_temporary_path(remote_path)?;
    let result = async {
        let mut source = local.file;
        let mut destination = session
            .open_with_flags(
                temporary.clone(),
                OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
            )
            .await
            .map_err(|_| TransferError::OperationFailed)?;
        let mut buffer = vec![0_u8; CHUNK_BYTES];
        let mut done = 0_u64;
        loop {
            check_cancelled(&cancelled)?;
            let read = source
                .read(&mut buffer)
                .await
                .map_err(|_| TransferError::OperationFailed)?;
            if read == 0 {
                break;
            }
            destination
                .write_all(&buffer[..read])
                .await
                .map_err(|_| TransferError::OperationFailed)?;
            done = done.saturating_add(read as u64);
            progress(done, total);
        }
        destination
            .sync_all()
            .await
            .map_err(|_| TransferError::OperationFailed)?;
        destination
            .close()
            .await
            .map_err(|_| TransferError::OperationFailed)?;
        if done != total {
            return Err(TransferError::OperationFailed);
        }
        check_cancelled(&cancelled)?;
        if overwrite {
            session
                .rename(temporary.clone(), remote_path)
                .await
                .map_err(|_| TransferError::OperationFailed)?;
        } else {
            let linked = session
                .hardlink(temporary.clone(), remote_path)
                .await
                .map_err(|_| TransferError::OperationFailed)?;
            if !linked {
                return Err(TransferError::OperationFailed);
            }
            session
                .remove_file(temporary.clone())
                .await
                .map_err(|_| TransferError::OperationFailed)?;
        }
        Ok(done)
    }
    .await;
    if result.is_err() {
        let _ = session.remove_file(temporary).await;
    }
    result
}

pub async fn download<F>(
    session: &SftpSession,
    remote_path: &str,
    local_path: &str,
    overwrite: bool,
    cancelled: Arc<AtomicBool>,
    mut progress: F,
) -> Result<u64, TransferError>
where
    F: FnMut(u64, u64),
{
    validate_path(remote_path).map_err(|_| TransferError::InvalidPath)?;
    let metadata = session
        .symlink_metadata(remote_path)
        .await
        .map_err(|_| TransferError::OperationFailed)?;
    if kind(metadata.file_type()) != RemoteFileKind::File {
        return Err(TransferError::InvalidPath);
    }
    let expected_size = metadata.size;
    let total = expected_size.unwrap_or(0);
    progress(0, total);
    let destination = local_destination(local_path, overwrite).await?;
    let temporary = local_temporary_path(&destination)?;
    let result = async {
        check_cancelled(&cancelled)?;
        let mut source = session
            .open(remote_path)
            .await
            .map_err(|_| TransferError::OperationFailed)?;
        let mut target = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .await
            .map_err(|_| TransferError::OperationFailed)?;
        let mut buffer = vec![0_u8; CHUNK_BYTES];
        let mut done = 0_u64;
        loop {
            check_cancelled(&cancelled)?;
            let read = source
                .read(&mut buffer)
                .await
                .map_err(|_| TransferError::OperationFailed)?;
            if read == 0 {
                break;
            }
            target
                .write_all(&buffer[..read])
                .await
                .map_err(|_| TransferError::OperationFailed)?;
            done = done.saturating_add(read as u64);
            progress(done, total);
        }
        source
            .close()
            .await
            .map_err(|_| TransferError::OperationFailed)?;
        target
            .sync_all()
            .await
            .map_err(|_| TransferError::OperationFailed)?;
        drop(target);
        if expected_size.is_some_and(|expected| expected != done) {
            return Err(TransferError::OperationFailed);
        }
        check_cancelled(&cancelled)?;
        if overwrite {
            tokio::fs::rename(&temporary, &destination)
                .await
                .map_err(|_| TransferError::OperationFailed)?;
        } else {
            tokio::fs::hard_link(&temporary, &destination)
                .await
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        TransferError::AlreadyExists
                    } else {
                        TransferError::OperationFailed
                    }
                })?;
            tokio::fs::remove_file(&temporary)
                .await
                .map_err(|_| TransferError::OperationFailed)?;
        }
        Ok(done)
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
}

struct LocalSource {
    file: tokio::fs::File,
    metadata: std::fs::Metadata,
}

async fn local_file(path: &str) -> Result<LocalSource, TransferError> {
    let path = absolute_path(path)?;
    let metadata = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| TransferError::InvalidPath)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(TransferError::InvalidPath);
    }
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| TransferError::OperationFailed)?;
    Ok(LocalSource { file, metadata })
}

async fn local_destination(path: &str, overwrite: bool) -> Result<PathBuf, TransferError> {
    let path = absolute_path(path)?;
    if path.file_name().is_none() || path.parent().is_none() {
        return Err(TransferError::InvalidPath);
    }
    match tokio::fs::symlink_metadata(&path).await {
        Ok(metadata) => {
            if !overwrite {
                return Err(TransferError::AlreadyExists);
            }
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(TransferError::InvalidPath);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(TransferError::OperationFailed),
    }
    Ok(path)
}

fn absolute_path(path: &str) -> Result<PathBuf, TransferError> {
    validate_path(path).map_err(|_| TransferError::InvalidPath)?;
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        Err(TransferError::InvalidPath)
    } else {
        Ok(path)
    }
}

fn local_temporary_path(destination: &Path) -> Result<PathBuf, TransferError> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(TransferError::InvalidPath)?;
    Ok(destination.with_file_name(format!(
        ".{name}.sparrowshell-{}.part",
        Uuid::new_v4().simple()
    )))
}

fn remote_temporary_path(destination: &str) -> Result<String, TransferError> {
    let (parent, name) = destination
        .rsplit_once('/')
        .ok_or(TransferError::InvalidPath)?;
    validate_name(name).map_err(|_| TransferError::InvalidPath)?;
    Ok(join(
        if parent.is_empty() { "/" } else { parent },
        &format!(".{name}.sparrowshell-{}.part", Uuid::new_v4().simple()),
    ))
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), TransferError> {
    if cancelled.load(Ordering::Acquire) {
        Err(TransferError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_paths_never_equal_the_destination() {
        let local = local_temporary_path(Path::new("/tmp/report.txt")).unwrap();
        assert_ne!(local, PathBuf::from("/tmp/report.txt"));
        assert!(local.to_string_lossy().ends_with(".part"));
        let remote = remote_temporary_path("/home/user/report.txt").unwrap();
        assert_ne!(remote, "/home/user/report.txt");
        assert!(remote.starts_with("/home/user/.report.txt.sparrowshell-"));
    }

    #[test]
    fn remote_temporary_path_rejects_relative_destination() {
        assert!(remote_temporary_path("report.txt").is_err());
    }

    #[test]
    fn cancellation_is_observed_without_io() {
        let cancelled = Arc::new(AtomicBool::new(false));
        assert!(check_cancelled(&cancelled).is_ok());
        cancelled.store(true, Ordering::Release);
        assert_eq!(check_cancelled(&cancelled), Err(TransferError::Cancelled));
    }

    #[tokio::test]
    async fn local_destination_requires_explicit_overwrite_without_changing_data() {
        let directory = std::env::temp_dir().join(format!("sparrowshell-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir(&directory).await.unwrap();
        let destination = directory.join("report.txt");
        tokio::fs::write(&destination, b"original").await.unwrap();
        let path = destination.to_str().unwrap();
        assert_eq!(
            local_destination(path, false).await,
            Err(TransferError::AlreadyExists)
        );
        assert_eq!(local_destination(path, true).await, Ok(destination.clone()));
        assert_eq!(tokio::fs::read(&destination).await.unwrap(), b"original");
        tokio::fs::remove_dir_all(directory).await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_destination_rejects_symlinks_even_when_overwrite_is_requested() {
        use std::os::unix::fs::symlink;

        let directory = std::env::temp_dir().join(format!("sparrowshell-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir(&directory).await.unwrap();
        let target = directory.join("target.txt");
        let destination = directory.join("link.txt");
        tokio::fs::write(&target, b"original").await.unwrap();
        symlink(&target, &destination).unwrap();
        assert_eq!(
            local_destination(destination.to_str().unwrap(), true).await,
            Err(TransferError::InvalidPath)
        );
        assert_eq!(tokio::fs::read(&target).await.unwrap(), b"original");
        tokio::fs::remove_dir_all(directory).await.unwrap();
    }
}
