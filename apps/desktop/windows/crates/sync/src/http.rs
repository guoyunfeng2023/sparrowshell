use async_trait::async_trait;
use reqwest::{RequestBuilder, StatusCode, Url};
use sparrowshell_domain::{SyncMutation, SyncPullPage, SyncPushResult};
use sparrowshell_secrets::CredentialStore;
use uuid::Uuid;

use crate::{AccountTokenStore, AccountTokens, SyncError, SyncTransport};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub status: String,
    pub version: Option<String>,
    pub minimum_mac_os: Option<String>,
    pub target: Option<String>,
    pub channel: Option<String>,
    #[serde(default)]
    pub update_policy: Option<String>,
    #[serde(default)]
    pub release_notes: Option<String>,
    pub artifact_url: Option<String>,
    pub artifact_bytes: Option<u64>,
    pub sha256: Option<String>,
    pub signature: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackReceipt {
    pub feedback_id: String,
    pub status: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfile {
    pub email: String,
    pub nickname: String,
}

#[derive(Clone)]
pub struct CloudClient {
    base_url: Url,
    http: reqwest::Client,
}

impl CloudClient {
    pub fn new(value: &str) -> Result<Self, SyncError> {
        let normalized = if value.ends_with('/') {
            value.to_owned()
        } else {
            format!("{value}/")
        };
        let base_url =
            Url::parse(&normalized).map_err(|_| SyncError::Transport("invalid_cloud_url"))?;
        let loopback = matches!(base_url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
        if (base_url.scheme() != "https" && !(base_url.scheme() == "http" && loopback))
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
        {
            return Err(SyncError::Transport("invalid_cloud_url"));
        }
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|_| SyncError::Transport("client_initialization_failed"))?;
        Ok(Self { base_url, http })
    }

    pub async fn send_verification_code(
        &self,
        email: &str,
        purpose: &str,
        locale: &str,
    ) -> Result<(), SyncError> {
        let response = self
            .http
            .post(self.endpoint("client-api/v1/auth/verification-code")?)
            .json(&serde_json::json!({"email": email, "purpose": purpose, "locale": locale}))
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        empty(response).await
    }

    pub async fn register(
        &self,
        email: &str,
        password: String,
        code: &str,
        device_id: Uuid,
    ) -> Result<AccountTokens, SyncError> {
        let response = self
            .http
            .post(self.endpoint("client-api/v1/auth/register")?)
            .json(&serde_json::json!({
                "email": email, "password": password, "code": code, "device_id": device_id
            }))
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        decode(response).await
    }

    pub async fn reset_password(
        &self,
        email: &str,
        code: &str,
        new_password: String,
    ) -> Result<(), SyncError> {
        let response = self
            .http
            .post(self.endpoint("client-api/v1/auth/password-reset")?)
            .json(&serde_json::json!({"email": email, "code": code, "new_password": new_password}))
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        empty(response).await
    }

    pub async fn login(
        &self,
        email: &str,
        password: String,
        device_id: Uuid,
    ) -> Result<AccountTokens, SyncError> {
        let response = self
            .http
            .post(self.endpoint("client-api/v1/auth/login")?)
            .json(
                &serde_json::json!({"email": email, "password": password, "device_id": device_id}),
            )
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        decode(response).await
    }

    pub async fn refresh(
        &self,
        refresh_token: &str,
        device_id: Uuid,
    ) -> Result<AccountTokens, SyncError> {
        let response = self
            .http
            .post(self.endpoint("client-api/v1/auth/refresh")?)
            .json(&serde_json::json!({"refresh_token": refresh_token, "device_id": device_id}))
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        decode(response).await
    }

    pub async fn logout(&self, access_token: &str) -> Result<(), SyncError> {
        let response = self
            .authorized(
                self.http.post(self.endpoint("client-api/v1/auth/logout")?),
                access_token,
            )
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        empty(response).await
    }

    pub async fn delete_account(&self, access_token: &str) -> Result<(), SyncError> {
        let response = self
            .authorized(
                self.http.delete(self.endpoint("client-api/v1/account")?),
                access_token,
            )
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        empty(response).await
    }

    pub async fn profile(&self, access_token: &str) -> Result<AccountProfile, SyncError> {
        let response = self
            .authorized(
                self.http.get(self.endpoint("client-api/v1/account")?),
                access_token,
            )
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        decode(response).await
    }

    pub async fn update_nickname(
        &self,
        access_token: &str,
        nickname: &str,
    ) -> Result<AccountProfile, SyncError> {
        let response = self
            .authorized(
                self.http
                    .patch(self.endpoint("client-api/v1/account/profile")?),
                access_token,
            )
            .json(&serde_json::json!({"nickname": nickname}))
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        decode(response).await
    }

    pub async fn change_password(
        &self,
        access_token: &str,
        current_password: String,
        new_password: String,
    ) -> Result<(), SyncError> {
        let response = self
            .authorized(self.http.post(self.endpoint("client-api/v1/account/password")?), access_token)
            .json(&serde_json::json!({"current_password": current_password, "new_password": new_password}))
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        empty(response).await
    }

    pub async fn check_update(
        &self,
        current_version: &str,
        device_id: Uuid,
        target: &str,
        include_silent: bool,
    ) -> Result<UpdateCheck, SyncError> {
        let device_id = device_id.to_string();
        let include_silent = include_silent.to_string();
        let response = self
            .http
            .get(self.endpoint("client-api/v1/updates/check")?)
            .query(&[
                ("currentVersion", current_version),
                ("channel", "stable"),
                ("deviceId", device_id.as_str()),
                ("target", target),
                ("includeSilent", include_silent.as_str()),
            ])
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        decode(response).await
    }

    pub async fn submit_feedback(
        &self,
        access_token: Option<&str>,
        description: &str,
        contact: Option<&str>,
        attachment: Option<(String, String, Vec<u8>)>,
    ) -> Result<FeedbackReceipt, SyncError> {
        let mut form = reqwest::multipart::Form::new().text("description", description.to_owned());
        if let Some(contact) = contact.filter(|value| !value.is_empty()) {
            form = form.text("contact", contact.to_owned());
        }
        if let Some((name, content_type, bytes)) = attachment {
            let part = reqwest::multipart::Part::bytes(bytes)
                .file_name(name)
                .mime_str(&content_type)
                .map_err(|_| SyncError::Transport("request_rejected"))?;
            form = form.part("attachment", part);
        }
        let mut request = self
            .http
            .post(self.endpoint("client-api/v1/feedback")?)
            .multipart(form);
        if let Some(token) = access_token {
            request = self.authorized(request, token);
        }
        let response = request
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        decode(response).await
    }

    fn endpoint(&self, path: &str) -> Result<Url, SyncError> {
        self.base_url
            .join(path)
            .map_err(|_| SyncError::Transport("invalid_cloud_url"))
    }

    fn authorized(&self, request: RequestBuilder, access_token: &str) -> RequestBuilder {
        request.bearer_auth(access_token)
    }
}

pub struct HttpSyncTransport<S> {
    client: CloudClient,
    accounts: AccountTokenStore<S>,
}

impl<S: CredentialStore> HttpSyncTransport<S> {
    pub fn new(base_url: &str, store: S) -> Result<Self, SyncError> {
        let accounts = AccountTokenStore::new(store)
            .map_err(|_| SyncError::Transport("credential_store_unavailable"))?;
        Ok(Self {
            client: CloudClient::new(base_url)?,
            accounts,
        })
    }

    async fn access_token(&self) -> Result<String, SyncError> {
        let tokens = self
            .accounts
            .tokens()
            .map_err(|_| SyncError::Transport("credential_store_unavailable"))?
            .ok_or(SyncError::Transport("unauthorized"))?;
        if tokens.expires_at > chrono::Utc::now() + chrono::Duration::seconds(30) {
            return Ok(tokens.access_token.clone());
        }
        self.refresh().await
    }

    async fn refresh(&self) -> Result<String, SyncError> {
        let tokens = self
            .accounts
            .tokens()
            .map_err(|_| SyncError::Transport("credential_store_unavailable"))?
            .ok_or(SyncError::Transport("unauthorized"))?;
        let refresh_token = tokens
            .refresh_token
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or(SyncError::Transport("unauthorized"))?;
        let device_id =
            Uuid::parse_str(&tokens.device_id).map_err(|_| SyncError::Transport("unauthorized"))?;
        let refreshed = self.client.refresh(refresh_token, device_id).await?;
        let access_token = refreshed.access_token.clone();
        self.accounts
            .save(refreshed)
            .map_err(|_| SyncError::Transport("credential_store_unavailable"))?;
        Ok(access_token)
    }
}

#[async_trait]
impl<S: CredentialStore> SyncTransport for HttpSyncTransport<S> {
    async fn push(
        &self,
        device_id: Uuid,
        changes: Vec<SyncMutation>,
    ) -> Result<Vec<SyncPushResult>, SyncError> {
        let mut token = self.access_token().await?;
        let mut response = self
            .client
            .authorized(
                self.client
                    .http
                    .post(self.client.endpoint("client-api/v1/sync/push")?),
                &token,
            )
            .json(&serde_json::json!({"device_id": device_id, "changes": &changes}))
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            token = self.refresh().await?;
            response = self
                .client
                .authorized(
                    self.client
                        .http
                        .post(self.client.endpoint("client-api/v1/sync/push")?),
                    &token,
                )
                .json(&serde_json::json!({"device_id": device_id, "changes": &changes}))
                .send()
                .await
                .map_err(|_| SyncError::Transport("unavailable"))?;
        }
        decode(response).await
    }

    async fn pull(&self, cursor: u64, limit: usize) -> Result<SyncPullPage, SyncError> {
        let mut token = self.access_token().await?;
        let mut response = self
            .client
            .authorized(
                self.client
                    .http
                    .get(self.client.endpoint("client-api/v1/sync/pull")?),
                &token,
            )
            .query(&[
                ("cursor", cursor.to_string()),
                ("limit", limit.clamp(1, 100).to_string()),
            ])
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            token = self.refresh().await?;
            response = self
                .client
                .authorized(
                    self.client
                        .http
                        .get(self.client.endpoint("client-api/v1/sync/pull")?),
                    &token,
                )
                .query(&[
                    ("cursor", cursor.to_string()),
                    ("limit", limit.clamp(1, 100).to_string()),
                ])
                .send()
                .await
                .map_err(|_| SyncError::Transport("unavailable"))?;
        }
        if response.status() == StatusCode::GONE {
            return Ok(SyncPullPage {
                snapshot_required: true,
                ..SyncPullPage::default()
            });
        }
        decode(response).await
    }

    async fn snapshot(&self, cursor: u64, limit: usize) -> Result<SyncPullPage, SyncError> {
        let mut token = self.access_token().await?;
        let mut response = self
            .client
            .authorized(
                self.client
                    .http
                    .get(self.client.endpoint("client-api/v1/sync/snapshot")?),
                &token,
            )
            .query(&[
                ("cursor", cursor.to_string()),
                ("limit", limit.clamp(1, 100).to_string()),
            ])
            .send()
            .await
            .map_err(|_| SyncError::Transport("unavailable"))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            token = self.refresh().await?;
            response = self
                .client
                .authorized(
                    self.client
                        .http
                        .get(self.client.endpoint("client-api/v1/sync/snapshot")?),
                    &token,
                )
                .query(&[
                    ("cursor", cursor.to_string()),
                    ("limit", limit.clamp(1, 100).to_string()),
                ])
                .send()
                .await
                .map_err(|_| SyncError::Transport("unavailable"))?;
        }
        decode(response).await
    }
}

