use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use sparrowshell_domain::{
    validate_automation_definition, AutomationDefinitionSyncDto, SyncEntityType,
};
use uuid::Uuid;

use crate::{current_account_id, Repository, StorageError};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRunRecord {
    pub id: Uuid,
    pub kind: String,
    pub state: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Repository {
    pub fn save_automation_definition(
        &self,
        entity_type: SyncEntityType,
        id: Option<Uuid>,
        name: &str,
        definition: serde_json::Value,
    ) -> Result<AutomationDefinitionSyncDto, StorageError> {
        let name = name.trim();
        if name.is_empty() || name.len() > 128 {
            return Err(StorageError::InvalidAutomationDefinition);
        }
        validate_automation_definition(entity_type, &definition)
            .map_err(|_| StorageError::InvalidAutomationDefinition)?;
        let id = id.unwrap_or_else(Uuid::new_v4);
        let now = Utc::now();
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO automation_definitions(id,kind,name,definition,revision,updated_at) VALUES(?1,?2,?3,?4,1,?5) ON CONFLICT(id) DO UPDATE SET name=excluded.name,definition=excluded.definition,revision=automation_definitions.revision+1,updated_at=excluded.updated_at,deleted_at=NULL",
            params![id.to_string(), entity_name(entity_type)?, name, definition.to_string(), now.to_rfc3339()],
        )?;
        let dto = definition_by_id(&tx, entity_type, id)?
            .ok_or(StorageError::InvalidAutomationDefinition)?;
        tx.execute(
            "INSERT INTO sync_outbox(mutation_id,entity_type,entity_id,operation,payload,base_revision,schema_version,created_at,account_id) VALUES(?1,?2,?3,'upsert',?4,?5,1,?6,?7)",
            params![Uuid::new_v4().to_string(), entity_name(entity_type)?, id.to_string(), serde_json::to_string(&dto).map_err(|_| StorageError::Serialization)?, dto.revision.saturating_sub(1), now.to_rfc3339(), current_account_id(&tx)?],
        )?;
        tx.commit()?;
        Ok(dto)
    }

    pub fn automation_definitions(
        &self,
        entity_type: SyncEntityType,
    ) -> Result<Vec<AutomationDefinitionSyncDto>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id,name,definition,revision,updated_at,deleted_at FROM automation_definitions WHERE kind=?1 AND deleted_at IS NULL ORDER BY name,id",
        )?;
        let rows = statement.query_map(params![entity_name(entity_type)?], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        rows.map(|row| {
            let (id, name, definition, revision, updated_at, deleted_at) = row?;
            Ok(AutomationDefinitionSyncDto {
                id: Uuid::parse_str(&id).map_err(|_| StorageError::InvalidSyncData)?,
                name,
                definition: serde_json::from_str(&definition)
                    .map_err(|_| StorageError::InvalidSyncData)?,
                revision,
                updated_at: parse_time(&updated_at)?,
                deleted_at: deleted_at.map(|value| parse_time(&value)).transpose()?,
            })
        })
        .collect()
    }

    pub fn delete_automation_definition(
        &self,
        entity_type: SyncEntityType,
        id: Uuid,
    ) -> Result<(), StorageError> {
        let tx = self.connection.unchecked_transaction()?;
        let mut dto = definition_by_id(&tx, entity_type, id)?
            .ok_or(StorageError::InvalidAutomationDefinition)?;
        let now = Utc::now();
        dto.revision = dto.revision.saturating_add(1);
        dto.updated_at = now;
        dto.deleted_at = Some(now);
        tx.execute(
            "UPDATE automation_definitions SET revision=?1,updated_at=?2,deleted_at=?2 WHERE id=?3 AND kind=?4",
            params![dto.revision, now.to_rfc3339(), id.to_string(), entity_name(entity_type)?],
        )?;
        tx.execute(
            "INSERT INTO sync_outbox(mutation_id,entity_type,entity_id,operation,payload,base_revision,schema_version,created_at,account_id) VALUES(?1,?2,?3,'delete',?4,?5,1,?6,?7)",
            params![Uuid::new_v4().to_string(), entity_name(entity_type)?, id.to_string(), serde_json::to_string(&dto).map_err(|_| StorageError::Serialization)?, dto.revision.saturating_sub(1), now.to_rfc3339(), current_account_id(&tx)?],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Local-only run history. This method deliberately never writes sync_outbox.
    pub fn save_automation_run(
        &self,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<(), StorageError> {
        if !matches!(kind, "batch" | "workflow")
            || serde_json::to_vec(&payload)
                .map_err(|_| StorageError::Serialization)?
                .len()
                > 2_097_152
        {
            return Err(StorageError::InvalidAutomationRun);
        }
        let id = payload
            .get("id")
            .and_then(|value| value.as_str())
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or(StorageError::InvalidAutomationRun)?;
        let state = payload
            .get("state")
            .and_then(|value| value.as_str())
            .ok_or(StorageError::InvalidAutomationRun)?;
        let created = payload
            .get("createdAt")
            .or_else(|| payload.get("startedAt"))
            .and_then(|value| value.as_str())
            .ok_or(StorageError::InvalidAutomationRun)?;
        DateTime::parse_from_rfc3339(created).map_err(|_| StorageError::InvalidAutomationRun)?;
        self.connection.execute(
            "INSERT INTO automation_runs(id,kind,state,payload,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET state=excluded.state,payload=excluded.payload,updated_at=excluded.updated_at",
            params![id.to_string(), kind, state, payload.to_string(), created, Utc::now().to_rfc3339()],
        )?;
        self.connection.execute(
            "DELETE FROM automation_runs WHERE id NOT IN (SELECT id FROM automation_runs ORDER BY created_at DESC,id DESC LIMIT 500)",
            [],
        )?;
        Ok(())
    }

    pub fn automation_runs(&self, limit: u32) -> Result<Vec<AutomationRunRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id,kind,state,payload,created_at,updated_at FROM automation_runs ORDER BY created_at DESC,id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit.clamp(1, 500)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        rows.map(|row| {
            let (id, kind, state, payload, created_at, updated_at) = row?;
            Ok(AutomationRunRecord {
                id: Uuid::parse_str(&id).map_err(|_| StorageError::InvalidAutomationRun)?,
                kind,
                state,
                payload: serde_json::from_str(&payload)
                    .map_err(|_| StorageError::InvalidAutomationRun)?,
                created_at: parse_time(&created_at)?,
                updated_at: parse_time(&updated_at)?,
            })
        })
        .collect()
    }
}

fn definition_by_id(
    connection: &Connection,
    entity_type: SyncEntityType,
    id: Uuid,
) -> Result<Option<AutomationDefinitionSyncDto>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT id,name,definition,revision,updated_at,deleted_at FROM automation_definitions WHERE id=?1 AND kind=?2",
    )?;
    let mut rows = statement.query(params![id.to_string(), entity_name(entity_type)?])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    Ok(Some(AutomationDefinitionSyncDto {
        id,
        name: row.get(1)?,
        definition: serde_json::from_str(&row.get::<_, String>(2)?)
            .map_err(|_| StorageError::InvalidSyncData)?,
        revision: row.get(3)?,
        updated_at: parse_time(&row.get::<_, String>(4)?)?,
        deleted_at: row
            .get::<_, Option<String>>(5)?
            .map(|value| parse_time(&value))
            .transpose()?,
    }))
}

