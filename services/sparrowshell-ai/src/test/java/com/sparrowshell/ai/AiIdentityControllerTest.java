package com.sparrowshell.ai;

import com.sparrowshell.ai.platform.account.AccountException;
import com.sparrowshell.ai.platform.account.AccountService;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.setup.MockMvcBuilders;
import static org.mockito.Mockito.*;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.*;

class AiIdentityControllerTest {
    @Test void invalidBearerReturns401AndCannotFallbackToAnonymous() throws Exception {
        AccountService accounts = mock(AccountService.class);
        when(accounts.authenticateAccountId("Bearer expired")).thenThrow(mock(AccountException.class));
        AiUsageService usage = mock(AiUsageService.class);
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new AiController(mock(ProviderRouter.class), null,
                usage, new AiIdentityService(accounts))).build();

        mvc.perform(post("/v1/ai/agent/turn").contentType(MediaType.APPLICATION_JSON).content("{}")
                        .header("Authorization", "Bearer expired")
                        .header(AiController.ANONYMOUS_ID_HEADER, "d2d5bafa-76ce-4a72-9db7-a82f527dd17b"))
                .andExpect(status().isUnauthorized()).andExpect(jsonPath("$.code").value("unauthorized"));
        verifyNoInteractions(usage);
    }

    @Test void malformedAnonymousIdReturns400BeforeProviderOrMetering() throws Exception {
        AiUsageService usage = mock(AiUsageService.class);
        ProviderRouter router = mock(ProviderRouter.class);
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new AiController(router, null,
                usage, new AiIdentityService(mock(AccountService.class)))).build();

        mvc.perform(post("/v1/ai/agent/turn").contentType(MediaType.APPLICATION_JSON).content("{}")
                        .header(AiController.ANONYMOUS_ID_HEADER, "not-a-uuid"))
                .andExpect(status().isBadRequest()).andExpect(jsonPath("$.code").value("invalid_identity"));
        verifyNoInteractions(usage, router);
    }
}
