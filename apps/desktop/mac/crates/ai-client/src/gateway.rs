//! Narrow client for the local AI Gateway. The model never supplies target,
//! execution mode, credentials or approval decisions.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sparrowshell_execution::{CommandProposal, RiskLevel};
use std::{fmt, time::Duration};

const API_BASE_URL: &str = match option_env!("SPARROWSHELL_API_BASE_URL") {
    Some(value) => value,
    None => "http://127.0.0.1:8070",
};
const MAX_RESPONSE_BYTES: usize = 512 * 1024;

fn gateway_endpoint(base_url: &str, path: &str) -> String {
    format!("{}/v1/ai/{path}", base_url.trim_end_matches('/'))
}

#[derive(Clone)]
pub struct GatewayIdentity {
    access_token: Option<String>,
    anonymous_id: String,
    device_token: Option<String>,
}

impl GatewayIdentity {
    pub fn new(access_token: Option<String>, anonymous_id: String) -> Result<Self, GatewayError> {
        Self::with_device_token(access_token, anonymous_id, None)
    }

    pub fn with_device_token(
        access_token: Option<String>,
        anonymous_id: String,
        device_token: Option<String>,
    ) -> Result<Self, GatewayError> {
        uuid::Uuid::parse_str(&anonymous_id).map_err(|_| GatewayError::InvalidIdentity)?;
        if access_token.as_ref().is_some_and(|token| {
            token.trim().is_empty() || token.len() > 4096 || token.contains(['\r', '\n'])
        }) || device_token.as_ref().is_some_and(|token| {
            token.len() != 64 || !token.bytes().all(|value| value.is_ascii_hexdigit())
        }) {
            return Err(GatewayError::InvalidIdentity);
        }
        Ok(Self {
            access_token,
            anonymous_id,
            device_token,
        })
    }
}

