package com.sparrowshell.ai;

import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.http.converter.json.MappingJackson2HttpMessageConverter;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.MvcResult;
import org.springframework.test.web.servlet.setup.MockMvcBuilders;
import java.util.Collections;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.not;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.asyncDispatch;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.*;

class AiStreamTest {
    private AiStreamService streamService;

    @AfterEach void shutdown() { if (streamService != null) streamService.close(); }

    @Test void emitsOnlyFullyValidatedProposalInFinalEvent() throws Exception {
        MockMvc mvc = mvc(envelope("{\"summary\":\"检查磁盘\",\"commands\":[{\"command\":\"df -h\",\"riskHint\":\"low\",\"requiresInteraction\":false}]}"));
        MvcResult result = mvc.perform(post("/v1/ai/proposals/stream").contentType(MediaType.APPLICATION_JSON)
                        .content(requestJson()))
                .andExpect(request().asyncStarted()).andReturn();
        mvc.perform(asyncDispatch(result)).andExpect(status().isOk())
                .andExpect(content().string(containsString("event:meta")))
                .andExpect(content().string(containsString("event:final")))
                .andExpect(content().string(containsString("df -h")))
                .andExpect(content().string(containsString("deepseek.chat.completions")));
    }

    @Test void truncatedProviderJsonProducesErrorAndNeverFinalCommand() throws Exception {
        MockMvc mvc = mvc(envelope("{\"summary\":\"x\",\"commands\":["));
        MvcResult result = mvc.perform(post("/v1/ai/proposals/stream").contentType(MediaType.APPLICATION_JSON)
                        .content(requestJson()))
                .andExpect(request().asyncStarted()).andReturn();
        mvc.perform(asyncDispatch(result)).andExpect(status().isOk())
                .andExpect(content().string(containsString("event:error")))
                .andExpect(content().string(containsString("invalid_provider_response")))
                .andExpect(content().string(not(containsString("event:final"))))
                .andExpect(content().string(not(containsString("df -h"))));
    }

    private MockMvc mvc(String providerBody) {
        DeepSeekProvider deepSeek = new DeepSeekProvider("server-only-secret", "deepseek-chat", new ObjectMapper(),
                new DeepSeekProviderTest.RecordingTransport(200, providerBody));
        QwenProvider qwen = QwenProviderTest.empty();
        ProviderRouter router = new ProviderRouter(deepSeek, qwen);
        streamService = new AiStreamService(router);
        ObjectMapper strict = new ObjectMapper().enable(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES);
        return MockMvcBuilders.standaloneSetup(new AiController(router, streamService))
                .setMessageConverters(new MappingJackson2HttpMessageConverter(strict)).build();
    }

    private String requestJson() {
        return "{\"clientRequestId\":\"d2d5bafa-76ce-4a72-9db7-a82f527dd17b\",\"routeId\":\"deepseek.chat.completions\",\"intent\":\"检查磁盘\",\"locale\":\"zh-CN\",\"context\":{},\"limits\":{\"maxProposals\":1}}";
    }

    private static String envelope(String content) {
        try {
            return new ObjectMapper().writeValueAsString(Collections.singletonMap("choices", Collections.singletonList(
                    Collections.singletonMap("message", Collections.singletonMap("content", content)))));
        } catch (Exception exception) { throw new IllegalStateException(exception); }
    }
}
