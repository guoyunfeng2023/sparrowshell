package com.sparrowshell.ai;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Component;
import java.nio.charset.StandardCharsets;

/** Qwen application adapter. The SDK keeps the API key and app call on the server. */
@Component
public class QwenProvider implements AiProvider {
    static final String ENDPOINT = "https://dashscope.aliyuncs.com/api/v1/apps/{appId}/completion";
    static final String PROPOSAL_ROUTE = "qwen.application.completions";
    static final String ANALYSIS_ROUTE = "qwen.application.analysis";
    static final String AGENT_ROUTE = "qwen.application.agent";

    private final String apiKey;
    private final String appId;
    private final String model;
    private final ObjectMapper mapper;
    private final QwenClient client;
    private final DeepSeekProvider delegate;

    @org.springframework.beans.factory.annotation.Autowired
    public QwenProvider(@Value("${sparrowshell.qwen.api-key:}") String apiKey,
                        @Value("${sparrowshell.qwen.app-id:}") String appId,
                        @Value("${sparrowshell.qwen.model:qwen3.8-max}") String model,
                        ObjectMapper mapper) {
        this(apiKey, appId, model, mapper, new DashScopeQwenClient());
    }

    QwenProvider(String apiKey, String appId, String model, ObjectMapper mapper, QwenClient client) {
        this.apiKey = apiKey == null ? "" : apiKey.trim();
        this.appId = appId == null ? "" : appId.trim();
        this.model = model;
        this.mapper = mapper;
        this.client = client;
        this.delegate = provider(new ApplicationTransport(null));
    }

    public ProposalResponse propose(ProposalRequest request) { return delegate.propose(request); }
    public AnalysisResponse analyze(AnalysisRequest request) { return delegate.analyze(request); }

    public AgentTurnResponse agentTurn(AgentTurnRequest request) {
        String requestedSession = request == null ? null : request.sessionId;
        validateSession(requestedSession);
        ApplicationTransport transport = new ApplicationTransport(requestedSession);
        AgentTurnResponse response = provider(transport).agentTurn(request);
        String returnedSession = transport.result == null ? null : transport.result.sessionId;
        if (!validSession(returnedSession) || (requestedSession != null && !requestedSession.equals(returnedSession)))
            throw new ProviderException("invalid_provider_response", "通义千问会话标识无效");
        response.routeId = AGENT_ROUTE;
        response.provider = providerName();
        response.sessionId = returnedSession;
        return response;
    }

    public boolean isConfigured() { return !apiKey.isEmpty() && !appId.isEmpty(); }
    public String providerName() { return "qwen"; }
    public String proposalRouteId() { return PROPOSAL_ROUTE; }
    public String analysisRouteId() { return ANALYSIS_ROUTE; }

    private DeepSeekProvider provider(ApplicationTransport transport) {
        return new DeepSeekProvider(apiKey, model, mapper, transport, ENDPOINT,
                providerName(), PROPOSAL_ROUTE, ANALYSIS_ROUTE);
    }

    private boolean validSession(String sessionId) {
        return sessionId != null && !sessionId.trim().isEmpty() && sessionId.equals(sessionId.trim())
                && sessionId.getBytes(StandardCharsets.UTF_8).length <= 256 && sessionId.indexOf('\0') < 0;
    }

    private void validateSession(String sessionId) {
        if (sessionId != null && !validSession(sessionId))
            throw new ProviderException("invalid_request", "通义千问会话标识格式无效");
    }

    private final class ApplicationTransport implements DeepSeekProvider.HttpTransport {
        private final String sessionId;
        private QwenClient.Result result;

        private ApplicationTransport(String sessionId) { this.sessionId = sessionId; }

        public String post(String url, String bearerToken, String body) {
            try {
                JsonNode payload = mapper.readTree(body);
                StringBuilder prompt = new StringBuilder();
                for (JsonNode message : payload.path("messages")) {
                    prompt.append(message.path("role").asText()).append(":\n")
                            .append(message.path("content").asText()).append("\n\n");
                }
                if (prompt.length() == 0) throw new ProviderException("invalid_request", "通义千问提示词不能为空");
                result = client.call(apiKey, appId, model, prompt.toString(), sessionId);
                ObjectNode envelope = mapper.createObjectNode();
                envelope.putArray("choices").addObject().putObject("message")
                        .put("role", "assistant").put("content", result.text == null ? "" : result.text);
                ObjectNode usage = envelope.putObject("usage");
                usage.put("prompt_tokens", result.inputTokens);
                usage.put("completion_tokens", result.outputTokens);
                return mapper.writeValueAsString(envelope);
            } catch (ProviderException exception) {
                throw exception;
            } catch (Exception exception) {
                throw new ProviderException("invalid_provider_response", "通义千问响应格式无效");
            }
        }
    }
}