async fn decode<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, SyncError> {
    let status = response.status();
    if !status.is_success() {
        let body = response.bytes().await.unwrap_or_default();
        return Err(remote_error(status, &body));
    }
    response
        .json()
        .await
        .map_err(|_| SyncError::Protocol("response JSON could not be decoded"))
}

async fn empty(response: reqwest::Response) -> Result<(), SyncError> {
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        let body = response.bytes().await.unwrap_or_default();
        Err(remote_error(status, &body))
    }
}

fn remote_error(status: StatusCode, body: &[u8]) -> SyncError {
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(message) = value
            .pointer("/error/message")
            .and_then(|item| item.as_str())
        {
            let message = message.trim();
            if !message.is_empty() && message.len() <= 500 {
                return SyncError::Remote(message.to_owned());
            }
        }
    }
    transport_error(status)
}

fn transport_error(status: StatusCode) -> SyncError {
    match status {
        StatusCode::UNAUTHORIZED => SyncError::Transport("unauthorized"),
        StatusCode::TOO_MANY_REQUESTS => SyncError::Transport("rate_limited"),
        status if status.is_server_error() => SyncError::Transport("unavailable"),
        _ => SyncError::Transport("request_rejected"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_url_requires_https_except_for_loopback_development() {
        assert!(CloudClient::new("https://cloud.example.com").is_ok());
        assert!(CloudClient::new("http://127.0.0.1:8070").is_ok());
        assert!(CloudClient::new("http://cloud.example.com").is_err());
        assert!(CloudClient::new("https://user:secret@cloud.example.com").is_err());
    }

    #[test]
    fn cloud_url_preserves_reverse_proxy_subpaths() {
        let production = CloudClient::new("https://example.test/sparrowshell").unwrap();
        assert_eq!(
            "https://example.test/sparrowshell/client-api/v1/updates/check",
            production
                .endpoint("client-api/v1/updates/check")
                .unwrap()
                .as_str()
        );
        let with_slash = CloudClient::new("https://cloud.example.com/sparrowshell/").unwrap();
        assert_eq!(
            "https://cloud.example.com/sparrowshell/client-api/v1/feedback",
            with_slash
                .endpoint("client-api/v1/feedback")
                .unwrap()
                .as_str()
        );
        let loopback = CloudClient::new("http://127.0.0.1:8070").unwrap();
        assert_eq!(
            "http://127.0.0.1:8070/client-api/v1/sync/pull",
            loopback
                .endpoint("client-api/v1/sync/pull")
                .unwrap()
                .as_str()
        );
    }
}
