mod sync;

pub use sync::*;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use uuid::Uuid;

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
            pub fn parse(value: &str) -> Result<Self, DomainError> {
                Uuid::parse_str(value)
                    .map(Self)
                    .map_err(|_| DomainError::InvalidId(stringify!($name)))
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
        impl FromStr for $name {
            type Err = DomainError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }
    };
}

stable_id!(FolderId);
stable_id!(ServerId);
stable_id!(SessionId);
stable_id!(ChannelId);
stable_id!(TransferId);
stable_id!(HistoryId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRef {
    pub server_id: ServerId,
    pub session_id: SessionId,
    pub generation: u64,
}

impl TargetRef {
    pub fn next_generation(self) -> Result<Self, DomainError> {
        Ok(Self {
            generation: self
                .generation
                .checked_add(1)
                .ok_or(DomainError::GenerationOverflow)?,
            ..self
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    Password,
    PrivateKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerInput {
    pub id: Option<ServerId>,
    pub folder_id: Option<FolderId>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub private_key_path_hint: Option<String>,
    pub jump_server_id: Option<ServerId>,
}

impl ServerInput {
    pub fn validated(mut self) -> Result<Self, DomainError> {
        self.name = self.name.trim().to_owned();
        self.host = self.host.trim().to_owned();
        self.username = self.username.trim().to_owned();
        if self.name.is_empty() || self.name.len() > 128 {
            return Err(DomainError::InvalidName);
        }
        if self.host.is_empty()
            || self.host.len() > 253
            || self.host.chars().any(char::is_whitespace)
        {
            return Err(DomainError::InvalidHost);
        }
        if self.port == 0 {
            return Err(DomainError::InvalidPort);
        }
        if self.username.is_empty() || self.username.len() > 128 {
            return Err(DomainError::InvalidUsername);
        }
        if self.id.is_some() && self.id == self.jump_server_id {
            return Err(DomainError::InvalidJumpServer);
        }
        if self.auth_type == AuthType::PrivateKey
            && self
                .private_key_path_hint
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
        {
            return Err(DomainError::PrivateKeyPathRequired);
        }
        Ok(self)
    }
}

/// Local-only record. The credential reference is deliberately never serialized.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalServerRecord {
    pub id: ServerId,
    pub folder_id: Option<FolderId>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub private_key_path_hint: Option<String>,
    pub jump_server_id: Option<ServerId>,
    pub sort_order: i64,
    #[serde(skip)]
    pub credential_ref: Option<String>,
    pub revision: u64,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerSyncDto {
    pub id: ServerId,
    pub folder_id: Option<FolderId>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub private_key_path_hint: Option<String>,
    pub jump_server_id: Option<ServerId>,
    #[serde(default)]
    pub sort_order: i64,
    pub revision: u64,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl From<&LocalServerRecord> for ServerSyncDto {
    fn from(s: &LocalServerRecord) -> Self {
        Self {
            id: s.id,
            folder_id: s.folder_id,
            name: s.name.clone(),
            host: s.host.clone(),
            port: s.port,
            username: s.username.clone(),
            auth_type: s.auth_type,
            private_key_path_hint: s.private_key_path_hint.clone(),
            jump_server_id: s.jump_server_id,
            sort_order: s.sort_order,
            revision: s.revision,
            updated_at: s.updated_at,
            deleted_at: s.deleted_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: FolderId,
    pub parent_id: Option<FolderId>,
    pub name: String,
    pub sort_order: i64,
    pub revision: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSnapshot {
    pub folders: Vec<Folder>,
    pub servers: Vec<LocalServerRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogOrderItem {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Resolving,
    VerifyingTarget,
    Authenticating,
    Connected,
    HostKeyBlocked,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub target: TargetRef,
    pub state: ConnectionState,
    pub channel_id: Option<ChannelId>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandHistoryEntry {
    pub id: HistoryId,
    pub server_id: ServerId,
    pub command: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("invalid {0} identifier")]
    InvalidId(&'static str),
    #[error("generation overflow")]
    GenerationOverflow,
    #[error("server name is empty or too long")]
    InvalidName,
    #[error("host is empty, too long or contains whitespace")]
    InvalidHost,
    #[error("port must be non-zero")]
    InvalidPort,
    #[error("username is empty or too long")]
    InvalidUsername,
    #[error("a server cannot be its own jump host")]
    InvalidJumpServer,
    #[error("private key authentication requires a local key path")]
    PrivateKeyPathRequired,
    #[error("automation definition contains invalid, runtime-only or secret fields")]
    InvalidAutomationDefinition,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> LocalServerRecord {
        LocalServerRecord {
            id: ServerId::new(),
            folder_id: None,
            name: "test".into(),
            host: "127.0.0.1".into(),
            port: 22,
            username: "tester".into(),
            auth_type: AuthType::Password,
            private_key_path_hint: None,
            jump_server_id: None,
            sort_order: 0,
            credential_ref: Some("ssh/local-only-reference".into()),
            revision: 1,
            updated_at: Utc::now(),
            deleted_at: None,
        }
    }

    #[test]
    fn local_and_sync_serialization_never_contains_credential_reference() {
        let local = record();
        for json in [
            serde_json::to_string(&local).unwrap(),
            serde_json::to_string(&ServerSyncDto::from(&local)).unwrap(),
        ] {
            assert!(!json.contains("credentialRef"));
            assert!(!json.contains("credential_ref"));
            assert!(!json.contains("local-only-reference"));
        }
    }

    #[test]
    fn sync_contract_rejects_unexpected_secret_fields() {
        let mut value = serde_json::to_value(ServerSyncDto::from(&record())).unwrap();
        value["password"] = serde_json::Value::String("canary-not-a-real-secret".into());
        assert!(serde_json::from_value::<ServerSyncDto>(value).is_err());
    }

    #[test]
    fn reconnect_changes_generation_without_changing_identity() {
        let target = TargetRef {
            server_id: ServerId::new(),
            session_id: SessionId::new(),
            generation: 1,
        };
        let next = target.next_generation().unwrap();
        assert_eq!(next.server_id, target.server_id);
        assert_eq!(next.session_id, target.session_id);
        assert_eq!(next.generation, 2);
        assert_ne!(next, target);
    }

    #[test]
    fn server_validation_does_not_accept_invalid_endpoint_or_self_jump() {
        let base = ServerInput {
            id: None,
            folder_id: None,
            name: "db".into(),
            host: "db.local".into(),
            port: 22,
            username: "admin".into(),
            auth_type: AuthType::Password,
            private_key_path_hint: None,
            jump_server_id: None,
        };
        assert!(base.clone().validated().is_ok());
        assert!(matches!(
            ServerInput {
                host: "bad host".into(),
                ..base.clone()
            }
            .validated(),
            Err(DomainError::InvalidHost)
        ));
        let id = ServerId::new();
        assert!(matches!(
            ServerInput {
                id: Some(id),
                jump_server_id: Some(id),
                ..base
            }
            .validated(),
            Err(DomainError::InvalidJumpServer)
        ));
    }
}