impl fmt::Debug for GatewayIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GatewayIdentity")
            .field(
                "access_token",
                &self.access_token.as_ref().map(|_| "[redacted]"),
            )
            .field("anonymous_id", &self.anonymous_id)
            .field(
                "device_token",
                &self.device_token.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct AiGateway {
    client: reqwest::Client,
    identity: GatewayIdentity,
    base_url: String,
    #[cfg(test)]
    endpoint: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GatewayRequest {
    client_request_id: String,
    route_id: String,
    intent: String,
    locale: String,
    context: Value,
    limits: Limits,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Limits {
    max_proposals: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GatewayResponse {
    request_id: String,
    route_id: String,
    provider: String,
    model: String,
    summary: String,
    commands: Vec<GatewayCommand>,
    #[serde(rename = "usage")]
    _usage: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GatewayCommand {
    command: String,
    #[serde(rename = "riskHint")]
    _risk_hint: Option<RiskLevel>,
    requires_interaction: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisGatewayRequest {
    client_request_id: String,
    route_id: String,
    locale: String,
    context: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AnalysisGatewayResponse {
    request_id: String,
    route_id: String,
    provider: String,
    model: String,
    analysis: String,
    #[serde(rename = "usage")]
    _usage: Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationMessage {
    pub role: ConversationRole,
    pub content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentGatewayRequest {
    client_request_id: String,
    task_id: String,
    route_id: String,
    session_id: Option<String>,
    intent: String,
    history: Vec<ConversationMessage>,
    locale: String,
    context: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentGatewayResponse {
    request_id: String,
    route_id: String,
    provider: String,
    model: String,
    session_id: String,
    #[serde(rename = "type")]
    decision_type: String,
    tool_call: Option<AgentToolCall>,
    final_answer: Option<AgentFinalAnswer>,
    #[serde(rename = "usage")]
    _usage: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentToolCall {
    summary: String,
    command: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentFinalAnswer {
    answer: String,
}

#[derive(Debug)]
pub enum AgentTurnResult {
    ToolCall {
        summary: String,
        proposal: CommandProposal,
        session_id: String,
    },
    FinalAnswer {
        answer: String,
        session_id: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("AI 服务暂时不可用，请稍后重试；终端和文件管理不受影响")]
    Unavailable,
    #[error("模型响应超时，请稍后重试")]
    Timeout,
    #[error("模型请求过于频繁，请稍后重试")]
    RateLimited,
    #[error("本周期 AI 次数已用完，请充值或等待下个周期")]
    QuotaExceeded,
    #[error("AI 请求或模型配置无效")]
    InvalidRequest,
    #[error("模型返回了无效或不完整的提案")]
    InvalidResponse,
    #[error("AI 生成了交互式命令，已停止自动执行；请使用手动终端，或要求 AI 使用非交互式命令")]
    InteractiveCommand,
    #[error("AI 用户身份无效，请重新启动客户端后重试")]
    InvalidIdentity,
    #[error("登录状态已失效，请重新登录；也可退出登录后匿名使用 AI")]
    Unauthorized,
}

impl AiGateway {
    pub fn new() -> Result<Self, GatewayError> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(125))
            .build()
            .map_err(|_| GatewayError::Unavailable)?;
        Ok(Self {
            client,
            identity: GatewayIdentity::new(None, uuid::Uuid::new_v4().to_string())?,
            base_url: API_BASE_URL.trim_end_matches('/').to_owned(),
            #[cfg(test)]
            endpoint: gateway_endpoint(API_BASE_URL, "proposals"),
        })
    }

    pub fn with_identity(&self, identity: GatewayIdentity) -> Self {
        Self {
            client: self.client.clone(),
            identity,
            base_url: self.base_url.clone(),
            #[cfg(test)]
            endpoint: self.endpoint.clone(),
        }
    }

    pub fn with_base_url(&self, base_url: &str) -> Self {
        Self {
            client: self.client.clone(),
            identity: self.identity.clone(),
            base_url: base_url.trim_end_matches('/').to_owned(),
            #[cfg(test)]
            endpoint: self.endpoint.clone(),
        }
    }

    fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let request = request.header("X-SparrowShell-Anonymous-Id", &self.identity.anonymous_id);
        let request = match &self.identity.device_token {
            Some(token) => request.header("X-SparrowShell-Device-Token", token),
            None => request,
        };
        match &self.identity.access_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    pub async fn propose(
        &self,
        intent: &str,
        locale: &str,
        authorized_context_json: String,
        max_proposals: usize,
    ) -> Result<(String, Vec<CommandProposal>), GatewayError> {
        self.propose_using(
            false,
            intent,
            locale,
            authorized_context_json,
            max_proposals,
        )
        .await
    }

    pub async fn propose_stream(
        &self,
        intent: &str,
        locale: &str,
        authorized_context_json: String,
        max_proposals: usize,
    ) -> Result<(String, Vec<CommandProposal>), GatewayError> {
        self.propose_using(true, intent, locale, authorized_context_json, max_proposals)
            .await
    }

    async fn propose_using(
        &self,
        stream: bool,
        intent: &str,
        locale: &str,
        authorized_context_json: String,
        max_proposals: usize,
    ) -> Result<(String, Vec<CommandProposal>), GatewayError> {
        if intent.trim().is_empty()
            || intent.len() > 8000
            || !(1..=8).contains(&max_proposals)
            || !matches!(locale, "zh-CN" | "en-US")
        {
            return Err(GatewayError::InvalidRequest);
        }
        let context = serde_json::from_str(&authorized_context_json)
            .map_err(|_| GatewayError::InvalidRequest)?;
        let request = GatewayRequest {
            client_request_id: uuid::Uuid::new_v4().to_string(),
            route_id: "qwen.application.completions".to_owned(),
            intent: intent.to_owned(),
            locale: locale.to_owned(),
            context,
            limits: Limits { max_proposals },
        };
        #[cfg(not(test))]
        let endpoint = if stream {
            gateway_endpoint(&self.base_url, "proposals/stream")
        } else {
            gateway_endpoint(&self.base_url, "proposals")
        };
        #[cfg(test)]
        let endpoint = if stream {
            self.endpoint.replace("/proposals", "/proposals/stream")
        } else {
            self.endpoint.clone()
        };
        let bytes = self.send_proposal(endpoint.as_ref(), &request).await?;
        let parsed: GatewayResponse = if stream {
            let body = std::str::from_utf8(&bytes).map_err(|_| GatewayError::InvalidResponse)?;
            parse_sse_final(body)?
        } else {
            serde_json::from_slice(&bytes).map_err(|_| GatewayError::InvalidResponse)?
        };
        validate_metadata(&parsed)?;
        if parsed.summary.trim().is_empty()
            || parsed.summary.len() > 4000
            || parsed.commands.is_empty()
            || parsed.commands.len() > max_proposals
        {
            return Err(GatewayError::InvalidResponse);
        }
        let mut proposals = Vec::with_capacity(parsed.commands.len());
        for command in parsed.commands {
            let command = command.command;
            if command.trim().is_empty()
                || command.len() > 4000
                || command.trim() != command
                || command.contains(['\0', '\n', '\r'])
            {
                return Err(GatewayError::InvalidResponse);
            }
            proposals.push(CommandProposal {
                command,
                model_risk: None,
            });
        }
        Ok((parsed.summary, proposals))
    }

    async fn send_proposal(
        &self,
        endpoint: &str,
        request: &GatewayRequest,
    ) -> Result<Vec<u8>, GatewayError> {
        let response = self
            .authorize(self.client.post(endpoint))
            .json(request)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    GatewayError::Timeout
                } else {
                    GatewayError::Unavailable
                }
            })?;
        let status = response.status();
        let mut response = response;
        if response
            .content_length()
            .is_some_and(|length| length as usize > MAX_RESPONSE_BYTES)
        {
            return Err(GatewayError::InvalidResponse);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| GatewayError::InvalidResponse)?
        {
            if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(GatewayError::InvalidResponse);
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            return Err(match status.as_u16() {
                400 => GatewayError::InvalidRequest,
                401 => GatewayError::Unauthorized,
                402 => GatewayError::QuotaExceeded,
                422 => GatewayError::InteractiveCommand,
                429 => GatewayError::RateLimited,
                502 => GatewayError::InvalidResponse,
                504 => GatewayError::Timeout,
                _ => GatewayError::Unavailable,
            });
        }
        Ok(bytes)
    }

    pub async fn analyze(
        &self,
        locale: &str,
        authorized_context_json: String,
    ) -> Result<String, GatewayError> {
        if !matches!(locale, "zh-CN" | "en-US") {
            return Err(GatewayError::InvalidRequest);
        }
        let context: Value = serde_json::from_str(&authorized_context_json)
            .map_err(|_| GatewayError::InvalidRequest)?;
        if context.as_object().is_none_or(|value| value.len() != 1)
            || context
                .get("recentResults")
                .and_then(Value::as_array)
                .is_none_or(|results| results.len() != 1)
        {
            return Err(GatewayError::InvalidRequest);
        }
        let request = AnalysisGatewayRequest {
            client_request_id: uuid::Uuid::new_v4().to_string(),
            route_id: "qwen.application.analysis".to_owned(),
            locale: locale.to_owned(),
            context,
        };
        #[cfg(not(test))]
        let endpoint = gateway_endpoint(&self.base_url, "analysis");
        #[cfg(test)]
        let endpoint = self.endpoint.replace("/proposals", "/analysis");
        let response = self
            .authorize(self.client.post(endpoint))
            .json(&request)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    GatewayError::Timeout
                } else {
                    GatewayError::Unavailable
                }
            })?;
        let status = response.status();
        let mut response = response;
        if response
            .content_length()
            .is_some_and(|length| length as usize > MAX_RESPONSE_BYTES)
        {
            return Err(GatewayError::InvalidResponse);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| GatewayError::InvalidResponse)?
        {
            if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(GatewayError::InvalidResponse);
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            return Err(match status.as_u16() {
                400 => GatewayError::InvalidRequest,
                401 => GatewayError::Unauthorized,
                402 => GatewayError::QuotaExceeded,
                422 => GatewayError::InteractiveCommand,
                429 => GatewayError::RateLimited,
                502 => GatewayError::InvalidResponse,
                504 => GatewayError::Timeout,
                _ => GatewayError::Unavailable,
            });
        }
        let parsed: AnalysisGatewayResponse =
            serde_json::from_slice(&bytes).map_err(|_| GatewayError::InvalidResponse)?;
        uuid::Uuid::parse_str(&parsed.request_id).map_err(|_| GatewayError::InvalidResponse)?;
        if parsed.route_id != "qwen.application.analysis"
            || parsed.provider != "qwen"
            || parsed.model.trim().is_empty()
            || parsed.analysis.trim().is_empty()
            || parsed.analysis.len() > 16000
            || parsed.analysis.contains('\0')
        {
            return Err(GatewayError::InvalidResponse);
        }
        Ok(parsed.analysis)
    }

    pub async fn agent_turn(
        &self,
        intent: &str,
        history: Vec<ConversationMessage>,
        locale: &str,
        session_id: Option<String>,
        authorized_context_json: String,
        task_id: String,
    ) -> Result<AgentTurnResult, GatewayError> {
        if uuid::Uuid::parse_str(&task_id).is_err() {
            return Err(GatewayError::InvalidRequest);
        }
        if intent.trim().is_empty()
            || intent.len() > 8000
            || !matches!(locale, "zh-CN" | "en-US")
            || history.len() > 12
            || history.iter().any(|message| {
                message.content.trim().is_empty()
                    || message.content.len() > 8000
                    || message.content.contains('\0')
            })
            || session_id.as_ref().is_some_and(|value| {
                value.trim().is_empty() || value.len() > 256 || value.contains('\0')
            })
        {
            return Err(GatewayError::InvalidRequest);
        }
        let context = serde_json::from_str(&authorized_context_json)
            .map_err(|_| GatewayError::InvalidRequest)?;
        let request = AgentGatewayRequest {
            client_request_id: uuid::Uuid::new_v4().to_string(),
            task_id,
            route_id: "qwen.application.agent".to_owned(),
            session_id: session_id.clone(),
            intent: intent.to_owned(),
            history,
            locale: locale.to_owned(),
            context,
        };
        #[cfg(not(test))]
        let endpoint = gateway_endpoint(&self.base_url, "agent/turn");
        #[cfg(test)]
        let endpoint = self.endpoint.replace("/proposals", "/agent/turn");
        let bytes = self.send_json(endpoint.as_ref(), &request).await?;
        let parsed: AgentGatewayResponse =
            serde_json::from_slice(&bytes).map_err(|_| GatewayError::InvalidResponse)?;
        uuid::Uuid::parse_str(&parsed.request_id).map_err(|_| GatewayError::InvalidResponse)?;
        if parsed.route_id != "qwen.application.agent"
            || parsed.provider != "qwen"
            || parsed.model.trim().is_empty()
            || parsed.session_id.trim().is_empty()
            || parsed.session_id.len() > 256
            || parsed.session_id.contains('\0')
            || session_id
                .as_ref()
                .is_some_and(|expected| expected != &parsed.session_id)
        {
            return Err(GatewayError::InvalidResponse);
        }
        let session_id = parsed.session_id;
        match (
            parsed.decision_type.as_str(),
            parsed.tool_call,
            parsed.final_answer,
        ) {
            ("tool_call", Some(tool), None)
                if !tool.summary.trim().is_empty()
                    && tool.summary.len() <= 4000
                    && !tool.command.trim().is_empty()
                    && tool.command.len() <= 4000
                    && tool.command.trim() == tool.command
                    && !tool.command.contains(['\0', '\n', '\r']) =>
            {
                Ok(AgentTurnResult::ToolCall {
                    summary: tool.summary,
                    proposal: CommandProposal {
                        command: tool.command,
                        model_risk: None,
                    },
                    session_id,
                })
            }
            ("final_answer", None, Some(answer))
                if !answer.answer.trim().is_empty()
                    && answer.answer.len() <= 16000
                    && !answer.answer.contains('\0') =>
            {
                Ok(AgentTurnResult::FinalAnswer {
                    answer: answer.answer,
                    session_id,
                })
            }
            _ => Err(GatewayError::InvalidResponse),
        }
    }

    async fn send_json<T: Serialize + ?Sized>(
        &self,
        endpoint: &str,
        request: &T,
    ) -> Result<Vec<u8>, GatewayError> {
        let response = self
            .authorize(self.client.post(endpoint))
            .json(request)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    GatewayError::Timeout
                } else {
                    GatewayError::Unavailable
                }
            })?;
        let status = response.status();
        let mut response = response;
        if response
            .content_length()
            .is_some_and(|length| length as usize > MAX_RESPONSE_BYTES)
        {
            return Err(GatewayError::InvalidResponse);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| GatewayError::InvalidResponse)?
        {
            if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(GatewayError::InvalidResponse);
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            return Err(match status.as_u16() {
                400 => GatewayError::InvalidRequest,
                401 => GatewayError::Unauthorized,
                402 => GatewayError::QuotaExceeded,
                422 => GatewayError::InteractiveCommand,
                429 => GatewayError::RateLimited,
                502 => GatewayError::InvalidResponse,
                504 => GatewayError::Timeout,
                _ => GatewayError::Unavailable,
            });
        }
        Ok(bytes)
    }
}

fn parse_sse_final(body: &str) -> Result<GatewayResponse, GatewayError> {
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(GatewayError::InvalidResponse);
    }
    let normalized = body.replace("\r\n", "\n");
    let mut final_payload: Option<String> = None;
    for event in normalized.split("\n\n") {
        if event.trim().is_empty() {
            continue;
        }
        let mut name = "";
        let mut data = String::new();
        for line in event.lines() {
            if let Some(value) = line.strip_prefix("event:") {
                name = value.trim();
            } else if let Some(value) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value.trim_start());
            }
        }
        match name {
            "final" => {
                if final_payload.replace(data).is_some() {
                    return Err(GatewayError::InvalidResponse);
                }
            }
            "error" => {
                return Err(if data.contains("timeout") {
                    GatewayError::Timeout
                } else if data.contains("rate_limit") {
                    GatewayError::RateLimited
                } else {
                    GatewayError::Unavailable
                });
            }
            _ => {}
        }
    }
    let payload = final_payload.ok_or(GatewayError::InvalidResponse)?;
    serde_json::from_str(&payload).map_err(|_| GatewayError::InvalidResponse)
}

