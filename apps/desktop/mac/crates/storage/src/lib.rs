mod automation;
mod billing;
mod switch;
mod sync;

pub use automation::AutomationRunRecord;
pub use switch::{AccountSwitchChoice, WorkspaceSnapshot};
pub use sync::{ApplyRemoteResult, SyncConflictRecord};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use sparrowshell_domain::{
    CatalogOrderItem, CatalogSnapshot, CommandHistoryEntry, Folder, FolderId, HistoryId,
    LocalServerRecord, ServerId, ServerInput,
};
use std::path::Path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS folders (
    id TEXT PRIMARY KEY,
    parent_id TEXT REFERENCES folders(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 128),
    sort_order INTEGER NOT NULL DEFAULT 0,
    revision INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);
CREATE INDEX IF NOT EXISTS folders_parent_order ON folders(parent_id, sort_order, name);
CREATE TABLE IF NOT EXISTS servers (
    id TEXT PRIMARY KEY,
    folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL CHECK(port BETWEEN 1 AND 65535),
    username TEXT NOT NULL,
    auth_type TEXT NOT NULL CHECK(auth_type IN ('password', 'private_key')),
    private_key_path_hint TEXT,
    jump_server_id TEXT REFERENCES servers(id) ON DELETE SET NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    credential_ref TEXT,
    revision INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);
CREATE INDEX IF NOT EXISTS servers_folder ON servers(folder_id) WHERE deleted_at IS NULL;
CREATE TABLE IF NOT EXISTS host_keys (
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    algorithm TEXT NOT NULL,
    public_key TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL,
    PRIMARY KEY(host, port)
);
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sync_outbox (
    mutation_id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    payload TEXT NOT NULL,
    base_revision INTEGER NOT NULL DEFAULT 0,
    schema_version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS sync_outbox_created ON sync_outbox(created_at);
CREATE INDEX IF NOT EXISTS sync_outbox_entity ON sync_outbox(entity_type, entity_id);
CREATE TABLE IF NOT EXISTS sync_state (
    id INTEGER PRIMARY KEY CHECK(id=1),
    device_id TEXT NOT NULL,
    cursor INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sync_conflicts (
    conflict_id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    local_payload TEXT NOT NULL,
    remote_payload TEXT NOT NULL,
    remote_revision INTEGER NOT NULL,
    detected_at TEXT NOT NULL,
    resolved_at TEXT
);
CREATE INDEX IF NOT EXISTS sync_conflicts_entity ON sync_conflicts(entity_type, entity_id, resolved_at);
CREATE TABLE IF NOT EXISTS command_history (
    id TEXT PRIMARY KEY,
    server_id TEXT NOT NULL,
    command TEXT NOT NULL CHECK(length(command) BETWEEN 1 AND 16384),
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS command_history_server_created ON command_history(server_id, created_at DESC);
CREATE INDEX IF NOT EXISTS command_history_created ON command_history(created_at DESC);
CREATE TABLE IF NOT EXISTS automation_definitions (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('batch_template', 'workflow_definition')),
    name TEXT NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 128),
    definition TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);
CREATE INDEX IF NOT EXISTS automation_definitions_kind ON automation_definitions(kind, deleted_at, name);
CREATE TABLE IF NOT EXISTS automation_runs (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('batch', 'workflow')),
    state TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS automation_runs_created ON automation_runs(created_at DESC);
"#;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("local database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("invalid server: {0}")]
    InvalidServer(#[from] sparrowshell_domain::DomainError),
    #[error("folder does not exist")]
    FolderNotFound,
    #[error("server does not exist")]
    ServerNotFound,
    #[error("folder name is empty or too long")]
    InvalidFolderName,
    #[error("folder hierarchy would contain a cycle or exceed 32 levels")]
    InvalidFolderHierarchy,
    #[error("local serialization error")]
    Serialization,
    #[error("command is empty, too long or contains control characters")]
    InvalidCommand,
    #[error("invalid or inconsistent synchronization payload")]
    InvalidSyncData,
    #[error("synchronization conflict does not exist or is already resolved")]
    ConflictNotFound,
    #[error("automation definition is invalid or contains runtime or secret fields")]
    InvalidAutomationDefinition,
    #[error("automation run history is invalid")]
    InvalidAutomationRun,
}

