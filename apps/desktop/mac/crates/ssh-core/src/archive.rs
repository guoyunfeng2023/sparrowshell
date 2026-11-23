use super::{SessionManager, SshError};
use sparrowshell_domain::SessionId;
use std::time::Duration;

const ARCHIVE_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    Tar,
}

struct ExtractionCommand {
    destination: String,
    command: String,
}

impl SessionManager {
    pub async fn extract_remote_archive(
        &self,
        session_id: SessionId,
        generation: u64,
        path: String,
        separate_directory: bool,
        overwrite: bool,
    ) -> Result<String, SshError> {
        let extraction = extraction_command(&path, separate_directory, overwrite)?;
        let (_, transport) = self.transport(session_id, generation)?;
        let result = self
            .exec_fixed_with_timeout(transport, &extraction.command, ARCHIVE_TIMEOUT)
            .await?;
        match result.exit_status {
            Some(0) => Ok(extraction.destination),
            Some(69) => Err(SshError::RemoteArchiveToolMissing),
            Some(70) => Err(SshError::RemoteArchiveInvalid),
            Some(71) => Err(SshError::RemoteArchiveUnsafe),
            Some(73) => {
                let detail = result.stdout.trim();
                if detail.is_empty() {
                    Err(SshError::RemoteFileAlreadyExists)
                } else {
                    Err(SshError::RemoteArchiveConflict(
                        detail.chars().take(4096).collect(),
                    ))
                }
            }
            _ => {
                let detail = result.stdout.trim();
                let detail = if detail.is_empty() {
                    "服务器未返回详细错误".to_owned()
                } else {
                    detail.chars().take(4000).collect()
                };
                Err(SshError::RemoteArchiveFailed(detail))
            }
        }
    }
}

fn extraction_command(
    path: &str,
    separate_directory: bool,
    overwrite: bool,
) -> Result<ExtractionCommand, SshError> {
    validate_archive_path(path)?;
    let file_name = path.rsplit('/').next().ok_or(SshError::InvalidRemotePath)?;
    let lower_name = file_name.to_ascii_lowercase();
    let (kind, suffix_length) = if lower_name.ends_with(".zip") {
        (ArchiveKind::Zip, 4)
    } else if lower_name.ends_with(".tar.gz") {
        (ArchiveKind::Tar, 7)
    } else if lower_name.ends_with(".tgz") || lower_name.ends_with(".tar") {
        (ArchiveKind::Tar, 4)
    } else {
        return Err(SshError::RemoteArchiveUnsupported);
    };
    let base_name = &file_name[..file_name.len() - suffix_length];
    if base_name.is_empty() || matches!(base_name, "." | "..") {
        return Err(SshError::InvalidRemotePath);
    }
    let parent = match path.rfind('/') {
        Some(0) => "/",
        Some(index) => &path[..index],
        None => return Err(SshError::InvalidRemotePath),
    };
    let destination = if separate_directory {
        format!("{}/{base_name}", parent.trim_end_matches('/'))
    } else {
        parent.to_owned()
    };
    let archive = shell_quote(path);
    let quoted_destination = shell_quote(&destination);
    let quoted_parent = shell_quote(parent);
    let (tool, list_command, extract_command) = match (kind, overwrite) {
        (ArchiveKind::Zip, false) => (
            "unzip",
            "unzip -Z1 -- \"$archive\"",
            "unzip -n -q -- \"$archive\" -d \"$destination\" < /dev/null",
        ),
        (ArchiveKind::Zip, true) => (
            "unzip",
            "unzip -Z1 -- \"$archive\"",
            "unzip -o -q -- \"$archive\" -d \"$destination\" < /dev/null",
        ),
        (ArchiveKind::Tar, false) => (
            "tar",
            "tar -tf \"$archive\"",
            "tar --keep-old-files --no-overwrite-dir -xf \"$archive\" -C \"$destination\" < /dev/null",
        ),
        (ArchiveKind::Tar, true) => (
            "tar",
            "tar -tf \"$archive\"",
            "tar --overwrite -xf \"$archive\" -C \"$destination\" < /dev/null",
        ),
    };
    let collision_check = if separate_directory {
        ""
    } else if overwrite {
        concat!(
            "if [ -L \"$parent/$entry\" ]; then printf '%s\\n' \"$parent/$entry\"; exit 73; fi; ",
            "case \"$entry\" in ",
            "*/) if [ -e \"$parent/$entry\" ] && [ ! -d \"$parent/$entry\" ]; then printf '%s\\n' \"$parent/$entry\"; exit 73; fi;; ",
            "esac; "
        )
    } else {
        concat!(
            "case \"$entry\" in ",
            "*/*) target=\"$parent/${entry%%/*}\";; ",
            "*) target=\"$parent/$entry\";; ",
            "esac; ",
            "if [ -d \"$target\" ] && [ ! -L \"$target\" ]; then printf '%s\\n' \"$target/\"; exit 73; fi; ",
            "if [ -e \"$target\" ] || [ -L \"$target\" ]; then printf '%s\\n' \"$target\"; exit 73; fi; "
        )
    };
    let prepare_destination = if separate_directory && overwrite {
        concat!(
            "if [ -L \"$destination\" ]; then printf '%s\\n' \"$destination\"; exit 73; fi; ",
            "if [ -e \"$destination\" ] && [ ! -d \"$destination\" ]; then printf '%s\\n' \"$destination\"; exit 73; fi; ",
            "destination_created=0; ",
            "if [ ! -e \"$destination\" ]; then ",
            "mkdir -- \"$destination\" || exit $?; destination_created=1; fi; "
        )
    } else if separate_directory {
        concat!(
            "if [ -d \"$destination\" ] && [ ! -L \"$destination\" ]; then printf '%s\\n' \"$destination/\"; exit 73; fi; ",
            "if [ -e \"$destination\" ] || [ -L \"$destination\" ]; then printf '%s\\n' \"$destination\"; exit 73; fi; ",
            "mkdir -- \"$destination\" || exit $?; destination_created=1; "
        )
    } else {
        "destination_created=0; if [ ! -d \"$destination\" ]; then exit 67; fi; "
    };
    let cleanup = if separate_directory {
        "if [ \"$destination_created\" -eq 1 ]; then rm -rf -- \"$destination\"; fi; "
    } else {
        ""
    };
    let command = format!(
        concat!(
            "LC_ALL=C; export LC_ALL; archive={archive}; destination={destination}; parent={parent}; ",
            "if [ ! -f \"$archive\" ]; then exit 66; fi; ",
            "if ! command -v {tool} >/dev/null 2>&1; then exit 69; fi; ",
            "entries=$({list_command}) || exit 70; ",
            "printf '%s\\n' \"$entries\" | while IFS= read -r entry; do ",
            "entry=${{entry#./}}; [ -z \"$entry\" ] && continue; ",
            "case \"$entry\" in /*) exit 71;; esac; ",
            "case \"/$entry/\" in */../*) exit 71;; esac; ",
            "{collision_check}done || exit $?; ",
            "{prepare_destination}",
            "{extract_command}; status=$?; ",
            "if [ \"$status\" -ne 0 ]; then {cleanup}exit \"$status\"; fi; exit 0"
        ),
        archive = archive,
        destination = quoted_destination,
        parent = quoted_parent,
        tool = tool,
        list_command = list_command,
        collision_check = collision_check,
        prepare_destination = prepare_destination,
        extract_command = extract_command,
        cleanup = cleanup,
    );
    Ok(ExtractionCommand {
        destination,
        command,
    })
}

