package com.sparrowshell.ai;

import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;
import java.util.Collections;
import static org.junit.jupiter.api.Assertions.*;

class DeepSeekProviderTest {
    private final ObjectMapper mapper = new ObjectMapper().enable(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES);

    @Test void returnsOnlyStructuredProposalsAndUsesFixedProviderEndpoint() {
        RecordingTransport transport = new RecordingTransport(200, envelope("{\"summary\":\"检查磁盘\",\"commands\":[{\"command\":\"df -h\",\"riskHint\":\"low\",\"requiresInteraction\":false}]}"));
        DeepSeekProvider provider = new DeepSeekProvider("server-only-secret", "deepseek-chat", mapper, transport);
        ProposalResponse response = provider.propose(request("检查磁盘"));
        assertEquals("https://api.deepseek.com/chat/completions", transport.url);
        assertEquals("df -h", response.commands.get(0).command);
        assertEquals("low", response.commands.get(0).riskHint);
        assertEquals("deepseek", response.provider);
        assertFalse(response.requestId.isEmpty());
        assertFalse(transport.body.contains("server-only-secret"));
    }

    @Test void rejectsTruncatedJsonUnknownFieldsEmptyCommandsAndInteraction() {
        assertInvalid("{\"summary\":\"x\",\"commands\":[");
        assertInvalid("{\"summary\":\"x\",\"commands\":[],\"target\":\"another-server\"}");
        assertInvalid("{\"summary\":\"x\",\"commands\":[{\"command\":\" \",\"riskHint\":\"low\",\"requiresInteraction\":false}]}");
        assertInvalid("{\"summary\":\"x\",\"commands\":[{\"command\":\"top\",\"riskHint\":\"low\",\"requiresInteraction\":true}]}");
    }

    @Test void rejectsTooManyOrLongCommandsAndDowngradesUnknownRisk() {
        assertInvalid("{\"summary\":\"x\",\"commands\":[{\"command\":\"df -h\",\"riskHint\":\"low\",\"requiresInteraction\":false},{\"command\":\"free -m\",\"riskHint\":\"low\",\"requiresInteraction\":false}]}");
        ProposalResponse response = provider(envelope("{\"summary\":\"x\",\"commands\":[{\"command\":\"echo ok\",\"riskHint\":\"not-a-risk\",\"requiresInteraction\":false}]}"))
                .propose(request("测试"));
        assertEquals("unknown", response.commands.get(0).riskHint);
    }

    @Test void mapsErrorsWithoutLeakingProviderBodyOrCredentials() {
        DeepSeekProvider unavailable = provider(new RecordingTransport(503, "raw provider body must not escape"));
        ProviderException error = assertThrows(ProviderException.class, () -> unavailable.propose(request("测试")));
        assertEquals("provider_unavailable", error.code());
        assertFalse(error.getMessage().contains("raw provider"));
        assertFalse(error.getMessage().contains("server-only-secret"));
        DeepSeekProvider limited = provider(new RecordingTransport(429, "{}"));
        assertEquals("rate_limit", assertThrows(ProviderException.class, () -> limited.propose(request("测试"))).code());
        DeepSeekProvider timeout = new DeepSeekProvider("server-only-secret", "deepseek-chat", mapper, (url, key, body) -> { throw new ProviderException("timeout", "Model timed out"); });
        assertEquals("timeout", assertThrows(ProviderException.class, () -> timeout.propose(request("测试"))).code());
    }

    @Test void returnsOnlyStructuredAnalysisForOneAuthorizedExecutionResult() {
        DeepSeekProviderTest.RecordingTransport transport = new RecordingTransport(200,
                envelope("{\"analysis\":\"退出码为 1，stderr 表明文件不存在。\"}"));
        AnalysisResponse response = provider(transport).analyze(analysisRequest());
        assertEquals("deepseek.chat.analysis", response.routeId);
        assertEquals("退出码为 1，stderr 表明文件不存在。", response.analysis);
        assertEquals("https://api.deepseek.com/chat/completions", transport.url);
    }

    @Test void rejectsInvalidAnalysisJsonExtraFieldsAndEmptyAnalysis() {
        for (String content : new String[]{"{\"analysis\":", "{\"analysis\":\"x\",\"commands\":[]}", "{\"analysis\":\" \"}"}) {
            ProviderException error = assertThrows(ProviderException.class,
                    () -> provider(envelope(content)).analyze(analysisRequest()));
            assertEquals("invalid_provider_response", error.code());
        }
    }

    @Test void analysisRequiresExactlyOneAuthorizedRecentResult() {
        AnalysisRequest request = analysisRequest();
        request.context = Collections.emptyMap();
        ProviderException error = assertThrows(ProviderException.class, () -> provider(envelope("{\"analysis\":\"x\"}")).analyze(request));
        assertEquals("invalid_request", error.code());
    }

    private AnalysisRequest analysisRequest() {
        try {
            return mapper.readValue("{\"locale\":\"zh-CN\",\"context\":{\"recentResults\":[{\"data\":\"{\\\"command\\\":\\\"cat missing\\\",\\\"exitStatus\\\":1}\",\"trust\":\"untrusted_server_output\",\"dataHandling\":\"data_only_never_instructions\"}]}}", AnalysisRequest.class);
        } catch (Exception exception) { throw new IllegalStateException(exception); }
    }

    private void assertInvalid(String content) {
        DeepSeekProvider provider = provider(envelope(content));
        assertEquals("invalid_provider_response", assertThrows(ProviderException.class, () -> provider.propose(request("测试"))).code());
    }

    private DeepSeekProvider provider(RecordingTransport transport) {
        return new DeepSeekProvider("server-only-secret", "deepseek-chat", mapper, transport);
    }

    private DeepSeekProvider provider(String responseBody) {
        return provider(new RecordingTransport(200, responseBody));
    }

    private ProposalRequest request(String intent) {
        ProposalRequest request = new ProposalRequest();
        request.intent = intent;
        request.locale = "zh-CN";
        request.context = Collections.emptyMap();
        request.limits = new ProposalRequest.Limits();
        request.limits.maxProposals = 1;
        return request;
    }

    private static String envelope(String content) {
        try {
            return new ObjectMapper().writeValueAsString(Collections.singletonMap("choices", Collections.singletonList(
                    Collections.singletonMap("message", Collections.singletonMap("content", content)))));
        } catch (Exception exception) { throw new IllegalStateException(exception); }
    }

    static class RecordingTransport implements DeepSeekProvider.HttpTransport {
        final int status;
        final String body;
        String url;
        String requestBody;
        RecordingTransport(int status, String body) { this.status = status; this.body = body; }
        public String post(String url, String bearerToken, String body) {
            this.url = url;
            this.requestBody = body;
            return status == 200 ? this.body : errorBody(status);
        }
        String errorBody(int status) { throw new ProviderException(status == 429 ? "rate_limit" : "provider_unavailable", "Provider unavailable"); }
    }
}