#[derive(Debug)]
pub struct Repository {
    pub(crate) connection: Connection,
    database_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyRecord {
    pub algorithm: String,
    pub public_key: String,
    pub fingerprint: String,
}

impl Repository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref();
        let mut repository = Self::initialize(Connection::open(path)?)?;
        repository.database_path = Some(path.to_path_buf());
        Ok(repository)
    }

    pub fn in_memory() -> Result<Self, StorageError> {
        Self::initialize(Connection::open_in_memory()?)
    }

    pub fn data_directory(&self) -> Option<&Path> {
        self.database_path.as_deref().and_then(Path::parent)
    }

    fn initialize(connection: Connection) -> Result<Self, StorageError> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;",
        )?;
        connection.execute_batch(SCHEMA)?;
        sync::migrate(&connection)?;
        Ok(Self {
            connection,
            database_path: None,
        })
    }

    pub fn snapshot(&self) -> Result<CatalogSnapshot, StorageError> {
        let mut statement = self.connection.prepare("SELECT id, parent_id, name, sort_order, revision, updated_at FROM folders WHERE deleted_at IS NULL ORDER BY sort_order, name")?;
        let folders = statement
            .query_map([], map_folder)?
            .collect::<Result<Vec<_>, _>>()?;
        let mut statement = self
            .connection
            .prepare("SELECT * FROM servers WHERE deleted_at IS NULL ORDER BY name, id")?;
        let servers = statement
            .query_map([], map_server)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CatalogSnapshot { folders, servers })
    }

    pub fn create_folder(
        &self,
        parent_id: Option<FolderId>,
        name: &str,
    ) -> Result<Folder, StorageError> {
        self.save_folder(FolderId::new(), parent_id, name)
    }

    pub fn save_folder(
        &self,
        id: FolderId,
        parent_id: Option<FolderId>,
        name: &str,
    ) -> Result<Folder, StorageError> {
        let name = name.trim();
        if name.is_empty() || name.len() > 128 {
            return Err(StorageError::InvalidFolderName);
        }
        if parent_id == Some(id) {
            return Err(StorageError::InvalidFolderHierarchy);
        }
        let tx = self.connection.unchecked_transaction()?;
        if let Some(parent) = parent_id {
            let mut statement = tx.prepare("WITH RECURSIVE ancestors(id, parent_id, depth) AS (SELECT id, parent_id, 1 FROM folders WHERE id=?1 UNION ALL SELECT f.id, f.parent_id, a.depth+1 FROM folders f JOIN ancestors a ON f.id=a.parent_id WHERE a.depth<33) SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=?2), max(depth) FROM ancestors")?;
            let (cycle, depth): (bool, Option<u32>) = statement
                .query_row(params![parent.to_string(), id.to_string()], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?
                .unwrap_or((false, None));
            if depth.is_none() {
                return Err(StorageError::FolderNotFound);
            }
            if cycle || depth.unwrap_or(0) >= 32 {
                return Err(StorageError::InvalidFolderHierarchy);
            }
        }
        let now = Utc::now();
        tx.execute("INSERT INTO folders(id,parent_id,name,revision,updated_at) VALUES(?1,?2,?3,1,?4) ON CONFLICT(id) DO UPDATE SET parent_id=excluded.parent_id,name=excluded.name,revision=folders.revision+1,updated_at=excluded.updated_at,deleted_at=NULL", params![id.to_string(), parent_id.map(|v|v.to_string()), name, now.to_rfc3339()])?;
        let folder = tx.query_row(
            "SELECT id,parent_id,name,sort_order,revision,updated_at FROM folders WHERE id=?1",
            params![id.to_string()],
            map_folder,
        )?;
        enqueue(
            &tx,
            "folder",
            &id.to_string(),
            "upsert",
            serde_json::to_value(sparrowshell_domain::FolderSyncDto::from_parts(
                &folder, None,
            ))
            .map_err(|_| StorageError::Serialization)?,
        )?;
        tx.commit()?;
        self.folder(id)?.ok_or(StorageError::FolderNotFound)
    }

    pub fn folder(&self, id: FolderId) -> Result<Option<Folder>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT id,parent_id,name,sort_order,revision,updated_at FROM folders WHERE id=?1",
                params![id.to_string()],
                map_folder,
            )
            .optional()?)
    }

    pub fn upsert_server(&self, input: ServerInput) -> Result<LocalServerRecord, StorageError> {
        let input = input.validated()?;
        let tx = self.connection.unchecked_transaction()?;
        if let Some(folder_id) = input.folder_id {
            if !exists(&tx, "folders", folder_id.to_string())? {
                return Err(StorageError::FolderNotFound);
            }
        }
        if let Some(jump_id) = input.jump_server_id {
            if !exists(&tx, "servers", jump_id.to_string())? {
                return Err(StorageError::ServerNotFound);
            }
        }
        let id = input.id.unwrap_or_default();
        let now = Utc::now();
        tx.execute(r#"INSERT INTO servers(id,folder_id,name,host,port,username,auth_type,private_key_path_hint,jump_server_id,revision,updated_at)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,1,?10)
            ON CONFLICT(id) DO UPDATE SET folder_id=excluded.folder_id,name=excluded.name,host=excluded.host,port=excluded.port,
            username=excluded.username,auth_type=excluded.auth_type,private_key_path_hint=excluded.private_key_path_hint,
            jump_server_id=excluded.jump_server_id,revision=servers.revision+1,updated_at=excluded.updated_at"#,
            params![id.to_string(), input.folder_id.map(|v|v.to_string()), input.name, input.host, input.port, input.username,
                auth_name(input.auth_type), input.private_key_path_hint, input.jump_server_id.map(|v|v.to_string()), now.to_rfc3339()])?;
        let server = tx.query_row(
            "SELECT * FROM servers WHERE id=?1",
            params![id.to_string()],
            map_server,
        )?;
        enqueue(
            &tx,
            "server",
            &id.to_string(),
            "upsert",
            serde_json::to_value(sparrowshell_domain::ServerSyncDto::from(&server))
                .map_err(|_| StorageError::Serialization)?,
        )?;
        tx.commit()?;
        Ok(server)
    }

    pub fn reorder_servers(&self, items: Vec<CatalogOrderItem>) -> Result<(), StorageError> {
        let tx = self.connection.unchecked_transaction()?;
        let mut ids = std::collections::HashSet::new();
        for item in items {
            if item.sort_order < 0 || !ids.insert(item.id) {
                return Err(StorageError::InvalidSyncData);
            }
            let id = ServerId(item.id);
            let parent_id = item.parent_id.map(FolderId);
            if let Some(parent) = parent_id {
                if !exists(&tx, "folders", parent.to_string())? {
                    return Err(StorageError::FolderNotFound);
                }
            }
            let current = tx
                .query_row(
                    "SELECT folder_id FROM servers WHERE id=?1 AND deleted_at IS NULL",
                    params![id.to_string()],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .ok_or(StorageError::ServerNotFound)?;
            let parent = parent_id.map(|value| value.to_string());
            if current == parent {
                continue;
            }
            let now = Utc::now();
            tx.execute(
                "UPDATE servers SET folder_id=?2,revision=revision+1,updated_at=?3 WHERE id=?1",
                params![id.to_string(), parent, now.to_rfc3339()],
            )?;
            let server = tx.query_row(
                "SELECT * FROM servers WHERE id=?1",
                params![id.to_string()],
                map_server,
            )?;
            enqueue(
                &tx,
                "server",
                &id.to_string(),
                "upsert",
                serde_json::to_value(sparrowshell_domain::ServerSyncDto::from(&server))
                    .map_err(|_| StorageError::Serialization)?,
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn reorder_folders(&self, items: Vec<CatalogOrderItem>) -> Result<(), StorageError> {
        let tx = self.connection.unchecked_transaction()?;
        let mut ids = std::collections::HashSet::new();
        for item in items {
            if item.sort_order < 0 || !ids.insert(item.id) {
                return Err(StorageError::InvalidSyncData);
            }
            let id = FolderId(item.id);
            let parent_id = item.parent_id.map(FolderId);
            validate_folder_parent(&tx, id, parent_id)?;
            let current = tx
                .query_row(
                    "SELECT parent_id,sort_order FROM folders WHERE id=?1 AND deleted_at IS NULL",
                    params![id.to_string()],
                    |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?
                .ok_or(StorageError::FolderNotFound)?;
            let parent = parent_id.map(|value| value.to_string());
            if current.0 == parent && current.1 == item.sort_order {
                continue;
            }
            let now = Utc::now();
            tx.execute(
                "UPDATE folders SET parent_id=?2,sort_order=?3,revision=revision+1,updated_at=?4 WHERE id=?1",
                params![id.to_string(), parent, item.sort_order, now.to_rfc3339()],
            )?;
            let folder = tx.query_row(
                "SELECT id,parent_id,name,sort_order,revision,updated_at FROM folders WHERE id=?1",
                params![id.to_string()],
                map_folder,
            )?;
            enqueue(
                &tx,
                "folder",
                &id.to_string(),
                "upsert",
                serde_json::to_value(sparrowshell_domain::FolderSyncDto::from_parts(
                    &folder, None,
                ))
                .map_err(|_| StorageError::Serialization)?,
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// This local-only mutation intentionally does not create a sync outbox item.
    pub fn set_credential_ref(
        &self,
        id: ServerId,
        reference: Option<String>,
    ) -> Result<(), StorageError> {
        let changed = self.connection.execute(
            "UPDATE servers SET credential_ref=?1 WHERE id=?2 AND deleted_at IS NULL",
            params![reference, id.to_string()],
        )?;
        if changed == 0 {
            Err(StorageError::ServerNotFound)
        } else {
            Ok(())
        }
    }

    pub fn server(&self, id: ServerId) -> Result<Option<LocalServerRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT * FROM servers WHERE id=?1 AND deleted_at IS NULL",
                params![id.to_string()],
                map_server,
            )
            .optional()?)
    }

    pub fn delete_server(&self, id: ServerId) -> Result<Option<String>, StorageError> {
        let tx = self.connection.unchecked_transaction()?;
        let mut server = tx
            .query_row(
                "SELECT * FROM servers WHERE id=?1 AND deleted_at IS NULL",
                params![id.to_string()],
                map_server,
            )
            .optional()?
            .ok_or(StorageError::ServerNotFound)?;
        let reference = server.credential_ref.clone();
        let now = Utc::now();
        server.revision = server.revision.saturating_add(1);
        server.updated_at = now;
        server.deleted_at = Some(now);
        tx.execute("UPDATE servers SET deleted_at=?1, credential_ref=NULL, revision=?2,updated_at=?1 WHERE id=?3", params![now.to_rfc3339(), server.revision, id.to_string()])?;
        enqueue(
            &tx,
            "server",
            &id.to_string(),
            "delete",
            serde_json::to_value(sparrowshell_domain::ServerSyncDto::from(&server))
                .map_err(|_| StorageError::Serialization)?,
        )?;
        tx.commit()?;
        Ok(reference)
    }

    /// Duplicates non-secret connection settings under a new stable identity.
    /// The source Keychain reference is deliberately never copied.
    pub fn duplicate_server(&self, id: ServerId) -> Result<LocalServerRecord, StorageError> {
        let source = self.server(id)?.ok_or(StorageError::ServerNotFound)?;
        let mut base = String::new();
        for character in source.name.chars() {
            if base.len() + character.len_utf8() > 123 {
                break;
            }
            base.push(character);
        }
        self.upsert_server(ServerInput {
            id: None,
            folder_id: source.folder_id,
            name: format!("{base}-copy"),
            host: source.host,
            port: source.port,
            username: source.username,
            auth_type: source.auth_type,
            private_key_path_hint: source.private_key_path_hint,
            jump_server_id: source.jump_server_id,
        })
    }

    /// Soft-deletes a folder subtree and all contained servers atomically.
    /// Returned Keychain references are local-only and never enter sync payloads.
    pub fn delete_folder(&self, id: FolderId) -> Result<Vec<String>, StorageError> {
        let tx = self.connection.unchecked_transaction()?;
        let mut statement = tx.prepare(
            "WITH RECURSIVE subtree(id) AS (
                SELECT id FROM folders WHERE id=?1 AND deleted_at IS NULL
                UNION ALL
                SELECT f.id FROM folders f JOIN subtree s ON f.parent_id=s.id WHERE f.deleted_at IS NULL
            ) SELECT id,parent_id,name,sort_order,revision,updated_at FROM folders
              WHERE deleted_at IS NULL AND id IN (SELECT id FROM subtree)",
        )?;
        let folders = statement
            .query_map(params![id.to_string()], map_folder)?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        if folders.is_empty() {
            return Err(StorageError::FolderNotFound);
        }
        let mut statement = tx.prepare(
            "WITH RECURSIVE subtree(id) AS (
                SELECT id FROM folders WHERE id=?1 AND deleted_at IS NULL
                UNION ALL
                SELECT f.id FROM folders f JOIN subtree s ON f.parent_id=s.id WHERE f.deleted_at IS NULL
            ) SELECT * FROM servers
              WHERE deleted_at IS NULL AND folder_id IN (SELECT id FROM subtree)",
        )?;
        let servers = statement
            .query_map(params![id.to_string()], map_server)?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let now = Utc::now();
        let mut references = Vec::new();
        for mut server in servers {
            if let Some(reference) = server.credential_ref.take() {
                references.push(reference);
            }
            server.revision = server.revision.saturating_add(1);
            server.updated_at = now;
            server.deleted_at = Some(now);
            tx.execute(
                "UPDATE servers SET deleted_at=?1,credential_ref=NULL,revision=?2,updated_at=?1 WHERE id=?3",
                params![now.to_rfc3339(), server.revision, server.id.to_string()],
            )?;
            enqueue(
                &tx,
                "server",
                &server.id.to_string(),
                "delete",
                serde_json::to_value(sparrowshell_domain::ServerSyncDto::from(&server))
                    .map_err(|_| StorageError::Serialization)?,
            )?;
        }
        for mut folder in folders {
            folder.revision = folder.revision.saturating_add(1);
            folder.updated_at = now;
            tx.execute(
                "UPDATE folders SET deleted_at=?1,revision=?2,updated_at=?1 WHERE id=?3",
                params![now.to_rfc3339(), folder.revision, folder.id.to_string()],
            )?;
            enqueue(
                &tx,
                "folder",
                &folder.id.to_string(),
                "delete",
                serde_json::to_value(sparrowshell_domain::FolderSyncDto::from_parts(
                    &folder,
                    Some(now),
                ))
                .map_err(|_| StorageError::Serialization)?,
            )?;
        }
        tx.commit()?;
        Ok(references)
    }

    pub fn trusted_host_key(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Option<HostKeyRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT algorithm,public_key,fingerprint FROM host_keys WHERE host=?1 AND port=?2",
                params![host, port],
                |r| {
                    Ok(HostKeyRecord {
                        algorithm: r.get(0)?,
                        public_key: r.get(1)?,
                        fingerprint: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn trust_host_key(
        &self,
        host: &str,
        port: u16,
        key: &HostKeyRecord,
    ) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        self.connection.execute("INSERT INTO host_keys(host,port,algorithm,public_key,fingerprint,first_seen,last_seen) VALUES(?1,?2,?3,?4,?5,?6,?6) ON CONFLICT(host,port) DO UPDATE SET algorithm=excluded.algorithm,public_key=excluded.public_key,fingerprint=excluded.fingerprint,last_seen=excluded.last_seen", params![host,port,key.algorithm,key.public_key,key.fingerprint,now])?;
        Ok(())
    }

    /// Manual terminal history is local-only and never creates a sync outbox item.
    pub fn record_command(
        &self,
        server_id: ServerId,
        command: &str,
    ) -> Result<Option<CommandHistoryEntry>, StorageError> {
        let command = command.trim();
        if command.is_empty()
            || command.len() > 16_384
            || command.chars().any(|value| {
                value == '\0'
                    || value == '\r'
                    || value == '\n'
                    || (value.is_control() && value != '\t')
            })
        {
            return Err(StorageError::InvalidCommand);
        }
        if is_sensitive_command(command) {
            return Ok(None);
        }
        let entry = CommandHistoryEntry {
            id: HistoryId::new(),
            server_id,
            command: command.to_owned(),
            created_at: Utc::now(),
        };
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO command_history(id,server_id,command,created_at) VALUES(?1,?2,?3,?4)",
            params![
                entry.id.to_string(),
                entry.server_id.to_string(),
                entry.command,
                entry.created_at.to_rfc3339()
            ],
        )?;
        tx.execute(
            "DELETE FROM command_history WHERE id NOT IN (SELECT id FROM command_history ORDER BY created_at DESC, id DESC LIMIT 5000)",
            [],
        )?;
        tx.commit()?;
        Ok(Some(entry))
    }

    pub fn command_history(
        &self,
        server_id: Option<ServerId>,
        limit: u32,
    ) -> Result<Vec<CommandHistoryEntry>, StorageError> {
        let limit = limit.clamp(1, 200) as i64;
        if let Some(server_id) = server_id {
            let mut statement = self.connection.prepare(
                "SELECT id,server_id,command,created_at FROM command_history WHERE server_id=?1 ORDER BY created_at DESC,id DESC LIMIT ?2",
            )?;
            let entries = statement
                .query_map(params![server_id.to_string(), limit], map_history)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(entries)
        } else {
            let mut statement = self.connection.prepare(
                "SELECT id,server_id,command,created_at FROM command_history ORDER BY created_at DESC,id DESC LIMIT ?1",
            )?;
            let entries = statement
                .query_map(params![limit], map_history)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(entries)
        }
    }

    pub fn clear_command_history(
        &self,
        server_id: Option<ServerId>,
    ) -> Result<usize, StorageError> {
        let changed = if let Some(server_id) = server_id {
            self.connection.execute(
                "DELETE FROM command_history WHERE server_id=?1",
                params![server_id.to_string()],
            )?
        } else {
            self.connection.execute("DELETE FROM command_history", [])?
        };
        Ok(changed)
    }

    pub fn outbox_len(&self) -> Result<usize, StorageError> {
        let account_id = self.workspace_owner()?;
        Ok(self.connection.query_row(
            "SELECT count(*) FROM sync_outbox WHERE account_id IS ?1",
            params![account_id],
            |r| r.get::<_, i64>(0),
        )? as usize)
    }
}

fn validate_folder_parent(
    tx: &Connection,
    id: FolderId,
    parent_id: Option<FolderId>,
) -> Result<(), StorageError> {
    if parent_id == Some(id) {
        return Err(StorageError::InvalidFolderHierarchy);
    }
    if let Some(parent) = parent_id {
        let mut statement = tx.prepare("WITH RECURSIVE ancestors(id, parent_id, depth) AS (SELECT id, parent_id, 1 FROM folders WHERE id=?1 UNION ALL SELECT f.id, f.parent_id, a.depth+1 FROM folders f JOIN ancestors a ON f.id=a.parent_id WHERE a.depth<33) SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=?2), max(depth) FROM ancestors")?;
        let (cycle, depth): (bool, Option<u32>) = statement
            .query_row(params![parent.to_string(), id.to_string()], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?
            .unwrap_or((false, None));
        if depth.is_none() {
            return Err(StorageError::FolderNotFound);
        }
        if cycle || depth.unwrap_or(0) >= 32 {
            return Err(StorageError::InvalidFolderHierarchy);
        }
    }
    Ok(())
}

fn exists(tx: &Connection, table: &'static str, id: String) -> Result<bool, rusqlite::Error> {
    let sql = match table {
        "folders" => "SELECT EXISTS(SELECT 1 FROM folders WHERE id=?1)",
        _ => "SELECT EXISTS(SELECT 1 FROM servers WHERE id=?1 AND deleted_at IS NULL)",
    };
    tx.query_row(sql, params![id], |r| r.get(0))
}

pub(crate) fn current_account_id(tx: &Connection) -> Result<Option<String>, StorageError> {
    tx.query_row(
        "SELECT value FROM settings WHERE key='workspace_owner_id'",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(StorageError::from)
}

fn enqueue(
    tx: &Connection,
    entity_type: &str,
    entity_id: &str,
    operation: &str,
    payload: serde_json::Value,
) -> Result<(), StorageError> {
    let account_id = current_account_id(tx)?;
    tx.execute("INSERT INTO sync_outbox(mutation_id,entity_type,entity_id,operation,payload,created_at,account_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![uuid::Uuid::new_v4().to_string(), entity_type, entity_id, operation, payload.to_string(), Utc::now().to_rfc3339(), account_id])?;
    Ok(())
}

fn auth_name(value: sparrowshell_domain::AuthType) -> &'static str {
    match value {
        sparrowshell_domain::AuthType::Password => "password",
        sparrowshell_domain::AuthType::PrivateKey => "private_key",
    }
}

fn parse_time(value: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&value)
        .map(|v| v.with_timezone(&Utc))
        .unwrap_or(DateTime::UNIX_EPOCH)
}

fn map_folder(row: &Row<'_>) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: parse_id(&row.get::<_, String>(0)?),
        parent_id: row.get::<_, Option<String>>(1)?.map(|v| parse_id(&v)),
        name: row.get(2)?,
        sort_order: row.get(3)?,
        revision: row.get(4)?,
        updated_at: parse_time(row.get(5)?),
    })
}

fn map_server(row: &Row<'_>) -> rusqlite::Result<LocalServerRecord> {
    Ok(LocalServerRecord {
        id: parse_id(&row.get::<_, String>("id")?),
        folder_id: row
            .get::<_, Option<String>>("folder_id")?
            .map(|v| parse_id(&v)),
        name: row.get("name")?,
        host: row.get("host")?,
        port: row.get("port")?,
        username: row.get("username")?,
        auth_type: if row.get::<_, String>("auth_type")? == "password" {
            sparrowshell_domain::AuthType::Password
        } else {
            sparrowshell_domain::AuthType::PrivateKey
        },
        private_key_path_hint: row.get("private_key_path_hint")?,
        jump_server_id: row
            .get::<_, Option<String>>("jump_server_id")?
            .map(|v| parse_id(&v)),
        sort_order: row.get("sort_order")?,
        credential_ref: row.get("credential_ref")?,
        revision: row.get("revision")?,
        updated_at: parse_time(row.get("updated_at")?),
        deleted_at: row.get::<_, Option<String>>("deleted_at")?.map(parse_time),
    })
}

fn map_history(row: &Row<'_>) -> rusqlite::Result<CommandHistoryEntry> {
    Ok(CommandHistoryEntry {
        id: parse_id(&row.get::<_, String>(0)?),
        server_id: parse_id(&row.get::<_, String>(1)?),
        command: row.get(2)?,
        created_at: parse_time(row.get(3)?),
    })
}

/// Conservative local filter. Interactive shell capture is heuristic, so this
/// is a second boundary rather than a claim that every secret can be detected.
fn is_sensitive_command(command: &str) -> bool {
    let command = command.to_lowercase();
    let compact = command.replace(' ', "");
    if command.contains("sshpass")
        || command.contains("authorization:")
        || (command.trim_start().starts_with("curl ") && command.contains(" -u "))
        || (command.trim_start().starts_with("mysql ") && command.contains(" -p"))
    {
        return true;
    }
    [
        "password=",
        "passwd=",
        "passphrase=",
        "token=",
        "secret=",
        "api_key=",
        "apikey=",
        "--password",
        "--passwd",
        "--passphrase",
        "--token",
        "--secret",
        "--api-key",
        "--api_key",
    ]
    .iter()
    .any(|pattern| compact.contains(pattern))
}

fn parse_id<T: std::str::FromStr>(value: &str) -> T {
    T::from_str(value).unwrap_or_else(|_| unreachable!("database IDs are UUIDs"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sparrowshell_domain::{AuthType, ServerSyncDto};

    fn input(folder_id: Option<FolderId>) -> ServerInput {
        ServerInput {
            id: None,
            folder_id,
            name: "database".into(),
            host: "127.0.0.1".into(),
            port: 2222,
            username: "tester".into(),
            auth_type: AuthType::Password,
            private_key_path_hint: None,
            jump_server_id: None,
        }
    }

    #[test]
    fn folders_servers_and_outbox_survive_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("catalog.db");
        let (folder_id, server_id) = {
            let repository = Repository::open(&path).unwrap();
            let folder = repository.create_folder(None, "Production").unwrap();
            let child = repository
                .create_folder(Some(folder.id), "Databases")
                .unwrap();
            let server = repository.upsert_server(input(Some(child.id))).unwrap();
            repository
                .set_credential_ref(server.id, Some("ssh/local-only".into()))
                .unwrap();
            assert_eq!(repository.outbox_len().unwrap(), 3);
            let journal: String = repository
                .connection
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(journal, "wal");
            (child.id, server.id)
        };
        let repository = Repository::open(&path).unwrap();
        let snapshot = repository.snapshot().unwrap();
        assert_eq!(snapshot.servers[0].id, server_id);
        assert!(snapshot.folders.iter().any(|folder| folder.id == folder_id));
        assert_eq!(repository.outbox_len().unwrap(), 3);
    }

    #[test]
    fn folder_cycles_and_invalid_parent_are_rejected() {
        let repository = Repository::in_memory().unwrap();
        let root = repository.create_folder(None, "root").unwrap();
        let child = repository.create_folder(Some(root.id), "child").unwrap();
        assert!(matches!(
            repository.save_folder(root.id, Some(child.id), "root"),
            Err(StorageError::InvalidFolderHierarchy)
        ));
        assert!(matches!(
            repository.create_folder(Some(FolderId::new()), "missing"),
            Err(StorageError::FolderNotFound)
        ));
    }

    #[test]
    fn server_moves_preserve_order_values_and_snapshot_uses_names() {
        let repository = Repository::in_memory().unwrap();
        let first = repository.create_folder(None, "first").unwrap();
        let second = repository.create_folder(None, "second").unwrap();
        let a = repository
            .upsert_server(ServerInput {
                name: "49".into(),
                ..input(Some(first.id))
            })
            .unwrap();
        let b = repository
            .upsert_server(ServerInput {
                name: "48".into(),
                ..input(Some(first.id))
            })
            .unwrap();
        let c = repository
            .upsert_server(ServerInput {
                name: "50".into(),
                ..input(Some(second.id))
            })
            .unwrap();
        repository
            .reorder_servers(vec![
                CatalogOrderItem {
                    id: b.id.0,
                    parent_id: Some(first.id.0),
                    sort_order: 20,
                },
                CatalogOrderItem {
                    id: a.id.0,
                    parent_id: Some(first.id.0),
                    sort_order: 10,
                },
                CatalogOrderItem {
                    id: c.id.0,
                    parent_id: Some(first.id.0),
                    sort_order: 30,
                },
            ])
            .unwrap();
        let snapshot = repository.snapshot().unwrap();
        let ordered: Vec<_> = snapshot
            .servers
            .iter()
            .filter(|server| server.folder_id == Some(first.id))
            .collect();
        assert_eq!(
            ordered.iter().map(|server| server.id).collect::<Vec<_>>(),
            vec![b.id, a.id, c.id]
        );
        assert_eq!(
            ordered
                .iter()
                .map(|server| server.sort_order)
                .collect::<Vec<_>>(),
            vec![0, 0, 0]
        );
        assert_eq!(
            repository.server(c.id).unwrap().unwrap().folder_id,
            Some(first.id)
        );
    }

    #[test]
    fn folder_reordering_rejects_cycles_without_applying_changes() {
        let repository = Repository::in_memory().unwrap();
        let root = repository.create_folder(None, "root").unwrap();
        let child = repository.create_folder(Some(root.id), "child").unwrap();
        assert!(matches!(
            repository.reorder_folders(vec![CatalogOrderItem {
                id: root.id.0,
                parent_id: Some(child.id.0),
                sort_order: 0
            }]),
            Err(StorageError::InvalidFolderHierarchy)
        ));
        assert_eq!(repository.folder(root.id).unwrap().unwrap().parent_id, None);
    }

    #[test]
    fn editing_server_keeps_identity_and_secret_out_of_sync_payload() {
        let repository = Repository::in_memory().unwrap();
        let server = repository.upsert_server(input(None)).unwrap();
        let updated = repository
            .upsert_server(ServerInput {
                id: Some(server.id),
                name: "renamed".into(),
                ..input(None)
            })
            .unwrap();
        assert_eq!(updated.id, server.id);
        assert_eq!(updated.revision, 2);
        let payload: String = repository.connection.query_row("SELECT payload FROM sync_outbox WHERE entity_type='server' ORDER BY created_at DESC LIMIT 1", [], |r|r.get(0)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let _: ServerSyncDto = serde_json::from_value(value.clone()).unwrap();
        for forbidden in [
            "password",
            "privateKey",
            "private_key",
            "passphrase",
            "credentialRef",
            "credential_ref",
            "token",
        ] {
            assert!(
                !value.as_object().unwrap().contains_key(forbidden),
                "forbidden key: {forbidden}"
            );
        }
    }

    #[test]
    fn duplicate_server_uses_new_identity_without_copying_keychain_reference() {
        let repository = Repository::in_memory().unwrap();
        let folder = repository.create_folder(None, "Production").unwrap();
        let source = repository.upsert_server(input(Some(folder.id))).unwrap();
        repository
            .set_credential_ref(source.id, Some("ssh/source-only".into()))
            .unwrap();
        let duplicate = repository.duplicate_server(source.id).unwrap();
        assert_ne!(duplicate.id, source.id);
        assert_eq!(duplicate.name, "database-copy");
        assert_eq!(duplicate.folder_id, Some(folder.id));
        assert_eq!(duplicate.host, "127.0.0.1");
        assert_eq!(duplicate.credential_ref, None);
        assert_eq!(duplicate.revision, 1);
        assert_eq!(repository.outbox_len().unwrap(), 3);
        assert_eq!(
            repository
                .server(source.id)
                .unwrap()
                .unwrap()
                .credential_ref,
            Some("ssh/source-only".into())
        );
    }

    #[test]
    fn folder_delete_tombstones_whole_subtree_and_preserves_unrelated_catalog() {
        let repository = Repository::in_memory().unwrap();
        let root = repository.create_folder(None, "root").unwrap();
        let child = repository.create_folder(Some(root.id), "child").unwrap();
        let outside = repository.create_folder(None, "outside").unwrap();
        let removed = repository.upsert_server(input(Some(child.id))).unwrap();
        let retained = repository.upsert_server(input(Some(outside.id))).unwrap();
        repository
            .set_credential_ref(removed.id, Some("ssh/remove-local-only".into()))
            .unwrap();
        let references = repository.delete_folder(root.id).unwrap();
        assert_eq!(references, vec!["ssh/remove-local-only".to_owned()]);
        let snapshot = repository.snapshot().unwrap();
        assert_eq!(snapshot.folders.len(), 1);
        assert_eq!(snapshot.folders[0].id, outside.id);
        assert_eq!(snapshot.servers.len(), 1);
        assert_eq!(snapshot.servers[0].id, retained.id);
        assert_eq!(repository.outbox_len().unwrap(), 8);
        let deleted: i64 = repository.connection.query_row(
            "SELECT (SELECT count(*) FROM folders WHERE deleted_at IS NOT NULL)+(SELECT count(*) FROM servers WHERE deleted_at IS NOT NULL)",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(deleted, 3);
        let credential_ref: Option<String> = repository
            .connection
            .query_row(
                "SELECT credential_ref FROM servers WHERE id=?1",
                params![removed.id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(credential_ref, None);
        assert!(matches!(
            repository.delete_folder(root.id),
            Err(StorageError::FolderNotFound)
        ));
    }

    #[test]
    fn host_fingerprint_can_be_replaced_only_by_explicit_repository_call() {
        let repository = Repository::in_memory().unwrap();
        let first = HostKeyRecord {
            algorithm: "ssh-ed25519".into(),
            public_key: "first".into(),
            fingerprint: "SHA256:first".into(),
        };
        repository.trust_host_key("localhost", 22, &first).unwrap();
        assert_eq!(
            repository.trusted_host_key("localhost", 22).unwrap(),
            Some(first)
        );
    }

    #[test]
    fn command_history_is_local_filterable_and_clearable_without_touching_outbox() {
        let repository = Repository::in_memory().unwrap();
        let first = ServerId::new();
        let second = ServerId::new();
        repository.record_command(first, "uname -a").unwrap();
        repository.record_command(second, "df -h").unwrap();
        assert_eq!(repository.outbox_len().unwrap(), 0);
        assert_eq!(repository.command_history(None, 200).unwrap().len(), 2);
        let filtered = repository.command_history(Some(first), 200).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].command, "uname -a");
        assert_eq!(repository.clear_command_history(Some(first)).unwrap(), 1);
        let remaining = repository.command_history(None, 200).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].server_id, second);
    }

    #[test]
    fn old_catalog_and_outbox_survive_sync_schema_upgrade() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("old-catalog.db");
        let folder_id = FolderId::new();
        {
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch(
                "CREATE TABLE folders(id TEXT PRIMARY KEY,parent_id TEXT,name TEXT NOT NULL,sort_order INTEGER NOT NULL DEFAULT 0,revision INTEGER NOT NULL DEFAULT 1,updated_at TEXT NOT NULL);
                 CREATE TABLE sync_outbox(mutation_id TEXT PRIMARY KEY,entity_type TEXT NOT NULL,entity_id TEXT NOT NULL,operation TEXT NOT NULL,payload TEXT NOT NULL,created_at TEXT NOT NULL);",
            ).unwrap();
            connection.execute(
                "INSERT INTO folders(id,parent_id,name,sort_order,revision,updated_at) VALUES(?1,NULL,'legacy',0,1,?2)",
                params![folder_id.to_string(), Utc::now().to_rfc3339()],
            ).unwrap();
            connection.execute(
                "INSERT INTO sync_outbox(mutation_id,entity_type,entity_id,operation,payload,created_at) VALUES(?1,'folder',?2,'upsert','{}',?3)",
                params![uuid::Uuid::new_v4().to_string(), folder_id.to_string(), Utc::now().to_rfc3339()],
            ).unwrap();
        }
        let repository = Repository::open(&path).unwrap();
        assert_eq!(repository.snapshot().unwrap().folders.len(), 1);
        assert_eq!(repository.outbox_len().unwrap(), 1);
        assert_eq!(repository.outbox_batch(1).unwrap()[0].schema_version, 1);
        repository.save_folder(folder_id, None, "upgraded").unwrap();
        assert_eq!(repository.outbox_len().unwrap(), 2);
    }

    #[test]
    fn sensitive_command_canaries_never_enter_history() {
        let repository = Repository::in_memory().unwrap();
        let server = ServerId::new();
        for command in [
            "export API_TOKEN=canary-token-001",
            "mysql --password=canary-password-001",
            "sshpass -p canary-password-002 ssh example",
            "curl -u user:canary-password-003 https://example.invalid",
        ] {
            assert_eq!(repository.record_command(server, command).unwrap(), None);
        }
        assert!(repository.command_history(None, 200).unwrap().is_empty());
        let count: i64 = repository
            .connection
            .query_row(
                "SELECT count(*) FROM command_history WHERE command LIKE '%canary%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
