package com.sparrowshell.ai;

import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;
import java.util.Collections;
import static org.junit.jupiter.api.Assertions.*;

class QwenProviderTest {
    private static final String PROPOSAL_JSON = "{\"summary\":\"检查内存\",\"commands\":[{\"command\":\"free -m\",\"riskHint\":\"low\",\"requiresInteraction\":false}]}";

    @Test void usesApplicationSdkContractAndServerSideConfiguration() {
        RecordingClient client = new RecordingClient(new QwenClient.Result(PROPOSAL_JSON, "session-1", 20, 10));
        QwenProvider provider = new QwenProvider("qwen-server-secret", "test-app-id", "qwen3.8-max", mapper(), client);
        ProposalResponse response = provider.propose(request());

        assertEquals("qwen", response.provider);
        assertEquals("qwen3.8-max", response.model);
        assertEquals(QwenProvider.PROPOSAL_ROUTE, response.routeId);
        assertEquals("free -m", response.commands.get(0).command);
        assertEquals("test-app-id", client.appId);
        assertEquals("qwen3.8-max", client.modelId);
        assertTrue(client.prompt.contains("检查内存"));
        assertTrue(client.prompt.contains("system:"));
        assertFalse(client.prompt.contains("qwen-server-secret"));
    }

    @Test void returnsAndReusesQwenAgentSession() {
        String decision = "{\"type\":\"tool_call\",\"toolCall\":{\"summary\":\"检查 Docker\",\"command\":\"command -v docker\"}}";
        RecordingClient client = new RecordingClient(new QwenClient.Result(decision, "session-1", 20, 10));
        QwenProvider provider = new QwenProvider("secret", "app", "qwen3.8-max", mapper(), client);
        AgentTurnRequest request = agentRequest();

        AgentTurnResponse first = provider.agentTurn(request);
        assertNull(client.sessionId);
        assertEquals("session-1", first.sessionId);
        assertEquals(QwenProvider.AGENT_ROUTE, first.routeId);

        request.sessionId = "session-1";
        AgentTurnResponse second = provider.agentTurn(request);
        assertEquals("session-1", client.sessionId);
        assertEquals("session-1", second.sessionId);
        assertTrue(client.prompt.contains("之前的问题"));
    }

    @Test void rejectsMissingChangedAndInvalidSessions() {
        String decision = "{\"type\":\"final_answer\",\"finalAnswer\":{\"answer\":\"完成\"}}";
        for (String returned : new String[]{null, "", "session-2"}) {
            RecordingClient client = new RecordingClient(new QwenClient.Result(decision, returned, 1, 1));
            QwenProvider provider = new QwenProvider("secret", "app", "qwen3.8-max", mapper(), client);
            AgentTurnRequest request = agentRequest();
            request.sessionId = "session-1";
            ProviderException error = assertThrows(ProviderException.class, () -> provider.agentTurn(request));
            assertEquals("invalid_provider_response", error.code());
        }
    }

    @Test void rejectsInvalidStructuredOutputWithoutLeakingSecrets() {
        RecordingClient client = new RecordingClient(new QwenClient.Result(
                "{\"summary\":\"x\",\"commands\":[],\"target\":\"another-server\"}", "session-1", 1, 1));
        ProviderException error = assertThrows(ProviderException.class,
                () -> new QwenProvider("qwen-server-secret", "app", "qwen3.8-max", mapper(), client).propose(request()));
        assertEquals("invalid_provider_response", error.code());
        assertFalse(error.getMessage().contains("qwen-server-secret"));
    }

    static QwenProvider provider(String content) {
        return new QwenProvider("test-key", "test-app", "qwen3.8-max", new ObjectMapper(),
                new RecordingClient(new QwenClient.Result(content, "session-1", 10, 5)));
    }

    static QwenProvider empty() {
        return new QwenProvider("", "", "qwen3.8-max", new ObjectMapper(),
                new RecordingClient(new QwenClient.Result("{}", "session-1", 0, 0)));
    }

    private static ObjectMapper mapper() {
        return new ObjectMapper().enable(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES);
    }

    private ProposalRequest request() {
        ProposalRequest request = new ProposalRequest();
        request.intent = "检查内存";
        request.locale = "zh-CN";
        request.context = Collections.emptyMap();
        request.limits = new ProposalRequest.Limits();
        request.limits.maxProposals = 1;
        request.routeId = QwenProvider.PROPOSAL_ROUTE;
        return request;
    }

    private AgentTurnRequest agentRequest() {
        AgentTurnRequest request = new AgentTurnRequest();
        request.intent = "检查 Docker";
        request.locale = "zh-CN";
        request.context = Collections.emptyMap();
        AgentTurnRequest.Message previous = new AgentTurnRequest.Message();
        previous.role = "user";
        previous.content = "之前的问题";
        request.history = Collections.singletonList(previous);
        return request;
    }

    static class RecordingClient implements QwenClient {
        private final QwenClient.Result result;
        String appId;
        String modelId;
        String prompt;
        String sessionId;

        RecordingClient(QwenClient.Result result) { this.result = result; }

        public QwenClient.Result call(String apiKey, String appId, String modelId, String prompt, String sessionId) {
            this.appId = appId;
            this.modelId = modelId;
            this.prompt = prompt;
            this.sessionId = sessionId;
            return result;
        }
    }
}
