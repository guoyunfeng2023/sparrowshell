use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use sparrowshell_domain::{
    validate_automation_definition, AutomationDefinitionSyncDto, FolderSyncDto, ServerSyncDto,
    SyncApplyOutcome, SyncChange, SyncEntityType, SyncMutation, SyncPullPage,
};
use uuid::Uuid;

use crate::{Repository, StorageError};

#[derive(Debug, Clone)]
pub struct ApplyRemoteResult {
    pub outcome: SyncApplyOutcome,
    pub credential_ref: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflictRecord {
    pub conflict_id: Uuid,
    pub entity_type: SyncEntityType,
    pub entity_id: Uuid,
    pub local_payload: serde_json::Value,
    pub remote_payload: serde_json::Value,
    pub remote_revision: u64,
    pub detected_at: DateTime<Utc>,
}

pub(crate) fn migrate(connection: &Connection) -> Result<(), StorageError> {
    ensure_column(connection, "folders", "deleted_at", "TEXT")?;
    ensure_column(
        connection,
        "servers",
        "sort_order",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS servers_folder_order ON servers(folder_id, sort_order, name)",
        [],
    )?;
    ensure_column(
        connection,
        "sync_outbox",
        "base_revision",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "sync_outbox",
        "schema_version",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    ensure_column(connection, "sync_outbox", "account_id", "TEXT")?;
    ensure_column(connection, "sync_conflicts", "account_id", "TEXT")?;
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS sync_outbox_account_created ON sync_outbox(account_id,created_at,mutation_id);
         CREATE INDEX IF NOT EXISTS sync_conflicts_account_entity ON sync_conflicts(account_id,entity_type,entity_id,resolved_at);
         CREATE TABLE IF NOT EXISTS sync_cursors (
             account_id TEXT PRIMARY KEY,
             device_id TEXT NOT NULL,
             cursor INTEGER NOT NULL DEFAULT 0,
             updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS workspace_snapshots (
             snapshot_id TEXT PRIMARY KEY,
             account_id TEXT,
             payload TEXT NOT NULL,
             created_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS workspace_snapshots_account_created ON workspace_snapshots(account_id,created_at);",
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO sync_state(id,device_id,cursor,updated_at) VALUES(1,?1,0,?2)",
        params![Uuid::new_v4().to_string(), Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &'static str,
    column: &'static str,
    definition: &'static str,
) -> Result<(), rusqlite::Error> {
    let sql = format!("PRAGMA table_info({table})");
    let mut statement = connection.prepare(&sql)?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !names.iter().any(|name| name == column) {
        connection.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))?;
    }
    Ok(())
}

impl Repository {
    pub fn cloud_base_url(&self) -> Result<Option<String>, StorageError> {
        self.connection
            .query_row(
                "SELECT value FROM settings WHERE key='cloud_base_url'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn set_cloud_base_url(&self, value: &str) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO settings(key,value,updated_at) VALUES('cloud_base_url',?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
            params![value, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn sync_device_id(&self) -> Result<Uuid, StorageError> {
        let value: String = self.connection.query_row(
            "SELECT device_id FROM sync_state WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        Uuid::parse_str(&value).map_err(|_| StorageError::InvalidSyncData)
    }

    pub fn workspace_owner(&self) -> Result<Option<String>, StorageError> {
        self.connection
            .query_row(
                "SELECT value FROM settings WHERE key='workspace_owner_id'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn set_workspace_owner(&self, account_id: &str) -> Result<(), StorageError> {
        set_workspace_owner(&self.connection, account_id)
    }

    pub fn sync_cursor(&self) -> Result<u64, StorageError> {
        self.sync_cursor_for(self.workspace_owner()?.as_deref())
    }

    pub fn sync_cursor_for(&self, account_id: Option<&str>) -> Result<u64, StorageError> {
        match account_id {
            Some(account_id) => Ok(self
                .connection
                .query_row(
                    "SELECT cursor FROM sync_cursors WHERE account_id=?1",
                    params![account_id],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or(0)),
            None => Ok(self.connection.query_row(
                "SELECT cursor FROM sync_state WHERE id=1",
                [],
                |row| row.get(0),
            )?),
        }
    }

    pub fn outbox_batch(&self, limit: usize) -> Result<Vec<SyncMutation>, StorageError> {
        self.outbox_batch_for(self.workspace_owner()?.as_deref(), limit)
    }

    pub fn outbox_batch_for(
        &self,
        account_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SyncMutation>, StorageError> {
        let limit = limit.clamp(1, 100) as i64;
        let mut statement = self.connection.prepare(
            "SELECT mutation_id,entity_type,entity_id,operation,payload,base_revision,schema_version
             FROM sync_outbox o WHERE o.account_id IS ?1
             AND NOT EXISTS (
                 SELECT 1 FROM sync_conflicts c
                 WHERE c.account_id IS o.account_id AND c.entity_type=o.entity_type
                 AND c.entity_id=o.entity_id AND c.resolved_at IS NULL
             )
             ORDER BY created_at,mutation_id LIMIT ?2",
        )?;
        let mutations = statement
            .query_map(params![account_id, limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, u64>(5)?,
                    row.get::<_, u16>(6)?,
                ))
            })?
            .map(|row| {
                let (
                    mutation_id,
                    entity_type,
                    entity_id,
                    operation,
                    payload,
                    base_revision,
                    schema_version,
                ) = row?;
                let payload_value: serde_json::Value =
                    serde_json::from_str(&payload).map_err(|_| StorageError::InvalidSyncData)?;
                let payload_revision = payload_value.get("revision").and_then(|v| v.as_u64());
                Ok(SyncMutation {
                    mutation_id: Uuid::parse_str(&mutation_id)
                        .map_err(|_| StorageError::InvalidSyncData)?,
                    entity_type: parse_entity_type(&entity_type)?,
                    entity_id: Uuid::parse_str(&entity_id)
                        .map_err(|_| StorageError::InvalidSyncData)?,
                    base_revision: if base_revision == 0 {
                        payload_revision.unwrap_or(1).saturating_sub(1)
                    } else {
                        base_revision
                    },
                    schema_version,
                    deleted: operation == "delete",
                    data: payload_value,
                })
            })
            .collect::<Result<Vec<_>, StorageError>>()?;
        Ok(mutations)
    }

    pub fn acknowledge_mutation(&self, mutation_id: Uuid) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM sync_outbox WHERE mutation_id=?1",
            params![mutation_id.to_string()],
        )?;
        Ok(())
    }

    pub fn record_push_conflict(
        &self,
        mutation_id: Uuid,
        current: &SyncChange,
    ) -> Result<(), StorageError> {
        validate_change(current)?;
        let tx = self.connection.unchecked_transaction()?;
        let (local, account_id): (String, Option<String>) = tx
            .query_row(
                "SELECT payload,account_id FROM sync_outbox WHERE mutation_id=?1",
                params![mutation_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or(StorageError::InvalidSyncData)?;
        tx.execute(
            "INSERT INTO sync_conflicts(conflict_id,entity_type,entity_id,local_payload,remote_payload,remote_revision,detected_at,account_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![Uuid::new_v4().to_string(), entity_type_name(current.entity_type), current.entity_id.to_string(), local, serde_json::to_string(current).map_err(|_| StorageError::Serialization)?, current.revision, Utc::now().to_rfc3339(), account_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn apply_remote_page(
        &self,
        page: &SyncPullPage,
    ) -> Result<Vec<ApplyRemoteResult>, StorageError> {
        self.apply_remote_page_for(self.workspace_owner()?.as_deref(), page)
    }

    pub fn apply_remote_page_for(
        &self,
        account_id: Option<&str>,
        page: &SyncPullPage,
    ) -> Result<Vec<ApplyRemoteResult>, StorageError> {
        self.apply_remote_pages_for(account_id, std::slice::from_ref(page))
    }

    /// Applies a complete pull batch atomically. Cloud changes are ordered by
    /// sequence, not by references, so foreign keys are checked at commit after
    /// every page has supplied its folders, servers, and jump-server targets.
    pub fn apply_remote_pages_for(
        &self,
        account_id: Option<&str>,
        pages: &[SyncPullPage],
    ) -> Result<Vec<ApplyRemoteResult>, StorageError> {
        if pages.iter().any(|page| page.snapshot_required) {
            return Err(StorageError::InvalidSyncData);
        }
        let result = self.apply_remote_pages_inner(account_id, pages);
        self.connection
            .execute("PRAGMA defer_foreign_keys=OFF", [])?;
        result
    }

    fn apply_remote_pages_inner(
        &self,
        account_id: Option<&str>,
        pages: &[SyncPullPage],
    ) -> Result<Vec<ApplyRemoteResult>, StorageError> {
        let mut previous_seq = self.sync_cursor_for(account_id)?;
        let tx = self.connection.unchecked_transaction()?;
        tx.execute("PRAGMA defer_foreign_keys=ON", [])?;
        let mut results = Vec::with_capacity(pages.iter().map(|page| page.changes.len()).sum());
        for page in pages {
            for change in &page.changes {
                if change.change_seq <= previous_seq {
                    return Err(StorageError::InvalidSyncData);
                }
                previous_seq = change.change_seq;
                results.push(apply_change(&tx, account_id, change)?);
            }
            if page.next_cursor < previous_seq {
                return Err(StorageError::InvalidSyncData);
            }
            set_sync_cursor(&tx, account_id, page.next_cursor)?;
            previous_seq = page.next_cursor;
        }
        tx.commit()?;
        Ok(results)
    }

    pub fn unresolved_conflicts(&self) -> Result<Vec<SyncConflictRecord>, StorageError> {
        let account_id = self.workspace_owner()?;
        let mut statement = self.connection.prepare(
            "SELECT conflict_id,entity_type,entity_id,local_payload,remote_payload,remote_revision,detected_at FROM sync_conflicts WHERE account_id IS ?1 AND resolved_at IS NULL ORDER BY detected_at,conflict_id",
        )?;
        let records = statement
            .query_map(params![account_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, u64>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .map(|row| {
                let (id, entity_type, entity_id, local, remote, revision, detected_at) = row?;
                Ok(SyncConflictRecord {
                    conflict_id: Uuid::parse_str(&id).map_err(|_| StorageError::InvalidSyncData)?,
                    entity_type: parse_entity_type(&entity_type)?,
                    entity_id: Uuid::parse_str(&entity_id)
                        .map_err(|_| StorageError::InvalidSyncData)?,
                    local_payload: serde_json::from_str(&local)
                        .map_err(|_| StorageError::InvalidSyncData)?,
                    remote_payload: serde_json::from_str(&remote)
                        .map_err(|_| StorageError::InvalidSyncData)?,
                    remote_revision: revision,
                    detected_at: DateTime::parse_from_rfc3339(&detected_at)
                        .map(|value| value.with_timezone(&Utc))
                        .map_err(|_| StorageError::InvalidSyncData)?,
                })
            })
            .collect::<Result<Vec<_>, StorageError>>()?;
        Ok(records)
    }

    /// Explicit user choice: discard only this entity's pending mutations and apply the cloud version.
    pub fn resolve_conflict_keep_remote(
        &self,
        conflict_id: Uuid,
    ) -> Result<ApplyRemoteResult, StorageError> {
        let account_id = self.workspace_owner()?;
        let tx = self.connection.unchecked_transaction()?;
        let (entity_type, entity_id, remote): (String, String, String) = tx.query_row(
            "SELECT entity_type,entity_id,remote_payload FROM sync_conflicts WHERE conflict_id=?1 AND account_id IS ?2 AND resolved_at IS NULL",
            params![conflict_id.to_string(), account_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional()?.ok_or(StorageError::ConflictNotFound)?;
        tx.execute(
            "DELETE FROM sync_outbox WHERE account_id IS ?1 AND entity_type=?2 AND entity_id=?3",
            params![account_id, entity_type, entity_id],
        )?;
        let remote: SyncChange =
            serde_json::from_str(&remote).map_err(|_| StorageError::InvalidSyncData)?;
        let result = apply_change_force(&tx, account_id.as_deref(), &remote, true)?;
        tx.execute(
            "UPDATE sync_conflicts SET resolved_at=?1 WHERE conflict_id=?2",
            params![Utc::now().to_rfc3339(), conflict_id.to_string()],
        )?;
        tx.commit()?;
        Ok(result)
    }

    /// Explicit user choice: rebase the latest local snapshot on the current cloud revision.
    pub fn resolve_conflict_keep_local(&self, conflict_id: Uuid) -> Result<(), StorageError> {
        let account_id = self.workspace_owner()?;
        let tx = self.connection.unchecked_transaction()?;
        let (entity_type, entity_id, remote_revision): (String, String, u64) = tx.query_row(
            "SELECT entity_type,entity_id,remote_revision FROM sync_conflicts WHERE conflict_id=?1 AND account_id IS ?2 AND resolved_at IS NULL",
            params![conflict_id.to_string(), account_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional()?.ok_or(StorageError::ConflictNotFound)?;
        let latest: Option<(String, String)> = tx.query_row(
            "SELECT operation,payload FROM sync_outbox WHERE account_id IS ?1 AND entity_type=?2 AND entity_id=?3 ORDER BY created_at DESC,mutation_id DESC LIMIT 1",
            params![account_id, entity_type, entity_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
        let (operation, payload) = latest.ok_or(StorageError::InvalidSyncData)?;
        let mut payload: serde_json::Value =
            serde_json::from_str(&payload).map_err(|_| StorageError::InvalidSyncData)?;
        payload["revision"] = serde_json::Value::from(remote_revision.saturating_add(1));
        tx.execute(
            "DELETE FROM sync_outbox WHERE account_id IS ?1 AND entity_type=?2 AND entity_id=?3",
            params![account_id, entity_type, entity_id],
        )?;
        tx.execute(
            "INSERT INTO sync_outbox(mutation_id,entity_type,entity_id,operation,payload,base_revision,schema_version,created_at,account_id) VALUES(?1,?2,?3,?4,?5,?6,1,?7,?8)",
            params![Uuid::new_v4().to_string(), entity_type, entity_id, operation, payload.to_string(), remote_revision, Utc::now().to_rfc3339(), account_id],
        )?;
        tx.execute(
            "UPDATE sync_conflicts SET resolved_at=?1 WHERE conflict_id=?2",
            params![Utc::now().to_rfc3339(), conflict_id.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }
}

pub(crate) fn set_workspace_owner(
    connection: &Connection,
    account_id: &str,
) -> Result<(), StorageError> {
    if account_id.trim().is_empty() {
        return Err(StorageError::InvalidSyncData);
    }
    connection.execute(
        "INSERT INTO settings(key,value,updated_at) VALUES('workspace_owner_id',?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
        params![account_id, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

pub(crate) fn set_sync_cursor(
    connection: &Connection,
    account_id: Option<&str>,
    cursor: u64,
) -> Result<(), StorageError> {
    match account_id {
        Some(account_id) => {
            let device_id: String =
                connection.query_row("SELECT device_id FROM sync_state WHERE id=1", [], |row| {
                    row.get(0)
                })?;
            connection.execute(
                "INSERT INTO sync_cursors(account_id,device_id,cursor,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(account_id) DO UPDATE SET cursor=excluded.cursor,updated_at=excluded.updated_at",
                params![account_id, device_id, cursor, Utc::now().to_rfc3339()],
            )?;
        }
        None => {
            connection.execute(
                "UPDATE sync_state SET cursor=?1,updated_at=?2 WHERE id=1",
                params![cursor, Utc::now().to_rfc3339()],
            )?;
        }
    }
    Ok(())
}

pub(crate) fn apply_change(
    tx: &Connection,
    account_id: Option<&str>,
    change: &SyncChange,
) -> Result<ApplyRemoteResult, StorageError> {
    apply_change_force(tx, account_id, change, false)
}

pub(crate) fn apply_change_force(
    tx: &Connection,
    account_id: Option<&str>,
    change: &SyncChange,
    force: bool,
) -> Result<ApplyRemoteResult, StorageError> {
    if change.revision == 0 || change.change_seq == 0 {
        return Err(StorageError::InvalidSyncData);
    }
    validate_change(change)?;
    let entity_type = entity_type_name(change.entity_type);
    let entity_id = change.entity_id.to_string();
    let pending: Option<String> = tx.query_row(
        "SELECT payload FROM sync_outbox WHERE account_id IS ?1 AND entity_type=?2 AND entity_id=?3 ORDER BY created_at DESC,mutation_id DESC LIMIT 1",
        params![account_id, entity_type, entity_id],
        |row| row.get(0),
    ).optional()?;
    if let Some(local) = pending {
        tx.execute(
            "INSERT INTO sync_conflicts(conflict_id,entity_type,entity_id,local_payload,remote_payload,remote_revision,detected_at,account_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![Uuid::new_v4().to_string(), entity_type, entity_id, local, serde_json::to_string(change).map_err(|_| StorageError::Serialization)?, change.revision, Utc::now().to_rfc3339(), account_id],
        )?;
        return Ok(ApplyRemoteResult {
            outcome: SyncApplyOutcome::Conflict,
            credential_ref: None,
        });
    }

    let current_revision: Option<u64> = match change.entity_type {
        SyncEntityType::Folder => tx.query_row(
            "SELECT revision FROM folders WHERE id=?1",
            params![entity_id],
            |row| row.get(0),
        ),
        SyncEntityType::Server => tx.query_row(
            "SELECT revision FROM servers WHERE id=?1",
            params![entity_id],
            |row| row.get(0),
        ),
        SyncEntityType::BatchTemplate | SyncEntityType::WorkflowDefinition => tx.query_row(
            "SELECT revision FROM automation_definitions WHERE id=?1 AND kind=?2",
            params![entity_id, entity_type_name(change.entity_type)],
            |row| row.get(0),
        ),
    }
    .optional()?;
    if !force && current_revision.is_some_and(|revision| revision >= change.revision) {
        return Ok(ApplyRemoteResult {
            outcome: SyncApplyOutcome::Duplicate,
            credential_ref: None,
        });
    }

    let credential_ref = match change.entity_type {
        SyncEntityType::Folder => {
            let dto: FolderSyncDto = serde_json::from_value(change.data.clone())
                .map_err(|_| StorageError::InvalidSyncData)?;
            tx.execute(
                "INSERT INTO folders(id,parent_id,name,sort_order,revision,updated_at,deleted_at) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET parent_id=excluded.parent_id,name=excluded.name,sort_order=excluded.sort_order,revision=excluded.revision,updated_at=excluded.updated_at,deleted_at=excluded.deleted_at",
                params![dto.id.to_string(), dto.parent_id.map(|id| id.to_string()), dto.name, dto.sort_order, dto.revision, dto.updated_at.to_rfc3339(), dto.deleted_at.map(|value| value.to_rfc3339())],
            )?;
            None
        }
        SyncEntityType::Server => {
            let dto: ServerSyncDto = serde_json::from_value(change.data.clone())
                .map_err(|_| StorageError::InvalidSyncData)?;
            let previous: Option<String> = tx
                .query_row(
                    "SELECT credential_ref FROM servers WHERE id=?1",
                    params![dto.id.to_string()],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
            let retained = if dto.deleted_at.is_some() {
                None
            } else {
                previous.clone()
            };
            tx.execute(
                "INSERT INTO servers(id,folder_id,name,host,port,username,auth_type,private_key_path_hint,jump_server_id,sort_order,credential_ref,revision,updated_at,deleted_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14) ON CONFLICT(id) DO UPDATE SET folder_id=excluded.folder_id,name=excluded.name,host=excluded.host,port=excluded.port,username=excluded.username,auth_type=excluded.auth_type,private_key_path_hint=excluded.private_key_path_hint,jump_server_id=excluded.jump_server_id,sort_order=excluded.sort_order,credential_ref=excluded.credential_ref,revision=excluded.revision,updated_at=excluded.updated_at,deleted_at=excluded.deleted_at",
                params![dto.id.to_string(), dto.folder_id.map(|id| id.to_string()), dto.name, dto.host, dto.port, dto.username, if dto.auth_type == sparrowshell_domain::AuthType::Password { "password" } else { "private_key" }, dto.private_key_path_hint, dto.jump_server_id.map(|id| id.to_string()), dto.sort_order, retained, dto.revision, dto.updated_at.to_rfc3339(), dto.deleted_at.map(|value| value.to_rfc3339())],
            )?;
            if change.deleted {
                previous
            } else {
                None
            }
        }
        SyncEntityType::BatchTemplate | SyncEntityType::WorkflowDefinition => {
            let dto: AutomationDefinitionSyncDto = serde_json::from_value(change.data.clone())
                .map_err(|_| StorageError::InvalidSyncData)?;
            tx.execute(
                "INSERT INTO automation_definitions(id,kind,name,definition,revision,updated_at,deleted_at) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET kind=excluded.kind,name=excluded.name,definition=excluded.definition,revision=excluded.revision,updated_at=excluded.updated_at,deleted_at=excluded.deleted_at",
                params![dto.id.to_string(), entity_type_name(change.entity_type), dto.name, dto.definition.to_string(), dto.revision, dto.updated_at.to_rfc3339(), dto.deleted_at.map(|value| value.to_rfc3339())],
            )?;
            None
        }
    };
    Ok(ApplyRemoteResult {
        outcome: SyncApplyOutcome::Applied,
        credential_ref,
    })
}

fn validate_change(change: &SyncChange) -> Result<(), StorageError> {
    match change.entity_type {
        SyncEntityType::Folder => {
            let dto: FolderSyncDto = serde_json::from_value(change.data.clone())
                .map_err(|_| StorageError::InvalidSyncData)?;
            if dto.id.0 != change.entity_id
                || dto.revision != change.revision
                || dto.deleted_at.is_some() != change.deleted
            {
                return Err(StorageError::InvalidSyncData);
            }
        }
        SyncEntityType::Server => {
            let dto: ServerSyncDto = serde_json::from_value(change.data.clone())
                .map_err(|_| StorageError::InvalidSyncData)?;
            if dto.id.0 != change.entity_id
                || dto.revision != change.revision
                || dto.deleted_at.is_some() != change.deleted
            {
                return Err(StorageError::InvalidSyncData);
            }
        }
        SyncEntityType::BatchTemplate | SyncEntityType::WorkflowDefinition => {
            let dto: AutomationDefinitionSyncDto = serde_json::from_value(change.data.clone())
                .map_err(|_| StorageError::InvalidSyncData)?;
            if dto.id != change.entity_id
                || dto.revision != change.revision
                || dto.deleted_at.is_some() != change.deleted
                || dto.name.trim().is_empty()
                || dto.name.len() > 128
                || validate_automation_definition(change.entity_type, &dto.definition).is_err()
            {
                return Err(StorageError::InvalidSyncData);
            }
        }
    }
    Ok(())
}

fn parse_entity_type(value: &str) -> Result<SyncEntityType, StorageError> {
    match value {
        "folder" => Ok(SyncEntityType::Folder),
        "server" => Ok(SyncEntityType::Server),
        "batch_template" => Ok(SyncEntityType::BatchTemplate),
        "workflow_definition" => Ok(SyncEntityType::WorkflowDefinition),
        _ => Err(StorageError::InvalidSyncData),
    }
}

pub(crate) fn entity_type_name(value: SyncEntityType) -> &'static str {
    match value {
        SyncEntityType::Folder => "folder",
        SyncEntityType::Server => "server",
        SyncEntityType::BatchTemplate => "batch_template",
        SyncEntityType::WorkflowDefinition => "workflow_definition",
    }
}
