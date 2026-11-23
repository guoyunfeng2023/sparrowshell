use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use sparrowshell_domain::{
    AutomationDefinitionSyncDto, CatalogSnapshot, FolderSyncDto, ServerSyncDto, SyncChange,
    SyncEntityType,
};
use std::collections::HashMap;
use uuid::Uuid;

use crate::sync::{apply_change, entity_type_name, set_sync_cursor, set_workspace_owner};
use crate::{Repository, StorageError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountSwitchChoice {
    CloudReplaceLocal,
    LocalReplaceCloud,
    Merge,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceSnapshot {
    pub catalog: CatalogSnapshot,
    pub automations: Vec<AutomationDefinitionSyncDto>,
}

#[derive(Debug, Clone)]
struct LocalEntity {
    entity_type: SyncEntityType,
    entity_id: Uuid,
    data: serde_json::Value,
}

impl Repository {
    pub fn workspace_snapshot(&self) -> Result<WorkspaceSnapshot, StorageError> {
        let mut automations = self.automation_definitions(SyncEntityType::BatchTemplate)?;
        automations.extend(self.automation_definitions(SyncEntityType::WorkflowDefinition)?);
        Ok(WorkspaceSnapshot {
            catalog: self.snapshot()?,
            automations,
        })
    }

    /// Archives only non-sensitive DTOs. LocalServerRecord skips credential_ref during serialization.
    pub fn archive_workspace(&self) -> Result<Uuid, StorageError> {
        let snapshot = self.workspace_snapshot()?;
        let payload = serde_json::to_string(&snapshot).map_err(|_| StorageError::Serialization)?;
        let snapshot_id = Uuid::new_v4();
        self.connection.execute(
            "INSERT INTO workspace_snapshots(snapshot_id,account_id,payload,created_at) VALUES(?1,?2,?3,?4)",
            params![snapshot_id.to_string(), self.workspace_owner()?, payload, Utc::now().to_rfc3339()],
        )?;
        Ok(snapshot_id)
    }

    pub fn workspace_snapshot_count(&self) -> Result<usize, StorageError> {
        Ok(self
            .connection
            .query_row("SELECT count(*) FROM workspace_snapshots", [], |row| {
                row.get::<_, i64>(0)
            })? as usize)
    }

    pub fn apply_account_switch(
        &self,
        account_id: &str,
        choice: AccountSwitchChoice,
        cloud_changes: &[SyncChange],
        cloud_cursor: u64,
    ) -> Result<(), StorageError> {
        if account_id.trim().is_empty() || cloud_changes.iter().any(|change| change.change_seq == 0)
        {
            return Err(StorageError::InvalidSyncData);
        }
        self.archive_workspace()?;
        let result =
            self.apply_account_switch_inner(account_id, choice, cloud_changes, cloud_cursor);
        self.connection
            .execute("PRAGMA defer_foreign_keys=OFF", [])?;
        result
    }

    fn apply_account_switch_inner(
        &self,
        account_id: &str,
        choice: AccountSwitchChoice,
        cloud_changes: &[SyncChange],
        cloud_cursor: u64,
    ) -> Result<(), StorageError> {
        let tx = self.connection.unchecked_transaction()?;
        tx.execute("PRAGMA defer_foreign_keys=ON", [])?;
        tx.execute(
            "DELETE FROM sync_outbox WHERE account_id=?1",
            params![account_id],
        )?;
        tx.execute(
            "DELETE FROM sync_conflicts WHERE account_id=?1",
            params![account_id],
        )?;

        match choice {
            AccountSwitchChoice::CloudReplaceLocal => {
                tx.execute("DELETE FROM servers", [])?;
                tx.execute("DELETE FROM folders", [])?;
                tx.execute("DELETE FROM automation_definitions", [])?;
                set_workspace_owner(&tx, account_id)?;
                for change in restorable_cloud_changes(cloud_changes) {
                    apply_change(&tx, Some(account_id), &change)?;
                }
            }
            AccountSwitchChoice::LocalReplaceCloud => {
                let local = local_entities(&tx)?;
                let cloud = cloud_map(cloud_changes);
                set_workspace_owner(&tx, account_id)?;
                let mut sequence = 0_i64;
                for entity in &local {
                    let base = cloud
                        .get(&entity_key(entity.entity_type, entity.entity_id))
                        .map(|change| change.revision)
                        .unwrap_or(0);
                    stage_mutation(&tx, account_id, entity, base, false, &mut sequence, true)?;
                }
                for change in ordered_cloud_deletions(cloud_changes) {
                    let key = entity_key(change.entity_type, change.entity_id);
                    if local
                        .iter()
                        .any(|entity| entity_key(entity.entity_type, entity.entity_id) == key)
                    {
                        continue;
                    }
                    let entity = LocalEntity {
                        entity_type: change.entity_type,
                        entity_id: change.entity_id,
                        data: change.data.clone(),
                    };
                    stage_mutation(
                        &tx,
                        account_id,
                        &entity,
                        change.revision,
                        true,
                        &mut sequence,
                        false,
                    )?;
                }
            }
            AccountSwitchChoice::Merge => {
                let local = local_entities(&tx)?;
                let cloud = cloud_map(cloud_changes);
                set_workspace_owner(&tx, account_id)?;
                let mut sequence = 0_i64;
                for entity in &local {
                    let key = entity_key(entity.entity_type, entity.entity_id);
                    if let Some(remote) = cloud.get(&key) {
                        if equivalent_content(&entity.data, &remote.data) {
                            update_local_revision(&tx, entity, remote.revision, false)?;
                            continue;
                        }
                        stage_mutation(
                            &tx,
                            account_id,
                            entity,
                            remote.revision,
                            false,
                            &mut sequence,
                            true,
                        )?;
                        tx.execute(
                            "INSERT INTO sync_conflicts(conflict_id,entity_type,entity_id,local_payload,remote_payload,remote_revision,detected_at,account_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                            params![
                                Uuid::new_v4().to_string(),
                                entity_type_name(entity.entity_type),
                                entity.entity_id.to_string(),
                                entity.data.to_string(),
                                serde_json::to_string(remote).map_err(|_| StorageError::Serialization)?,
                                remote.revision,
                                Utc::now().to_rfc3339(),
                                account_id,
                            ],
                        )?;
                    } else {
                        stage_mutation(&tx, account_id, entity, 0, false, &mut sequence, true)?;
                    }
                }
                for change in ordered_cloud_changes(cloud_changes) {
                    let key = entity_key(change.entity_type, change.entity_id);
                    if local
                        .iter()
                        .any(|entity| entity_key(entity.entity_type, entity.entity_id) == key)
                    {
                        continue;
                    }
                    remove_local_entity(&tx, change.entity_type, change.entity_id)?;
                    apply_change(&tx, Some(account_id), change)?;
                }
            }
        }
        set_sync_cursor(&tx, Some(account_id), cloud_cursor)?;
        tx.commit()?;
        Ok(())
    }
}

fn local_entities(connection: &Connection) -> Result<Vec<LocalEntity>, StorageError> {
    let mut entities = Vec::new();
    let mut statement = connection.prepare(
        "SELECT id,parent_id,name,sort_order,revision,updated_at FROM folders WHERE deleted_at IS NULL ORDER BY sort_order,name,id",
    )?;
    let folders = statement
        .query_map([], crate::map_folder)?
        .collect::<Result<Vec<_>, _>>()?;
    for folder in folders {
        entities.push(LocalEntity {
            entity_type: SyncEntityType::Folder,
            entity_id: folder.id.0,
            data: serde_json::to_value(FolderSyncDto::from_parts(&folder, None))
                .map_err(|_| StorageError::Serialization)?,
        });
    }

    let mut statement = connection
        .prepare("SELECT * FROM servers WHERE deleted_at IS NULL ORDER BY sort_order,name,id")?;
    let servers = statement
        .query_map([], crate::map_server)?
        .collect::<Result<Vec<_>, _>>()?;
    for server in servers {
        entities.push(LocalEntity {
            entity_type: SyncEntityType::Server,
            entity_id: server.id.0,
            data: serde_json::to_value(ServerSyncDto::from(&server))
                .map_err(|_| StorageError::Serialization)?,
        });
    }

    for entity_type in [
        SyncEntityType::BatchTemplate,
        SyncEntityType::WorkflowDefinition,
    ] {
        let mut statement = connection.prepare(
            "SELECT id,name,definition,revision,updated_at,deleted_at FROM automation_definitions WHERE kind=?1 AND deleted_at IS NULL ORDER BY name,id",
        )?;
        let rows = statement.query_map(params![entity_type_name(entity_type)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        for row in rows {
            let (id, name, definition, revision, updated_at, deleted_at) = row?;
            let dto = AutomationDefinitionSyncDto {
                id: Uuid::parse_str(&id).map_err(|_| StorageError::InvalidSyncData)?,
                name,
                definition: serde_json::from_str(&definition)
                    .map_err(|_| StorageError::InvalidSyncData)?,
                revision,
                updated_at: parse_time(&updated_at)?,
                deleted_at: deleted_at.map(|value| parse_time(&value)).transpose()?,
            };
            entities.push(LocalEntity {
                entity_type,
                entity_id: dto.id,
                data: serde_json::to_value(dto).map_err(|_| StorageError::Serialization)?,
            });
        }
    }
    Ok(entities)
}

fn cloud_map(changes: &[SyncChange]) -> HashMap<(SyncEntityType, Uuid), &SyncChange> {
    changes
        .iter()
        .filter(|change| !change.deleted)
        .map(|change| (entity_key(change.entity_type, change.entity_id), change))
        .collect()
}

fn ordered_cloud_changes(changes: &[SyncChange]) -> Vec<&SyncChange> {
    let mut changes = changes.iter().collect::<Vec<_>>();
    changes.sort_by_key(|change| {
        let rank = match change.entity_type {
            SyncEntityType::Folder => 0,
            SyncEntityType::Server => 1,
            SyncEntityType::BatchTemplate | SyncEntityType::WorkflowDefinition => 2,
        };
        (rank, change.change_seq)
    });
    changes
}

fn restorable_cloud_changes(changes: &[SyncChange]) -> Vec<SyncChange> {
    let folder_ids = changes
        .iter()
        .filter(|change| change.entity_type == SyncEntityType::Folder)
        .map(|change| change.entity_id)
        .collect::<std::collections::HashSet<_>>();
    let server_ids = changes
        .iter()
        .filter(|change| change.entity_type == SyncEntityType::Server)
        .map(|change| change.entity_id)
        .collect::<std::collections::HashSet<_>>();
    let mut restored = changes.to_vec();
    for change in &mut restored {
        // Older uploads could omit a parent folder or jump server while
        // retaining its ID. Match ON DELETE SET NULL instead of rejecting the
        // complete cloud snapshot and leaving the account switch stuck.
        let references: &[(&str, &std::collections::HashSet<Uuid>)] = match change.entity_type {
            SyncEntityType::Folder => &[("parent_id", &folder_ids)],
            SyncEntityType::Server => &[("folderId", &folder_ids), ("jumpServerId", &server_ids)],
            SyncEntityType::BatchTemplate | SyncEntityType::WorkflowDefinition => &[],
        };
        for (field, available_ids) in references {
            let missing = change.data[field]
                .as_str()
                .and_then(|value| Uuid::parse_str(value).ok())
                .is_some_and(|id| !available_ids.contains(&id));
            if missing {
                change.data[field] = serde_json::Value::Null;
            }
        }
    }
    // Keep tombstones because active rows may still reference them. Tombstones
    // stay hidden from the workspace but preserve database referential integrity.
    restored.sort_by_key(|change| {
        let rank = match change.entity_type {
            SyncEntityType::Folder => 0,
            SyncEntityType::Server => 1,
            SyncEntityType::BatchTemplate | SyncEntityType::WorkflowDefinition => 2,
        };
        (rank, change.change_seq)
    });
    restored
}

fn ordered_cloud_deletions(changes: &[SyncChange]) -> Vec<&SyncChange> {
    let mut changes = changes
        .iter()
        .filter(|change| !change.deleted)
        .collect::<Vec<_>>();
    changes.sort_by_key(|change| {
        let rank = match change.entity_type {
            SyncEntityType::Server => 0,
            SyncEntityType::Folder => 1,
            SyncEntityType::BatchTemplate | SyncEntityType::WorkflowDefinition => 2,
        };
        (rank, change.change_seq)
    });
    changes
}

fn entity_key(entity_type: SyncEntityType, entity_id: Uuid) -> (SyncEntityType, Uuid) {
    (entity_type, entity_id)
}

fn stage_mutation(
    connection: &Connection,
    account_id: &str,
    entity: &LocalEntity,
    base_revision: u64,
    deleted: bool,
    sequence: &mut i64,
    update_local: bool,
) -> Result<(), StorageError> {
    let now = Utc::now();
    let revision = base_revision.saturating_add(1);
    let mut payload = entity.data.clone();
    payload["revision"] = serde_json::Value::from(revision);
    match entity.entity_type {
        SyncEntityType::Server => {
            payload["updatedAt"] = serde_json::Value::from(now.to_rfc3339());
            payload["deletedAt"] = if deleted {
                serde_json::Value::from(now.to_rfc3339())
            } else {
                serde_json::Value::Null
            };
        }
        _ => {
            payload["updated_at"] = serde_json::Value::from(now.to_rfc3339());
            payload["deleted_at"] = if deleted {
                serde_json::Value::from(now.to_rfc3339())
            } else {
                serde_json::Value::Null
            };
        }
    }
    *sequence += 1;
    let created_at = now + chrono::Duration::microseconds(*sequence);
    connection.execute(
        "INSERT INTO sync_outbox(mutation_id,entity_type,entity_id,operation,payload,base_revision,schema_version,created_at,account_id) VALUES(?1,?2,?3,?4,?5,?6,1,?7,?8)",
        params![
            Uuid::new_v4().to_string(),
            entity_type_name(entity.entity_type),
            entity.entity_id.to_string(),
            if deleted { "delete" } else { "upsert" },
            payload.to_string(),
            base_revision,
            created_at.to_rfc3339(),
            account_id,
        ],
    )?;
    if update_local {
        update_local_revision(connection, entity, revision, deleted)?;
    }
    Ok(())
}

fn update_local_revision(
    connection: &Connection,
    entity: &LocalEntity,
    revision: u64,
    deleted: bool,
) -> Result<(), StorageError> {
    let now = Utc::now().to_rfc3339();
    let deleted_at = deleted.then_some(now.as_str());
    match entity.entity_type {
        SyncEntityType::Folder => {
            connection.execute(
                "UPDATE folders SET revision=?1,updated_at=?2,deleted_at=?3 WHERE id=?4",
                params![revision, now, deleted_at, entity.entity_id.to_string()],
            )?;
        }
        SyncEntityType::Server => {
            connection.execute(
                "UPDATE servers SET revision=?1,updated_at=?2,deleted_at=?3 WHERE id=?4",
                params![revision, now, deleted_at, entity.entity_id.to_string()],
            )?;
        }
        SyncEntityType::BatchTemplate | SyncEntityType::WorkflowDefinition => {
            connection.execute(
                "UPDATE automation_definitions SET revision=?1,updated_at=?2,deleted_at=?3 WHERE id=?4 AND kind=?5",
                params![revision, now, deleted_at, entity.entity_id.to_string(), entity_type_name(entity.entity_type)],
            )?;
        }
    }
    Ok(())
}

fn remove_local_entity(
    connection: &Connection,
    entity_type: SyncEntityType,
    entity_id: Uuid,
) -> Result<(), StorageError> {
    match entity_type {
        SyncEntityType::Folder => {
            connection.execute(
                "DELETE FROM folders WHERE id=?1",
                params![entity_id.to_string()],
            )?;
        }
        SyncEntityType::Server => {
            connection.execute(
                "DELETE FROM servers WHERE id=?1",
                params![entity_id.to_string()],
            )?;
        }
        SyncEntityType::BatchTemplate | SyncEntityType::WorkflowDefinition => {
            connection.execute(
                "DELETE FROM automation_definitions WHERE id=?1 AND kind=?2",
                params![entity_id.to_string(), entity_type_name(entity_type)],
            )?;
        }
    }
    Ok(())
}

fn equivalent_content(local: &serde_json::Value, remote: &serde_json::Value) -> bool {
    fn normalized(value: &serde_json::Value) -> serde_json::Value {
        let mut value = value.clone();
        if let Some(fields) = value.as_object_mut() {
            for key in [
                "revision",
                "updated_at",
                "updatedAt",
                "deleted_at",
                "deletedAt",
            ] {
                fields.remove(key);
            }
        }
        value
    }
    normalized(local) == normalized(remote)
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, StorageError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| StorageError::InvalidSyncData)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sparrowshell_domain::{
        AuthType, Folder, FolderId, FolderSyncDto, LocalServerRecord, ServerId, ServerInput,
        ServerSyncDto,
    };

    fn repository(account_id: &str) -> Repository {
        let repository = Repository::in_memory().unwrap();
        repository.set_workspace_owner(account_id).unwrap();
        repository
    }

    fn add_server(
        repository: &Repository,
        folder_id: Option<FolderId>,
        name: &str,
    ) -> LocalServerRecord {
        repository
            .upsert_server(ServerInput {
                id: None,
                folder_id,
                name: name.to_owned(),
                host: "127.0.0.1".to_owned(),
                port: 22,
                username: "tester".to_owned(),
                auth_type: AuthType::Password,
                private_key_path_hint: None,
                jump_server_id: None,
            })
            .unwrap()
    }

    fn clear_outbox(repository: &Repository) {
        for mutation in repository.outbox_batch(100).unwrap() {
            repository
                .acknowledge_mutation(mutation.mutation_id)
                .unwrap();
        }
    }

    fn remote_server(
        source: &LocalServerRecord,
        change_seq: u64,
        revision: u64,
        name: &str,
    ) -> SyncChange {
        let mut source = source.clone();
        source.revision = revision;
        source.updated_at = Utc::now();
        source.name = name.to_owned();
        source.deleted_at = None;
        SyncChange {
            change_seq,
            entity_type: SyncEntityType::Server,
            entity_id: source.id.0,
            revision,
            deleted: false,
            data: serde_json::to_value(ServerSyncDto::from(&source)).unwrap(),
        }
    }

    fn remote_folder(source: &Folder, change_seq: u64, revision: u64) -> SyncChange {
        let mut source = source.clone();
        source.revision = revision;
        source.updated_at = Utc::now();
        SyncChange {
            change_seq,
            entity_type: SyncEntityType::Folder,
            entity_id: source.id.0,
            revision,
            deleted: false,
            data: serde_json::to_value(FolderSyncDto::from_parts(&source, None)).unwrap(),
        }
    }

    #[test]
    fn local_replace_uses_the_entire_workspace_and_archives_without_credentials() {
        let repository = repository("account-a");
        let folder = repository.create_folder(None, "all-local").unwrap();
        let server = add_server(&repository, Some(folder.id), "local-server");
        repository
            .connection
            .execute(
                "UPDATE servers SET credential_ref='local-secret-reference' WHERE id=?1",
                params![server.id.to_string()],
            )
            .unwrap();
        clear_outbox(&repository);

        repository
            .apply_account_switch("account-b", AccountSwitchChoice::LocalReplaceCloud, &[], 0)
            .unwrap();

        assert_eq!(
            repository.workspace_owner().unwrap().as_deref(),
            Some("account-b")
        );
        assert_eq!(
            repository
                .outbox_batch_for(Some("account-a"), 100)
                .unwrap()
                .len(),
            0
        );
        let mutations = repository.outbox_batch_for(Some("account-b"), 100).unwrap();
        assert_eq!(
            mutations.len(),
            2,
            "synced local entities must also be uploaded"
        );
        assert!(mutations
            .iter()
            .any(|item| item.entity_type == SyncEntityType::Folder));
        assert!(mutations
            .iter()
            .any(|item| item.entity_type == SyncEntityType::Server));
        assert_eq!(repository.workspace_snapshot_count().unwrap(), 1);
        let archived: String = repository
            .connection
            .query_row("SELECT payload FROM workspace_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(archived.contains("local-server"));
        for forbidden in [
            "local-secret-reference",
            "credential_ref",
            "credentialRef",
            "passphrase",
            "\"privateKey\":",
            "accessToken",
        ] {
            assert!(!archived.contains(forbidden), "snapshot leaked {forbidden}");
        }
    }

    #[test]
    fn local_replace_deletes_cloud_only_servers_before_their_folders() {
        let repository = repository("account-a");
        let local = add_server(&repository, None, "local");
        let cloud_folder = Folder {
            id: FolderId::new(),
            parent_id: None,
            name: "cloud-only".to_owned(),
            sort_order: 0,
            revision: 3,
            updated_at: Utc::now(),
        };
        let mut cloud_server = local.clone();
        cloud_server.id = ServerId::new();
        cloud_server.folder_id = Some(cloud_folder.id);
        let changes = vec![
            remote_folder(&cloud_folder, 1, 3),
            remote_server(&cloud_server, 2, 4, "cloud-server"),
        ];

        repository
            .apply_account_switch(
                "account-b",
                AccountSwitchChoice::LocalReplaceCloud,
                &changes,
                2,
            )
            .unwrap();

        let mutations = repository.outbox_batch_for(Some("account-b"), 100).unwrap();
        let server_delete = mutations
            .iter()
            .position(|item| item.entity_id == cloud_server.id.0 && item.deleted)
            .unwrap();
        let folder_delete = mutations
            .iter()
            .position(|item| item.entity_id == cloud_folder.id.0 && item.deleted)
            .unwrap();
        assert!(server_delete < folder_delete);
        assert_eq!(repository.sync_cursor_for(Some("account-b")).unwrap(), 2);
        assert_eq!(repository.sync_cursor_for(Some("account-a")).unwrap(), 0);
    }

    #[test]
    fn cloud_replace_preserves_the_previous_account_outbox_and_snapshot() {
        let repository = repository("account-a");
        let local = add_server(&repository, None, "local");
        let mut cloud = local.clone();
        cloud.id = ServerId::new();
        let changes = vec![remote_server(&cloud, 9, 3, "cloud")];

        repository
            .apply_account_switch(
                "account-b",
                AccountSwitchChoice::CloudReplaceLocal,
                &changes,
                9,
            )
            .unwrap();

        assert_eq!(
            repository
                .outbox_batch_for(Some("account-a"), 100)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            repository
                .outbox_batch_for(Some("account-b"), 100)
                .unwrap()
                .len(),
            0
        );
        assert_eq!(repository.workspace_snapshot_count().unwrap(), 1);
        let snapshot = repository.snapshot().unwrap();
        assert_eq!(snapshot.servers.len(), 1);
        assert_eq!(snapshot.servers[0].id, cloud.id);
        assert_eq!(snapshot.servers[0].name, "cloud");
        assert_eq!(repository.sync_cursor_for(Some("account-b")).unwrap(), 9);
        assert_eq!(repository.sync_cursor_for(Some("account-a")).unwrap(), 0);
    }

    #[test]
    fn cloud_replace_accepts_a_child_folder_before_its_parent() {
        let repository = repository("account-a");
        add_server(&repository, None, "local");
        let parent = Folder {
            id: FolderId::new(),
            parent_id: None,
            name: "parent".to_owned(),
            sort_order: 0,
            revision: 1,
            updated_at: Utc::now(),
        };
        let child = Folder {
            id: FolderId::new(),
            parent_id: Some(parent.id),
            name: "child".to_owned(),
            sort_order: 0,
            revision: 1,
            updated_at: Utc::now(),
        };

        repository
            .apply_account_switch(
                "account-b",
                AccountSwitchChoice::CloudReplaceLocal,
                &[remote_folder(&child, 1, 1), remote_folder(&parent, 2, 1)],
                2,
            )
            .unwrap();

        let snapshot = repository.snapshot().unwrap();
        assert_eq!(snapshot.folders.len(), 2);
        assert_eq!(
            snapshot
                .folders
                .iter()
                .find(|folder| folder.id == child.id)
                .unwrap()
                .parent_id,
            Some(parent.id)
        );
    }

    #[test]
    fn cloud_replace_restores_tombstones_required_by_active_references() {
        let repository = repository("account-a");
        let local = add_server(&repository, None, "local");
        let cloud_folder = Folder {
            id: FolderId::new(),
            parent_id: None,
            name: "deleted-folder".to_owned(),
            sort_order: 0,
            revision: 3,
            updated_at: Utc::now(),
        };
        let mut deleted_jump = local.clone();
        deleted_jump.id = ServerId::new();
        let mut active_server = local.clone();
        active_server.id = ServerId::new();
        active_server.folder_id = Some(cloud_folder.id);
        active_server.jump_server_id = Some(deleted_jump.id);

        let mut folder_change = remote_folder(&cloud_folder, 1, 3);
        folder_change.deleted = true;
        folder_change.data["deleted_at"] = serde_json::Value::from(Utc::now().to_rfc3339());
        let mut jump_change = remote_server(&deleted_jump, 2, 4, "deleted-jump");
        jump_change.deleted = true;
        jump_change.data["deletedAt"] = serde_json::Value::from(Utc::now().to_rfc3339());
        let active_change = remote_server(&active_server, 3, 5, "active-server");

        repository
            .apply_account_switch(
                "account-b",
                AccountSwitchChoice::CloudReplaceLocal,
                &[folder_change, jump_change, active_change],
                3,
            )
            .unwrap();

        let snapshot = repository.snapshot().unwrap();
        assert!(snapshot.folders.is_empty());
        assert_eq!(snapshot.servers.len(), 1);
        assert_eq!(snapshot.servers[0].id, active_server.id);
        let violations: i64 = repository
            .connection
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(violations, 0);
        let tombstones: i64 = repository
            .connection
            .query_row(
                "SELECT (SELECT count(*) FROM folders WHERE deleted_at IS NOT NULL) + (SELECT count(*) FROM servers WHERE deleted_at IS NOT NULL)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tombstones, 2);
    }

    #[test]
    fn cloud_replace_detaches_references_absent_from_the_snapshot() {
        let repository = repository("account-a");
        let local = add_server(&repository, None, "local");
        let cloud_folder = Folder {
            id: FolderId::new(),
            parent_id: Some(FolderId::new()),
            name: "deleted-child".to_owned(),
            sort_order: 0,
            revision: 3,
            updated_at: Utc::now(),
        };
        let mut folder_change = remote_folder(&cloud_folder, 1, 3);
        folder_change.deleted = true;
        folder_change.data["deleted_at"] = serde_json::Value::from(Utc::now().to_rfc3339());
        let mut cloud_server = local.clone();
        cloud_server.id = ServerId::new();
        cloud_server.folder_id = Some(FolderId::new());
        cloud_server.jump_server_id = Some(ServerId::new());
        let server_change = remote_server(&cloud_server, 2, 3, "server-with-legacy-references");

        repository
            .apply_account_switch(
                "account-b",
                AccountSwitchChoice::CloudReplaceLocal,
                &[folder_change, server_change],
                2,
            )
            .unwrap();

        let snapshot = repository.snapshot().unwrap();
        assert!(snapshot.folders.is_empty());
        assert_eq!(snapshot.servers.len(), 1);
        assert_eq!(snapshot.servers[0].folder_id, None);
        assert_eq!(snapshot.servers[0].jump_server_id, None);
        let detached_parent: Option<String> = repository
            .connection
            .query_row(
                "SELECT parent_id FROM folders WHERE id=?1",
                params![cloud_folder.id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(detached_parent, None);
        let violations: i64 = repository
            .connection
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(violations, 0);
    }

    #[test]
    fn merge_combines_unique_entities_deduplicates_and_keeps_remote_on_conflict() {
        let repository = repository("account-a");
        let shared = add_server(&repository, None, "shared");
        let conflict = add_server(&repository, None, "local-conflict");
        let local_only = add_server(&repository, None, "local-only");
        clear_outbox(&repository);
        let mut cloud_only = local_only.clone();
        cloud_only.id = ServerId::new();
        let changes = vec![
            remote_server(&shared, 1, 5, "shared"),
            remote_server(&conflict, 2, 7, "cloud-conflict"),
            remote_server(&cloud_only, 3, 2, "cloud-only"),
        ];

        repository
            .apply_account_switch("account-b", AccountSwitchChoice::Merge, &changes, 3)
            .unwrap();

        let snapshot = repository.snapshot().unwrap();
        assert_eq!(snapshot.servers.len(), 4);
        assert_eq!(
            snapshot
                .servers
                .iter()
                .find(|item| item.id == shared.id)
                .unwrap()
                .revision,
            5
        );
        assert_eq!(
            snapshot
                .servers
                .iter()
                .find(|item| item.id == conflict.id)
                .unwrap()
                .name,
            "local-conflict"
        );
        let pending = repository.outbox_batch_for(Some("account-b"), 100).unwrap();
        assert_eq!(
            pending.len(),
            1,
            "identical and conflicting entities must not be pushed"
        );
        assert_eq!(pending[0].entity_id, local_only.id.0);
        let conflicts = repository.unresolved_conflicts().unwrap();
        assert_eq!(conflicts.len(), 1);

        repository
            .resolve_conflict_keep_remote(conflicts[0].conflict_id)
            .unwrap();
        let snapshot = repository.snapshot().unwrap();
        let resolved = snapshot
            .servers
            .iter()
            .find(|item| item.id == conflict.id)
            .unwrap();
        assert_eq!(resolved.name, "cloud-conflict");
        assert_eq!(resolved.revision, 7);
        assert!(repository.unresolved_conflicts().unwrap().is_empty());
        assert_eq!(
            repository
                .outbox_batch_for(Some("account-b"), 100)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn merge_conflict_keep_local_rebases_the_outbox_on_the_cloud_revision() {
        let repository = repository("account-a");
        let conflict = add_server(&repository, None, "local-conflict");
        clear_outbox(&repository);
        let changes = vec![remote_server(&conflict, 1, 7, "cloud-conflict")];
        repository
            .apply_account_switch("account-b", AccountSwitchChoice::Merge, &changes, 1)
            .unwrap();
        let conflict_id = repository.unresolved_conflicts().unwrap()[0].conflict_id;

        repository.resolve_conflict_keep_local(conflict_id).unwrap();

        let pending = repository.outbox_batch_for(Some("account-b"), 100).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].base_revision, 7);
        assert_eq!(pending[0].data["revision"], 8);
        assert_eq!(pending[0].data["name"], "local-conflict");
        assert!(repository.unresolved_conflicts().unwrap().is_empty());
    }
}