fn validate_archive_path(path: &str) -> Result<(), SshError> {
    if path.is_empty()
        || path.len() > 4096
        || !path.starts_with('/')
        || path.trim_matches('/').is_empty()
        || path.contains('\0')
        || path.split('/').any(|segment| matches!(segment, "." | ".."))
    {
        return Err(SshError::InvalidRemotePath);
    }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::{extraction_command, ArchiveKind};

    #[test]
    fn zip_uses_a_new_sibling_directory_and_never_overwrites_without_consent() {
        let extraction = extraction_command("/tmp/app release.zip", true, false).unwrap();
        assert_eq!(extraction.destination, "/tmp/app release");
        assert!(extraction.command.contains("command -v unzip"));
        assert!(extraction.command.contains("unzip -n -q"));
        assert!(extraction.command.contains("[ -e \"$destination\" ]"));
        assert!(extraction
            .command
            .contains("printf '%s\\n' \"$destination/\"; exit 73"));
        assert!(extraction.command.contains("'/tmp/app release.zip'"));
        assert!(extraction.command.contains("exit 71"));
    }

    #[test]
    fn tar_family_is_supported_case_insensitively() {
        for (path, destination) in [
            ("/root/app.tar", "/root/app"),
            ("/root/app.tar.gz", "/root/app"),
            ("/root/app.TGZ", "/root/app"),
        ] {
            let extraction = extraction_command(path, true, false).unwrap();
            assert_eq!(extraction.destination, destination);
            assert!(extraction.command.contains("tar --keep-old-files"));
        }
        assert_eq!(ArchiveKind::Tar, ArchiveKind::Tar);
    }

    #[test]
    fn current_directory_checks_top_level_targets_before_extracting() {
        let extraction = extraction_command("/root/app.zip", false, false).unwrap();
        assert_eq!(extraction.destination, "/root");
        assert!(extraction
            .command
            .contains("target=\"$parent/${entry%%/*}\""));
        assert!(extraction
            .command
            .contains("printf '%s\\n' \"$target/\"; exit 73"));
        assert!(extraction
            .command
            .contains("if [ -e \"$target\" ] || [ -L \"$target\" ]"));
        assert!(!extraction.command.contains("mkdir -- \"$destination\""));
    }

    #[test]
    fn explicit_overwrite_replaces_only_matching_entries() {
        let zip = extraction_command("/root/app.zip", false, true).unwrap();
        assert!(zip.command.contains("unzip -o -q"));
        assert!(!zip
            .command
            .contains("*) if [ -e \"$parent/$entry\" ]; then printf"));

        let tar = extraction_command("/root/app.tar.gz", true, true).unwrap();
        assert!(tar.command.contains("tar --overwrite"));
        assert!(tar
            .command
            .contains("if [ \"$destination_created\" -eq 1 ]"));
        assert!(!tar.command.contains("--keep-old-files"));
    }

    #[test]
    fn paths_are_data_not_shell_commands() {
        let extraction = extraction_command("/tmp/it's $(literal).zip", true, false).unwrap();
        assert!(extraction
            .command
            .contains("'/tmp/it'\"'\"'s $(literal).zip'"));
        for path in [
            "",
            "/",
            "relative.zip",
            "/tmp/../etc.zip",
            "/tmp/./etc.zip",
            "/tmp/no-extension.txt",
            "/tmp/.zip",
            "/tmp/bad\0.zip",
        ] {
            assert!(
                extraction_command(path, true, false).is_err(),
                "accepted {path:?}"
            );
        }
    }
}
