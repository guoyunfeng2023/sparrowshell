use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sparrowshell_secrets::{CredentialRef, CredentialStore, SecretError};
use zeroize::{Zeroize, Zeroizing};

const DEFAULT_ACCOUNT_REFERENCE: &str = "account/5ed3dd15-b59c-4d69-9ef4-10ad22d83a54";

/// Secret token payload. It intentionally has no Debug implementation.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountTokens {
    pub user_id: String,
    pub device_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub nickname: Option<String>,
}

impl Drop for AccountTokens {
    fn drop(&mut self) {
        self.user_id.zeroize();
        self.device_id.zeroize();
        self.access_token.zeroize();
        self.refresh_token.zeroize();
        self.email.zeroize();
        self.nickname.zeroize();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSession {
    pub user_id: String,
    pub device_id: String,
    pub expires_at: DateTime<Utc>,
    pub email: Option<String>,
    pub nickname: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "session")]
pub enum AccountStatus {
    SignedOut,
    SignedIn(AccountSession),
    Expired(AccountSession),
}

#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("operating system credential store is unavailable")]
    Store,
    #[error("stored account session is invalid")]
    InvalidSession,
}

/// Token lifecycle is deliberately separate from Repository: no method here can erase local data.
pub struct AccountTokenStore<S> {
    store: S,
    reference: CredentialRef,
}

impl<S: CredentialStore> AccountTokenStore<S> {
    pub fn new(store: S) -> Result<Self, AccountError> {
        let reference = CredentialRef::parse(DEFAULT_ACCOUNT_REFERENCE.to_owned())
            .map_err(|_| AccountError::InvalidSession)?;
        Ok(Self { store, reference })
    }

    pub fn save(&self, tokens: AccountTokens) -> Result<AccountSession, AccountError> {
        if tokens.user_id.trim().is_empty()
            || tokens.device_id.trim().is_empty()
            || tokens.access_token.trim().is_empty()
        {
            return Err(AccountError::InvalidSession);
        }
        let session = AccountSession {
            user_id: tokens.user_id.clone(),
            device_id: tokens.device_id.clone(),
            expires_at: tokens.expires_at,
            email: tokens.email.clone(),
            nickname: tokens.nickname.clone(),
        };
        let payload = serde_json::to_string(&tokens).map_err(|_| AccountError::InvalidSession)?;
        self.store
            .save(&self.reference, Zeroizing::new(payload))
            .map_err(|_| AccountError::Store)?;
        Ok(session)
    }

    pub fn tokens(&self) -> Result<Option<AccountTokens>, AccountError> {
        let Some(payload) = self.load_payload()? else {
            return Ok(None);
        };
        serde_json::from_str(payload.as_str())
            .map(Some)
            .map_err(|_| AccountError::InvalidSession)
    }

    pub fn update_profile(
        &self,
        email: String,
        nickname: String,
    ) -> Result<AccountSession, AccountError> {
        let mut tokens = self.tokens()?.ok_or(AccountError::InvalidSession)?;
        tokens.email = Some(email);
        tokens.nickname = Some(nickname);
        self.save(tokens)
    }

    pub fn status(&self) -> Result<AccountStatus, AccountError> {
        let Some(tokens) = self.tokens()? else {
            return Ok(AccountStatus::SignedOut);
        };
        let session = AccountSession {
            user_id: tokens.user_id.clone(),
            device_id: tokens.device_id.clone(),
            expires_at: tokens.expires_at,
            email: tokens.email.clone(),
            nickname: tokens.nickname.clone(),
        };
        if tokens.expires_at <= Utc::now() {
            Ok(AccountStatus::Expired(session))
        } else {
            Ok(AccountStatus::SignedIn(session))
        }
    }

    /// Local sign-out removes only the cloud token; catalog, history and outbox are untouched.
    pub fn sign_out(&self) -> Result<(), AccountError> {
        match self.store.delete(&self.reference) {
            Ok(()) | Err(SecretError::NotFound) => Ok(()),
            Err(_) => Err(AccountError::Store),
        }
    }

    fn load_payload(&self) -> Result<Option<Zeroizing<String>>, AccountError> {
        match self.store.load(&self.reference) {
            Ok(value) => Ok(Some(value)),
            Err(SecretError::NotFound) => Ok(None),
            Err(_) => Err(AccountError::Store),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sparrowshell_domain::{AuthType, ServerInput};
    use sparrowshell_secrets::MemoryCredentialStore;
    use sparrowshell_storage::Repository;

    fn tokens() -> AccountTokens {
        AccountTokens {
            user_id: "user-1".into(),
            device_id: "device-1".into(),
            access_token: "canary-access-token".into(),
            refresh_token: Some("canary-refresh-token".into()),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            email: Some("person@example.test".into()),
            nickname: Some("person@example.test".into()),
        }
    }

    #[test]
    fn sign_out_preserves_local_catalog_history_and_outbox() {
        let repository = Repository::in_memory().unwrap();
        repository
            .upsert_server(ServerInput {
                id: None,
                folder_id: None,
                name: "local".into(),
                host: "127.0.0.1".into(),
                port: 22,
                username: "tester".into(),
                auth_type: AuthType::Password,
                private_key_path_hint: None,
                jump_server_id: None,
            })
            .unwrap();
        let accounts = AccountTokenStore::new(MemoryCredentialStore::default()).unwrap();
        accounts.save(tokens()).unwrap();
        assert!(matches!(
            accounts.status().unwrap(),
            AccountStatus::SignedIn(_)
        ));
        accounts.sign_out().unwrap();
        assert_eq!(accounts.status().unwrap(), AccountStatus::SignedOut);
        assert_eq!(repository.snapshot().unwrap().servers.len(), 1);
        assert_eq!(repository.outbox_len().unwrap(), 1);
    }

    #[test]
    fn account_status_serialization_matches_desktop_contract() {
        let accounts = AccountTokenStore::new(MemoryCredentialStore::default()).unwrap();
        assert_eq!(
            serde_json::to_value(accounts.status().unwrap()).unwrap(),
            serde_json::json!({ "state": "signed_out" })
        );

        accounts.save(tokens()).unwrap();
        let signed_in = serde_json::to_value(accounts.status().unwrap()).unwrap();
        assert_eq!(signed_in["state"], "signed_in");
        assert_eq!(signed_in["session"]["userId"], "user-1");
        assert!(signed_in["session"].get("accessToken").is_none());

        let mut expired_tokens = tokens();
        expired_tokens.expires_at = Utc::now() - chrono::Duration::seconds(1);
        accounts.save(expired_tokens).unwrap();
        let expired = serde_json::to_value(accounts.status().unwrap()).unwrap();
        assert_eq!(expired["state"], "expired");
        assert_eq!(expired["session"]["userId"], "user-1");
    }

    #[test]
    fn account_serialization_is_only_used_inside_credential_store_boundary() {
        let accounts = AccountTokenStore::new(MemoryCredentialStore::default()).unwrap();
        let session = accounts.save(tokens()).unwrap();
        let public_json = serde_json::to_string(&session).unwrap();
        assert!(!public_json.contains("canary-access-token"));
        assert!(!public_json.contains("canary-refresh-token"));
        assert!(!public_json.contains("accessToken"));
        assert!(!public_json.contains("refreshToken"));
    }
}
