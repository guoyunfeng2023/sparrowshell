mod transfer;

pub use transfer::*;

use russh_sftp::{
    client::SftpSession,
    protocol::{FileType, OpenFlags},
};
use serde::{Deserialize, Serialize};
use sparrowshell_domain::TargetRef;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_PATH_BYTES: usize = 4096;
const MAX_ENTRIES: usize = 2000;
pub const MAX_TEXT_FILE_BYTES: usize = 1024 * 1024;
const TEXT_CHUNK_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteFileKind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub kind: RemoteFileKind,
    pub size: Option<u64>,
    pub modified_at: Option<i64>,
    pub permissions: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDirectory {
    pub target: TargetRef,
    pub path: String,
    pub entries: Vec<RemoteEntry>,
    pub truncated: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SftpError {
    #[error("远程路径或文件名无效")]
    InvalidPath,
    #[error("远程文件操作失败")]
    OperationFailed,
    #[error("文件类型已变化，操作已取消")]
    FileTypeChanged,
    #[error("目标已存在，未覆盖")]
    AlreadyExists,
    #[error("远程文本文件不能超过 1 MB")]
    TextFileTooLarge,
    #[error("远程文件包含二进制内容，不能作为文本编辑")]
    BinaryFile,
    #[error("远程文件不是有效的 UTF-8 文本")]
    InvalidUtf8,
}

pub async fn list(
    session: &SftpSession,
    target: TargetRef,
    path: Option<&str>,
) -> Result<RemoteDirectory, SftpError> {
    let requested = path.unwrap_or(".");
    validate_path(requested)?;
    let path = session
        .canonicalize(requested)
        .await
        .map_err(|_| SftpError::OperationFailed)?;
    let read_dir = session
        .read_dir(path.clone())
        .await
        .map_err(|_| SftpError::OperationFailed)?;
    let mut entries = read_dir
        .map(|entry| {
            let metadata = entry.metadata();
            RemoteEntry {
                name: entry.file_name(),
                path: entry.path(),
                kind: kind(entry.file_type()),
                size: metadata.size,
                modified_at: metadata.mtime.map(i64::from),
                permissions: metadata.permissions,
            }
        })
        .collect::<Vec<_>>();
    let truncated = entries.len() > MAX_ENTRIES;
    entries.truncate(MAX_ENTRIES);
    entries.sort_by(|left, right| {
        (
            left.kind != RemoteFileKind::Directory,
            left.name.to_lowercase(),
        )
            .cmp(&(
                right.kind != RemoteFileKind::Directory,
                right.name.to_lowercase(),
            ))
    });
    Ok(RemoteDirectory {
        target,
        path,
        entries,
        truncated,
    })
}

pub async fn create_directory(
    session: &SftpSession,
    parent: &str,
    name: &str,
) -> Result<(), SftpError> {
    validate_path(parent)?;
    validate_name(name)?;
    let path = join(parent, name);
    session
        .create_dir(path)
        .await
        .map_err(|_| SftpError::OperationFailed)
}

pub async fn create_file(session: &SftpSession, parent: &str, name: &str) -> Result<(), SftpError> {
    validate_path(parent)?;
    validate_name(name)?;
    let path = join(parent, name);
    if session.symlink_metadata(&path).await.is_ok() {
        return Err(SftpError::AlreadyExists);
    }
    let file = session
        .open_with_flags(
            path,
            OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
        )
        .await
        .map_err(|_| SftpError::OperationFailed)?;
    file.close().await.map_err(|_| SftpError::OperationFailed)
}

pub async fn read_text_file(session: &SftpSession, path: &str) -> Result<String, SftpError> {
    validate_path(path)?;
    verify_kind(session, path, RemoteFileKind::File).await?;
    let metadata = session
        .symlink_metadata(path)
        .await
        .map_err(|_| SftpError::OperationFailed)?;
    if metadata
        .size
        .is_some_and(|size| size > MAX_TEXT_FILE_BYTES as u64)
    {
        return Err(SftpError::TextFileTooLarge);
    }
    let mut file = session
        .open(path)
        .await
        .map_err(|_| SftpError::OperationFailed)?;
    let mut bytes = Vec::new();
    let mut buffer = vec![0_u8; TEXT_CHUNK_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|_| SftpError::OperationFailed)?;
        if read == 0 {
            break;
        }
        if bytes.len().saturating_add(read) > MAX_TEXT_FILE_BYTES {
            let _ = file.close().await;
            return Err(SftpError::TextFileTooLarge);
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    file.close().await.map_err(|_| SftpError::OperationFailed)?;
    decode_text(bytes)
}

pub async fn write_text_file(
    session: &SftpSession,
    path: &str,
    content: &str,
) -> Result<(), SftpError> {
    validate_path(path)?;
    validate_text_bytes(content.as_bytes())?;
    verify_kind(session, path, RemoteFileKind::File).await?;
    let mut file = session
        .open_with_flags(path, OpenFlags::WRITE | OpenFlags::TRUNCATE)
        .await
        .map_err(|_| SftpError::OperationFailed)?;
    if file.write_all(content.as_bytes()).await.is_err() {
        let _ = file.close().await;
        return Err(SftpError::OperationFailed);
    }
    if file.sync_all().await.is_err() {
        let _ = file.close().await;
        return Err(SftpError::OperationFailed);
    }
    file.close().await.map_err(|_| SftpError::OperationFailed)
}

fn decode_text(bytes: Vec<u8>) -> Result<String, SftpError> {
    validate_text_bytes(&bytes)?;
    String::from_utf8(bytes).map_err(|_| SftpError::InvalidUtf8)
}

fn validate_text_bytes(bytes: &[u8]) -> Result<(), SftpError> {
    if bytes.len() > MAX_TEXT_FILE_BYTES {
        return Err(SftpError::TextFileTooLarge);
    }
    if bytes.contains(&0) {
        return Err(SftpError::BinaryFile);
    }
    std::str::from_utf8(bytes)
        .map(|_| ())
        .map_err(|_| SftpError::InvalidUtf8)
}

pub async fn verify_removal_target(
    session: &SftpSession,
    path: &str,
    expected_kind: RemoteFileKind,
) -> Result<(), SftpError> {
    validate_path(path)?;
    if path == "/" || expected_kind == RemoteFileKind::Other {
        return Err(SftpError::InvalidPath);
    }
    verify_kind(session, path, expected_kind).await
}

pub async fn remove(
    session: &SftpSession,
    path: &str,
    expected_kind: RemoteFileKind,
) -> Result<(), SftpError> {
    validate_path(path)?;
    if path == "/" || expected_kind == RemoteFileKind::Other {
        return Err(SftpError::InvalidPath);
    }
    let metadata = session
        .symlink_metadata(path)
        .await
        .map_err(|_| SftpError::OperationFailed)?;
    let actual_kind = kind(metadata.file_type());
    if actual_kind != expected_kind {
        return Err(SftpError::FileTypeChanged);
    }
    match actual_kind {
        RemoteFileKind::Directory => session.remove_dir(path).await,
        RemoteFileKind::File | RemoteFileKind::Symlink => session.remove_file(path).await,
        RemoteFileKind::Other => return Err(SftpError::InvalidPath),
    }
    .map_err(|_| SftpError::OperationFailed)
}

pub async fn rename(
    session: &SftpSession,
    path: &str,
    new_name: &str,
    expected_kind: RemoteFileKind,
) -> Result<(), SftpError> {
    validate_path(path)?;
    validate_name(new_name)?;
    if path == "/" || expected_kind == RemoteFileKind::Other {
        return Err(SftpError::InvalidPath);
    }
    verify_kind(session, path, expected_kind).await?;
    let parent = parent(path).ok_or(SftpError::InvalidPath)?;
    rename_checked(session, path, &join(parent, new_name)).await
}

pub async fn move_to(
    session: &SftpSession,
    path: &str,
    destination_directory: &str,
    expected_kind: RemoteFileKind,
) -> Result<(), SftpError> {
    validate_path(path)?;
    validate_path(destination_directory)?;
    if path == "/"
        || !destination_directory.starts_with('/')
        || expected_kind == RemoteFileKind::Other
    {
        return Err(SftpError::InvalidPath);
    }
    verify_kind(session, path, expected_kind).await?;
    let destination_directory = session
        .canonicalize(destination_directory)
        .await
        .map_err(|_| SftpError::OperationFailed)?;
    let destination_metadata = session
        .metadata(destination_directory.clone())
        .await
        .map_err(|_| SftpError::OperationFailed)?;
    if kind(destination_metadata.file_type()) != RemoteFileKind::Directory {
        return Err(SftpError::InvalidPath);
    }
    if expected_kind == RemoteFileKind::Directory {
        let source = session
            .canonicalize(path)
            .await
            .map_err(|_| SftpError::OperationFailed)?;
        if destination_is_within_source(&source, &destination_directory) {
            return Err(SftpError::InvalidPath);
        }
    }
    let name = file_name(path).ok_or(SftpError::InvalidPath)?;
    rename_checked(session, path, &join(&destination_directory, name)).await
}

async fn verify_kind(
    session: &SftpSession,
    path: &str,
    expected_kind: RemoteFileKind,
) -> Result<(), SftpError> {
    let metadata = session
        .symlink_metadata(path)
        .await
        .map_err(|_| SftpError::OperationFailed)?;
    if kind(metadata.file_type()) == expected_kind {
        Ok(())
    } else {
        Err(SftpError::FileTypeChanged)
    }
}

async fn rename_checked(
    session: &SftpSession,
    source: &str,
    destination: &str,
) -> Result<(), SftpError> {
    if source == destination {
        return Ok(());
    }
    if session.symlink_metadata(destination).await.is_ok() {
        return Err(SftpError::AlreadyExists);
    }
    session
        .rename(source, destination)
        .await
        .map_err(|_| SftpError::OperationFailed)
}

fn parent(path: &str) -> Option<&str> {
    let index = path.rfind('/')?;
    if index == 0 {
        Some("/")
    } else {
        Some(&path[..index])
    }
}

fn file_name(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next()?;
    if name.is_empty() || matches!(name, "." | "..") {
        None
    } else {
        Some(name)
    }
}

fn destination_is_within_source(source: &str, destination: &str) -> bool {
    source == destination || destination.starts_with(&format!("{}/", source.trim_end_matches('/')))
}

fn kind(value: FileType) -> RemoteFileKind {
    match value {
        FileType::Dir => RemoteFileKind::Directory,
        FileType::File => RemoteFileKind::File,
        FileType::Symlink => RemoteFileKind::Symlink,
        FileType::Other => RemoteFileKind::Other,
    }
}

fn validate_path(path: &str) -> Result<(), SftpError> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES || path.contains('\0') {
        Err(SftpError::InvalidPath)
    } else {
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<(), SftpError> {
    if name.is_empty()
        || name.len() > 255
        || matches!(name, "." | "..")
        || name.contains(['/', '\0'])
    {
        Err(SftpError::InvalidPath)
    } else {
        Ok(())
    }
}

fn join(parent: &str, name: &str) -> String {
    if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_cannot_escape_the_selected_directory() {
        assert!(validate_name("reports").is_ok());
        assert!(validate_name("中文目录").is_ok());
        for invalid in ["", ".", "..", "../other", "a/b", "bad\0name"] {
            assert!(validate_name(invalid).is_err());
        }
    }

    #[test]
    fn posix_join_preserves_root_separator() {
        assert_eq!(join("/", "tmp"), "/tmp");
        assert_eq!(join("/home/user", "tmp"), "/home/user/tmp");
    }

    #[test]
    fn rename_paths_keep_the_original_parent() {
        assert_eq!(parent("/var/log/app.log"), Some("/var/log"));
        assert_eq!(parent("/app.log"), Some("/"));
        assert_eq!(parent("relative"), None);
        assert_eq!(file_name("/var/log/app.log"), Some("app.log"));
        assert_eq!(file_name("/var/log/"), None);
    }

    #[test]
    fn directories_cannot_move_inside_themselves() {
        assert!(destination_is_within_source("/var/www", "/var/www"));
        assert!(destination_is_within_source("/var/www", "/var/www/archive"));
        assert!(!destination_is_within_source("/var/www", "/var/www-old"));
        assert!(!destination_is_within_source("/var/www", "/tmp"));
    }

    #[test]
    fn text_editor_accepts_utf8_up_to_one_mebibyte() {
        assert_eq!(
            decode_text("中文\nEnglish\n".as_bytes().to_vec()).unwrap(),
            "中文\nEnglish\n"
        );
        assert!(validate_text_bytes(&vec![b'a'; MAX_TEXT_FILE_BYTES]).is_ok());
        assert!(matches!(
            validate_text_bytes(&vec![b'a'; MAX_TEXT_FILE_BYTES + 1]),
            Err(SftpError::TextFileTooLarge)
        ));
    }

    #[test]
    fn text_editor_rejects_binary_and_invalid_utf8() {
        assert!(matches!(
            decode_text(b"before\0after".to_vec()),
            Err(SftpError::BinaryFile)
        ));
        assert!(matches!(
            decode_text(vec![0xf0, 0x28, 0x8c, 0x28]),
            Err(SftpError::InvalidUtf8)
        ));
    }
}