fn validate_metadata(response: &GatewayResponse) -> Result<(), GatewayError> {
    uuid::Uuid::parse_str(&response.request_id).map_err(|_| GatewayError::InvalidResponse)?;
    if response.route_id != "qwen.application.completions"
        || response.provider != "qwen"
        || response.model.trim().is_empty()
        || response
            .commands
            .iter()
            .any(|command| command.requires_interaction)
    {
        return Err(GatewayError::InvalidResponse);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn gateway_server(
        body: &'static str,
        status: &'static str,
    ) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener");
        let address = listener.local_addr().expect("test address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accepted request");
            let mut request = [0_u8; 16 * 1024];
            let read = stream.read(&mut request).expect("read request");
            let request = String::from_utf8_lossy(&request[..read]).into_owned();
            let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            request
        });
        (format!("http://{address}/v1/ai/proposals"), handle)
    }

    fn valid_body() -> &'static str {
        r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.completions","provider":"qwen","model":"qwen3.8-max","summary":"检查磁盘","commands":[{"command":"df -h","riskHint":"low","requiresInteraction":false}],"usage":{}}"#
    }

    #[tokio::test]
    async fn sends_persistent_anonymous_identity_and_optional_bearer() {
        let anonymous_id = "d2d5bafa-76ce-4a72-9db7-a82f527dd17b";
        for token in [None, Some("secret-access-token".to_owned())] {
            let (endpoint, request) = gateway_server(valid_body(), "200 OK");
            let mut gateway = AiGateway::new().expect("gateway");
            gateway.endpoint = endpoint;
            let identity =
                GatewayIdentity::new(token.clone(), anonymous_id.to_owned()).expect("identity");
            assert!(!format!("{identity:?}").contains("secret-access-token"));
            gateway
                .with_identity(identity)
                .propose("检查磁盘", "zh-CN", "{}".into(), 1)
                .await
                .expect("identified proposal");
            let request = request.join().expect("request thread").to_ascii_lowercase();
            assert!(request.contains(&format!("x-sparrowshell-anonymous-id: {anonymous_id}")));
            assert_eq!(
                request.contains("authorization: bearer secret-access-token"),
                token.is_some()
            );
        }
        assert!(GatewayIdentity::new(None, "invalid".into()).is_err());
    }

    #[tokio::test]
    async fn accepts_only_complete_proposals_and_ignores_model_risk_claims() {
        let (endpoint, request) = gateway_server(valid_body(), "200 OK");
        let mut gateway = AiGateway::new().expect("gateway");
        gateway.endpoint = endpoint;
        let (summary, proposals) = gateway
            .propose("检查磁盘", "zh-CN", "{}".into(), 1)
            .await
            .expect("proposal");
        assert_eq!(summary, "检查磁盘");
        assert_eq!(
            proposals,
            vec![CommandProposal {
                command: "df -h".into(),
                model_risk: None
            }]
        );
        let request = request.join().expect("request thread");
        assert!(request.contains("\"intent\":\"检查磁盘\""));
        assert!(!request.contains("target") && !request.contains("mode"));
    }

    #[tokio::test]
    async fn stream_gateway_uses_pinned_route_and_complete_final() {
        let body: &'static str = Box::leak(
            format!("event: meta\ndata: {{\"routeId\":\"qwen.application.completions\"}}\n\nevent: final\ndata: {}\n\n", valid_body())
                .into_boxed_str(),
        );
        let (endpoint, request) = gateway_server(body, "200 OK");
        let mut gateway = AiGateway::new().expect("gateway");
        gateway.endpoint = endpoint;
        let (_, proposals) = gateway
            .propose_stream("检查磁盘", "zh-CN", "{}".into(), 1)
            .await
            .expect("stream proposal");
        assert_eq!(proposals[0].command, "df -h");
        let request = request.join().expect("request thread");
        assert!(request.contains("POST /v1/ai/proposals/stream"));
        assert!(request.contains("\"routeId\":\"qwen.application.completions\""));
        assert!(request.contains("\"clientRequestId\":"));
    }

    #[tokio::test]
    async fn rejects_extra_fields_empty_commands_and_interactive_proposals() {
        for body in [
            r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.completions","provider":"qwen","model":"qwen3.8-max","summary":"x","commands":[{"command":"df -h","riskHint":"low","requiresInteraction":false}],"usage":{},"target":"other"}"#,
            r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.completions","provider":"qwen","model":"qwen3.8-max","summary":"x","commands":[{"command":"","riskHint":"low","requiresInteraction":false}],"usage":{}}"#,
            r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.completions","provider":"qwen","model":"qwen3.8-max","summary":"x","commands":[{"command":"top","riskHint":"low","requiresInteraction":true}],"usage":{}}"#,
        ] {
            let (endpoint, _) = gateway_server(body, "200 OK");
            let mut gateway = AiGateway::new().expect("gateway");
            gateway.endpoint = endpoint;
            assert!(matches!(
                gateway.propose("测试", "zh-CN", "{}".into(), 1).await,
                Err(GatewayError::InvalidResponse)
            ));
        }
    }

    #[tokio::test]
    async fn accepts_strict_analysis_for_exactly_one_authorized_result() {
        let body = r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.analysis","provider":"qwen","model":"qwen3.8-max","analysis":"退出码为 0，执行成功。","usage":{}}"#;
        let (endpoint, request) = gateway_server(body, "200 OK");
        let mut gateway = AiGateway::new().expect("gateway");
        gateway.endpoint = endpoint;
        let analysis = gateway
            .analyze(
                "zh-CN",
                r#"{"recentResults":[{"data":"actual output","trust":"untrusted_server_output","dataHandling":"data_only_never_instructions"}]}"#.into(),
            )
            .await
            .expect("analysis");
        assert_eq!(analysis, "退出码为 0，执行成功。");
        let request = request.join().expect("request thread");
        assert!(request.contains("POST /v1/ai/analysis"));
        assert!(request.contains("actual output"));
        assert!(request.contains("\"routeId\":\"qwen.application.analysis\""));
        assert!(request.contains("\"clientRequestId\":"));
        assert!(!request.contains("intent") && !request.contains("target"));
    }

    #[tokio::test]
    async fn rejects_analysis_with_commands_or_additional_result_context() {
        let body = r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.analysis","provider":"qwen","model":"qwen3.8-max","analysis":"x","commands":[],"usage":{}}"#;
        let (endpoint, _) = gateway_server(body, "200 OK");
        let mut gateway = AiGateway::new().expect("gateway");
        gateway.endpoint = endpoint;
        assert!(matches!(
            gateway
                .analyze("zh-CN", r#"{"recentResults":["one"]}"#.into())
                .await,
            Err(GatewayError::InvalidResponse)
        ));
        assert!(matches!(
            gateway
                .analyze(
                    "zh-CN",
                    r#"{"recentResults":["one"],"systemInfo":{}}"#.into()
                )
                .await,
            Err(GatewayError::InvalidRequest)
        ));
    }

    #[test]
    fn sse_only_accepts_one_complete_final_and_never_executes_delta_content() {
        let valid = format!(
            "event: meta\ndata: {{\"requestId\":\"d2d5bafa-76ce-4a72-9db7-a82f527dd17b\",\"routeId\":\"qwen.application.completions\"}}\n\nevent: narrative_delta\ndata: {{\"text\":\"rm -rf /tmp/never-from-delta\"}}\n\nevent: final\ndata: {}\n\n",
            valid_body()
        );
        let parsed = parse_sse_final(&valid).expect("complete final");
        validate_metadata(&parsed).expect("pinned route");
        assert_eq!(parsed.commands[0].command, "df -h");

        for invalid in [
            "event: narrative_delta\ndata: {\"text\":\"df -h\"}\n\n".to_owned(),
            "event: final\ndata: {\"requestId\":".to_owned(),
            format!(
                "event: final\ndata: {}\n\nevent: final\ndata: {}\n\n",
                valid_body(),
                valid_body()
            ),
            "event: error\ndata: {\"code\":\"timeout\"}\n\n".to_owned(),
        ] {
            assert!(
                parse_sse_final(&invalid).is_err(),
                "accepted invalid SSE: {invalid}"
            );
        }
    }

    #[tokio::test]
    async fn agent_turn_returns_one_tool_call_or_final_answer() {
        let tool_body = r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.agent","provider":"qwen","model":"qwen3.8-max","sessionId":"session-1","type":"tool_call","toolCall":{"summary":"检查 Docker","command":"command -v docker"},"finalAnswer":null,"usage":{}}"#;
        let (endpoint, request) = gateway_server(tool_body, "200 OK");
        let mut gateway = AiGateway::new().expect("gateway");
        gateway.endpoint = endpoint;
        let result = gateway
            .agent_turn(
                "检查 Docker",
                vec![ConversationMessage {
                    role: ConversationRole::User,
                    content: "之前的问题".into(),
                }],
                "zh-CN",
                None,
                "{}".into(),
                uuid::Uuid::new_v4().to_string(),
            )
            .await
            .expect("agent tool call");
        match result {
            AgentTurnResult::ToolCall {
                summary,
                proposal,
                session_id,
            } => {
                assert_eq!(summary, "检查 Docker");
                assert_eq!(proposal.command, "command -v docker");
                assert_eq!(proposal.model_risk, None);
                assert_eq!(session_id, "session-1");
            }
            _ => panic!("expected tool call"),
        }
        let request = request.join().expect("request thread");
        assert!(request.contains("POST /v1/ai/agent/turn"));
        assert!(request.contains("\"routeId\":\"qwen.application.agent\""));
        assert!(request.contains("之前的问题"));
        assert!(!request.contains("target") && !request.contains("mode"));

        let answer_body = r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.agent","provider":"qwen","model":"qwen3.8-max","sessionId":"session-1","type":"final_answer","toolCall":null,"finalAnswer":{"answer":"这是普通问答。"},"usage":{}}"#;
        let (endpoint, request) = gateway_server(answer_body, "200 OK");
        let mut gateway = AiGateway::new().expect("gateway");
        gateway.endpoint = endpoint;
        let result = gateway
            .agent_turn(
                "普通问题",
                vec![],
                "zh-CN",
                Some("session-1".into()),
                "{}".into(),
                uuid::Uuid::new_v4().to_string(),
            )
            .await
            .expect("agent answer");
        assert!(matches!(
            result,
            AgentTurnResult::FinalAnswer { answer, session_id }
                if answer == "这是普通问答。" && session_id == "session-1"
        ));
        let request = request.join().expect("request thread");
        assert!(request.contains("\"sessionId\":\"session-1\""));
    }

    #[tokio::test]
    async fn agent_turn_rejects_invalid_decisions_and_unknown_fields() {
        for body in [
            r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.agent","provider":"qwen","model":"qwen3.8-max","sessionId":"session-1","type":"other","toolCall":null,"finalAnswer":null,"usage":{}}"#,
            r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.agent","provider":"qwen","model":"qwen3.8-max","sessionId":"session-1","type":"tool_call","toolCall":{"summary":"x","command":""},"finalAnswer":null,"usage":{}}"#,
            r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.agent","provider":"qwen","model":"qwen3.8-max","sessionId":"session-1","type":"tool_call","toolCall":{"summary":"x","command":"echo one\necho two"},"finalAnswer":null,"usage":{}}"#,
            r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.agent","provider":"qwen","model":"qwen3.8-max","sessionId":"session-1","type":"tool_call","toolCall":{"summary":"x"},"finalAnswer":null,"usage":{}}"#,
            r#"{"requestId":"d2d5bafa-76ce-4a72-9db7-a82f527dd17b","routeId":"qwen.application.agent","provider":"qwen","model":"qwen3.8-max","sessionId":"session-1","type":"final_answer","toolCall":null,"finalAnswer":{"answer":"x"},"usage":{},"target":"other"}"#,
        ] {
            let (endpoint, _) = gateway_server(body, "200 OK");
            let mut gateway = AiGateway::new().expect("gateway");
            gateway.endpoint = endpoint;
            assert!(matches!(
                gateway
                    .agent_turn(
                        "测试",
                        vec![],
                        "zh-CN",
                        None,
                        "{}".into(),
                        uuid::Uuid::new_v4().to_string()
                    )
                    .await,
                Err(GatewayError::InvalidResponse)
            ));
        }
    }

    #[tokio::test]
    async fn agent_turn_rejects_excessive_history_before_http() {
        let gateway = AiGateway::new().expect("gateway");
        let history = (0..13)
            .map(|index| ConversationMessage {
                role: if index % 2 == 0 {
                    ConversationRole::User
                } else {
                    ConversationRole::Assistant
                },
                content: format!("消息{index}"),
            })
            .collect();
        assert!(matches!(
            gateway
                .agent_turn(
                    "继续",
                    history,
                    "zh-CN",
                    None,
                    "{}".into(),
                    uuid::Uuid::new_v4().to_string()
                )
                .await,
            Err(GatewayError::InvalidRequest)
        ));
    }

    #[tokio::test]
    async fn maps_http_failures_without_returning_provider_body() {
        let (endpoint, _) =
            gateway_server("provider body must not escape", "429 Too Many Requests");
        let mut gateway = AiGateway::new().expect("gateway");
        gateway.endpoint = endpoint;
        let error = gateway
            .propose("测试", "zh-CN", "{}".into(), 1)
            .await
            .expect_err("rate limit");
        assert!(matches!(error, GatewayError::RateLimited));
        assert!(!error.to_string().contains("provider body"));
    }

    #[tokio::test]
    async fn maps_interactive_commands_to_a_specific_safe_message() {
        let (endpoint, _) = gateway_server("raw provider detail", "422 Unprocessable Entity");
        let mut gateway = AiGateway::new().expect("gateway");
        gateway.endpoint = endpoint;

        let error = gateway
            .agent_turn(
                "实时查看 CPU",
                vec![],
                "zh-CN",
                None,
                "{}".into(),
                uuid::Uuid::new_v4().to_string(),
            )
            .await
            .expect_err("interactive command");

        assert!(matches!(error, GatewayError::InteractiveCommand));
        assert!(error.to_string().contains("交互式命令"));
        assert!(error.to_string().contains("手动终端"));
        assert!(!error.to_string().contains("raw provider detail"));
    }

    #[tokio::test]
    async fn maps_bad_gateway_to_invalid_model_response_not_connection_failure() {
        let (endpoint, _) = gateway_server("raw provider detail", "502 Bad Gateway");
        let mut gateway = AiGateway::new().expect("gateway");
        gateway.endpoint = endpoint;

        let error = gateway
            .propose("检查磁盘", "zh-CN", "{}".into(), 1)
            .await
            .expect_err("invalid provider response");

        assert!(matches!(error, GatewayError::InvalidResponse));
        assert!(!error.to_string().contains("服务连接失败"));
        assert!(!error.to_string().contains("raw provider detail"));
    }
}
