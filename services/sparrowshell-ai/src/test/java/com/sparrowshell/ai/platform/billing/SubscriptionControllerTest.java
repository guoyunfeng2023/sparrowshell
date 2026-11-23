package com.sparrowshell.ai.platform.billing;

import com.sparrowshell.ai.AiController;
import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.setup.MockMvcBuilders;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import static org.mockito.Mockito.*;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.*;

class SubscriptionControllerTest {
    private static final String DEVICE = "0be0dd37-9379-4663-a94f-799f16ab7678";

    @Test void subscriptionEndpointUsesDeviceIdentityAndToken() throws Exception {
        BillingService billing = mock(BillingService.class); AiTaskQuotaService quotas = mock(AiTaskQuotaService.class);
        Map<String, Object> response = new LinkedHashMap<String, Object>(); response.put("plan", "personal"); response.put("autoRenew", false);
        response.put("current", Collections.singletonMap("status", "active")); response.put("upcoming", Collections.emptyList());
        when(billing.subscription(DEVICE, "device-token")).thenReturn(response);
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new BillingController(billing, quotas)).build();
        mvc.perform(get("/client-api/v1/subscription").header(AiController.ANONYMOUS_ID_HEADER, DEVICE).header(AiController.DEVICE_TOKEN_HEADER, "device-token"))
                .andExpect(status().isOk()).andExpect(jsonPath("$.plan").value("personal"))
                .andExpect(jsonPath("$.current.status").value("active"))
                .andExpect(jsonPath("$.autoRenew").value(false)).andExpect(jsonPath("$.upcoming").isArray());
    }
}