pub(crate) fn entity_name(entity_type: SyncEntityType) -> Result<&'static str, StorageError> {
    match entity_type {
        SyncEntityType::BatchTemplate => Ok("batch_template"),
        SyncEntityType::WorkflowDefinition => Ok("workflow_definition"),
        _ => Err(StorageError::InvalidAutomationDefinition),
    }
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, StorageError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| StorageError::InvalidSyncData)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sparrowshell_domain::{SyncChange, SyncPullPage};

    fn batch_definition() -> serde_json::Value {
        serde_json::json!({"command":"uptime","serverIds":[Uuid::new_v4()],"maxConcurrent":2})
    }

    #[test]
    fn definitions_sync_but_runtime_and_secret_fields_are_rejected() {
        let repository = Repository::in_memory().unwrap();
        let saved = repository
            .save_automation_definition(
                SyncEntityType::BatchTemplate,
                None,
                "Daily health",
                batch_definition(),
            )
            .unwrap();
        assert_eq!(saved.revision, 1);
        assert_eq!(repository.outbox_len().unwrap(), 1);
        let mutation = repository.outbox_batch(10).unwrap().remove(0);
        assert_eq!(mutation.entity_type, SyncEntityType::BatchTemplate);
        for forbidden in ["sessionId", "generation", "password", "stdout"] {
            let mut invalid = batch_definition();
            invalid[forbidden] = serde_json::Value::String("canary".into());
            assert!(repository
                .save_automation_definition(SyncEntityType::BatchTemplate, None, "invalid", invalid)
                .is_err());
        }
        assert_eq!(repository.outbox_len().unwrap(), 1);
    }

    #[test]
    fn remote_workflow_definition_applies_without_runtime_state() {
        let repository = Repository::in_memory().unwrap();
        let dto = AutomationDefinitionSyncDto {
            id: Uuid::new_v4(),
            name: "Health workflow".into(),
            definition: serde_json::json!({"definitionId":Uuid::new_v4(),"version":1,"startStep":"check","steps":[],"maxTransitions":20}),
            revision: 1,
            updated_at: Utc::now(),
            deleted_at: None,
        };
        repository
            .apply_remote_page(&SyncPullPage {
                changes: vec![SyncChange {
                    change_seq: 1,
                    entity_type: SyncEntityType::WorkflowDefinition,
                    entity_id: dto.id,
                    revision: 1,
                    deleted: false,
                    data: serde_json::to_value(&dto).unwrap(),
                }],
                next_cursor: 1,
                has_more: false,
                snapshot_required: false,
            })
            .unwrap();
        assert_eq!(
            repository
                .automation_definitions(SyncEntityType::WorkflowDefinition)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(repository.outbox_len().unwrap(), 0);
    }

    #[test]
    fn run_history_is_persistent_local_only_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("automation.db");
        let id = Uuid::new_v4();
        {
            let repository = Repository::open(&path).unwrap();
            repository.save_automation_run("batch", serde_json::json!({"id":id,"state":"running","createdAt":Utc::now().to_rfc3339()})).unwrap();
            assert_eq!(repository.outbox_len().unwrap(), 0);
        }
        let repository = Repository::open(&path).unwrap();
        let history = repository.automation_runs(10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].state, "running");
        assert_eq!(repository.outbox_len().unwrap(), 0);
    }
}
