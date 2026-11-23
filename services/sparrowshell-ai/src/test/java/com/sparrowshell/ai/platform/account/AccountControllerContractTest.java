package com.sparrowshell.ai.platform.account;

import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;
import org.springframework.http.converter.json.MappingJackson2HttpMessageConverter;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.setup.MockMvcBuilders;
import java.util.Collections;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

class AccountControllerContractTest {
    @Test void syncContractAcceptsAndEmitsRustSnakeCaseFields() throws Exception {
        AccountService service = mock(AccountService.class);
        AccountModels.DeviceSession session = new AccountModels.DeviceSession(); session.accountId = java.util.UUID.randomUUID().toString(); session.deviceId = java.util.UUID.randomUUID().toString();
        AccountModels.SyncPushResult result = new AccountModels.SyncPushResult(); result.mutationId = java.util.UUID.randomUUID().toString(); result.accepted = true; result.revision = 1L; result.changeSeq = 1L;
        when(service.authenticate(any())).thenReturn(session); when(service.push(any(), any(), any())).thenReturn(Collections.singletonList(result));
        ObjectMapper json = new ObjectMapper().configure(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES, true);
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new AccountController(service, mock(SyncRequestMonitor.class))).setMessageConverters(new MappingJackson2HttpMessageConverter(json)).build();
        String body = "{\"device_id\":\"" + session.deviceId + "\",\"changes\":[{\"mutation_id\":\"" + result.mutationId + "\",\"entity_type\":\"server\",\"entity_id\":\"" + java.util.UUID.randomUUID() + "\",\"base_revision\":0,\"schema_version\":1,\"deleted\":false,\"data\":{\"name\":\"test\"}}]}";
        mvc.perform(post("/client-api/v1/sync/push").header("Authorization", "Bearer test").contentType("application/json").content(body))
                .andExpect(status().isOk()).andExpect(jsonPath("$[0].mutation_id").value(result.mutationId))
                .andExpect(jsonPath("$[0].accepted").value(true)).andExpect(jsonPath("$[0].revision").value(1))
                .andExpect(jsonPath("$[0].change_seq").value(1));
    }

    @Test void verificationCodeContractPassesLocale() throws Exception {
        AccountService service = mock(AccountService.class);
        ObjectMapper json = new ObjectMapper().configure(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES, true);
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new AccountController(service, mock(SyncRequestMonitor.class))).setMessageConverters(new MappingJackson2HttpMessageConverter(json)).build();
        mvc.perform(post("/client-api/v1/auth/verification-code").contentType("application/json")
                        .content("{\"email\":\"person@example.test\",\"purpose\":\"register\",\"locale\":\"en-US\"}"))
                .andExpect(status().isOk()).andExpect(jsonPath("$.status").value("sent"));
        verify(service).sendVerificationCode("person@example.test", "register", "en-US");
    }
}
