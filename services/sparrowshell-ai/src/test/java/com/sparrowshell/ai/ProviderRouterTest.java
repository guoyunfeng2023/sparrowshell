package com.sparrowshell.ai;

import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;
import java.util.Collections;
import static org.junit.jupiter.api.Assertions.*;

class ProviderRouterTest {
    @Test void defaultsToQwenAndKeepsExplicitDeepSeekRouteAvailable() {
        DeepSeekProvider deepSeek = deepSeek();
        QwenProvider qwen = QwenProviderTest.provider(proposal("free -m"));
        ProviderRouter router = new ProviderRouter(deepSeek, qwen);

        ProposalResponse qwenResponse = router.propose(request(null));
        assertEquals("qwen", qwenResponse.provider);
        assertEquals(QwenProvider.PROPOSAL_ROUTE, qwenResponse.routeId);

        ProposalResponse deepSeekResponse = router.propose(request("deepseek.chat.completions"));
        assertEquals("deepseek", deepSeekResponse.provider);
        assertEquals("df -h", deepSeekResponse.commands.get(0).command);
    }

    @Test void defaultsAgentToQwenAndNeverFallsBackAcrossProviders() {
        String decision = "{\"type\":\"final_answer\",\"finalAnswer\":{\"answer\":\"通义回答\"}}";
        ProviderRouter router = new ProviderRouter(deepSeek(), QwenProviderTest.provider(decision));
        AgentTurnRequest request = new AgentTurnRequest();
        request.intent = "普通问题";
        request.context = Collections.emptyMap();
        AgentTurnResponse response = router.agentTurn(request);
        assertEquals("qwen", response.provider);
        assertEquals("session-1", response.sessionId);
        assertEquals("通义回答", response.finalAnswer.answer);

        request.routeId = "deepseek.chat.agent";
        ProviderException error = assertThrows(ProviderException.class, () -> router.agentTurn(request));
        assertEquals("route_unavailable", error.code());
    }

    @Test void rejectsUnknownRouteWithoutFallingBackToAnotherProvider() {
        ProviderException error = assertThrows(ProviderException.class,
                () -> new ProviderRouter(deepSeek(), QwenProviderTest.empty()).agentTurn(agentRequest("qwen.unknown")));
        assertEquals("route_unavailable", error.code());
    }

    private AgentTurnRequest agentRequest(String route) {
        AgentTurnRequest request = new AgentTurnRequest();
        request.routeId = route;
        request.intent = "检查";
        request.context = Collections.emptyMap();
        return request;
    }

    private DeepSeekProvider deepSeek() {
        String content = proposal("df -h");
        String body;
        try {
            body = new ObjectMapper().writeValueAsString(Collections.singletonMap("choices", Collections.singletonList(
                    Collections.singletonMap("message", Collections.singletonMap("content", content)))));
        } catch (Exception exception) { throw new IllegalStateException(exception); }
        return new DeepSeekProvider("deepseek-secret", "deepseek-chat", new ObjectMapper(),
                new DeepSeekProviderTest.RecordingTransport(200, body));
    }

    private ProposalRequest request(String routeId) {
        ProposalRequest request = new ProposalRequest();
        request.intent = "检查";
        request.locale = "zh-CN";
        request.context = Collections.emptyMap();
        request.limits = new ProposalRequest.Limits();
        request.limits.maxProposals = 1;
        request.routeId = routeId;
        return request;
    }

    private static String proposal(String command) {
        return "{\"summary\":\"检查\",\"commands\":[{\"command\":\"" + command + "\",\"riskHint\":\"low\",\"requiresInteraction\":false}]}";
    }
}
