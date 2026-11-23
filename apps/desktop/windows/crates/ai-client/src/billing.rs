use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:8070";
const API_BASE_URL: &str = match option_env!("SPARROWSHELL_API_BASE_URL") {
    Some(value) => value,
    None => DEFAULT_API_BASE_URL,
};
const DEVICE_HEADER: &str = "X-SparrowShell-Anonymous-Id";
const TOKEN_HEADER: &str = "X-SparrowShell-Device-Token";

#[derive(Debug, Clone)]
pub struct BillingClient {
    client: reqwest::Client,
    device_id: String,
    device_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiEntitlement {
    pub plan: String,
    pub licensed: bool,
    pub bound: bool,
    pub quota: u32,
    pub used: u32,
    pub remaining: u32,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub expired: bool,
    pub previous_plan: Option<String>,
    pub previous_period_end: Option<String>,
    pub payment_enabled: bool,
    pub mock_payment: bool,
    #[serde(default)]
    pub payment_amount_override: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingOrder {
    pub token: String,
    pub plan: String,
    pub amount: f64,
    pub status: String,
    pub checkout_url: String,
    pub mock: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateOrder<'a> {
    plan: &'a str,
    license_key: Option<&'a str>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Activate<'a> {
    license_key: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Activation {
    pub device_token: String,
    pub entitlement: AiEntitlement,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoveredLicense {
    license_key: String,
}
#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    #[error("AI 额度服务暂时不可用，请稍后重试")]
    Unavailable,
    #[error("本机安装标识或许可证凭据无效")]
    InvalidIdentity,
    #[error("{0}")]
    Server(String),
    #[error("AI 额度服务返回了无效响应")]
    InvalidResponse,
}

impl BillingClient {
    pub fn new(device_id: String, device_token: Option<String>) -> Result<Self, BillingError> {
        uuid::Uuid::parse_str(&device_id).map_err(|_| BillingError::InvalidIdentity)?;
        if device_token.as_ref().is_some_and(|value| {
            value.len() != 64 || !value.bytes().all(|item| item.is_ascii_hexdigit())
        }) {
            return Err(BillingError::InvalidIdentity);
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| BillingError::Unavailable)?;
        Ok(Self {
            client,
            device_id,
            device_token,
        })
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let request = self
            .client
            .request(
                method,
                format!(
                    "{}/client-api/v1/{path}",
                    API_BASE_URL.trim_end_matches('/')
                ),
            )
            .header(DEVICE_HEADER, &self.device_id);
        match &self.device_token {
            Some(token) => request.header(TOKEN_HEADER, token),
            None => request,
        }
    }

    async fn read<T: for<'de> Deserialize<'de>>(
        response: reqwest::Response,
    ) -> Result<T, BillingError> {
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| BillingError::Unavailable)?;
        if bytes.len() > 128 * 1024 {
            return Err(BillingError::InvalidResponse);
        }
        if !status.is_success() {
            if let Ok(body) = serde_json::from_slice::<ErrorBody>(&bytes) {
                if let Some(message) = body
                    .message
                    .filter(|value| !value.trim().is_empty() && value.len() <= 500)
                {
                    return Err(BillingError::Server(message));
                }
            }
            return Err(BillingError::Unavailable);
        }
        serde_json::from_slice(&bytes).map_err(|_| BillingError::InvalidResponse)
    }

    pub async fn entitlement(&self) -> Result<AiEntitlement, BillingError> {
        Self::read(
            self.request(reqwest::Method::GET, "ai-entitlement")
                .send()
                .await
                .map_err(|_| BillingError::Unavailable)?,
        )
        .await
    }
    pub async fn create_order(
        &self,
        plan: &str,
        license_key: Option<&str>,
    ) -> Result<BillingOrder, BillingError> {
        if !matches!(plan, "personal" | "high") {
            return Err(BillingError::InvalidResponse);
        }
        Self::read(
            self.request(reqwest::Method::POST, "orders")
                .json(&CreateOrder { plan, license_key })
                .send()
                .await
                .map_err(|_| BillingError::Unavailable)?,
        )
        .await
    }
    pub async fn activate(&self, license_key: &str) -> Result<Activation, BillingError> {
        if license_key.len() > 64 || license_key.contains(['\r', '\n', '\0']) {
            return Err(BillingError::InvalidIdentity);
        }
        Self::read(
            self.request(reqwest::Method::POST, "licenses/activate")
                .json(&Activate { license_key })
                .send()
                .await
                .map_err(|_| BillingError::Unavailable)?,
        )
        .await
    }
    pub async fn recover(&self) -> Result<String, BillingError> {
        if self.device_token.is_none() {
            return Err(BillingError::InvalidIdentity);
        }
        let recovered: RecoveredLicense = Self::read(
            self.request(reqwest::Method::POST, "licenses/recover")
                .send()
                .await
                .map_err(|_| BillingError::Unavailable)?,
        )
        .await?;
        if recovered.license_key.len() > 64 || recovered.license_key.contains(['\r', '\n', '\0']) {
            return Err(BillingError::InvalidResponse);
        }
        Ok(recovered.license_key)
    }
    pub async fn deactivate(&self) -> Result<AiEntitlement, BillingError> {
        Self::read(
            self.request(reqwest::Method::POST, "licenses/deactivate")
                .send()
                .await
                .map_err(|_| BillingError::Unavailable)?,
        )
        .await
    }
    pub async fn release_task(&self, task_id: &str) -> Result<(), BillingError> {
        uuid::Uuid::parse_str(task_id).map_err(|_| BillingError::InvalidResponse)?;
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("ai-tasks/{task_id}/release"),
            )
            .send()
            .await
            .map_err(|_| BillingError::Unavailable)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(BillingError::Unavailable)
        }
    }
}
