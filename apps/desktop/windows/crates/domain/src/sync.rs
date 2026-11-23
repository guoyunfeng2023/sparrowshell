use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Folder, FolderId, LocalServerRecord, ServerSyncDto};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncEntityType {
    Folder,
    Server,
    BatchTemplate,
    WorkflowDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationDefinitionSyncDto {
    pub id: Uuid,
    pub name: String,
    pub definition: serde_json::Value,
    pub revision: u64,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

pub fn validate_automation_definition(
    entity_type: SyncEntityType,
    definition: &serde_json::Value,
) -> Result<(), crate::DomainError> {
    if !matches!(
        entity_type,
        SyncEntityType::BatchTemplate | SyncEntityType::WorkflowDefinition
    ) || !definition.is_object()
        || serde_json::to_vec(definition)
            .map_err(|_| crate::DomainError::InvalidAutomationDefinition)?
            .len()
            > 131_072
    {
        return Err(crate::DomainError::InvalidAutomationDefinition);
    }
    let required: &[&str] = match entity_type {
        SyncEntityType::BatchTemplate => &["command", "serverIds", "maxConcurrent"],
        SyncEntityType::WorkflowDefinition => &[
            "definitionId",
            "version",
            "startStep",
            "steps",
            "maxTransitions",
        ],
        _ => return Err(crate::DomainError::InvalidAutomationDefinition),
    };
    if required.iter().any(|key| definition.get(*key).is_none()) {
        return Err(crate::DomainError::InvalidAutomationDefinition);
    }
    fn visit(value: &serde_json::Value) -> Result<(), crate::DomainError> {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, child) in fields {
                    if matches!(
                        key.as_str(),
                        "sessionId"
                            | "session_id"
                            | "generation"
                            | "target"
                            | "targets"
                            | "runId"
                            | "run_id"
                            | "executionId"
                            | "execution_id"
                            | "state"
                            | "stdout"
                            | "stderr"
                            | "exitStatus"
                            | "exit_status"
                            | "startedAt"
                            | "started_at"
                            | "finishedAt"
                            | "finished_at"
                            | "password"
                            | "passphrase"
                            | "privateKey"
                            | "private_key"
                            | "credentialRef"
                            | "credential_ref"
                            | "token"
                            | "secret"
                    ) {
                        return Err(crate::DomainError::InvalidAutomationDefinition);
                    }
                    visit(child)?;
                }
                Ok(())
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    visit(item)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    visit(definition)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderSyncDto {
    pub id: FolderId,
    pub parent_id: Option<FolderId>,
    pub name: String,
    pub sort_order: i64,
    pub revision: u64,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl FolderSyncDto {
    pub fn from_parts(folder: &Folder, deleted_at: Option<DateTime<Utc>>) -> Self {
        Self {
            id: folder.id,
            parent_id: folder.parent_id,
            name: folder.name.clone(),
            sort_order: folder.sort_order,
            revision: folder.revision,
            updated_at: folder.updated_at,
            deleted_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncMutation {
    pub mutation_id: Uuid,
    pub entity_type: SyncEntityType,
    pub entity_id: Uuid,
    pub base_revision: u64,
    pub schema_version: u16,
    pub deleted: bool,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncChange {
    pub change_seq: u64,
    pub entity_type: SyncEntityType,
    pub entity_id: Uuid,
    pub revision: u64,
    pub deleted: bool,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncPushResult {
    pub mutation_id: Uuid,
    pub accepted: bool,
    pub revision: Option<u64>,
    pub change_seq: Option<u64>,
    pub current: Option<SyncChange>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncPullPage {
    pub changes: Vec<SyncChange>,
    pub next_cursor: u64,
    pub has_more: bool,
    pub snapshot_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncApplyOutcome {
    Applied,
    Duplicate,
    Conflict,
}

pub fn sync_dto(
    entity_type: SyncEntityType,
    folder: Option<&Folder>,
    server: Option<&LocalServerRecord>,
) -> Option<serde_json::Value> {
    match (entity_type, folder, server) {
        (SyncEntityType::Folder, Some(folder), None) => {
            serde_json::to_value(FolderSyncDto::from_parts(folder, None)).ok()
        }
        (SyncEntityType::Server, None, Some(server)) => {
            serde_json::to_value(ServerSyncDto::from(server)).ok()
        }
        _ => None,
    }
}
