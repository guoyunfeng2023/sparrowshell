package com.sparrowshell.ai.platform.admin;

import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.setup.MockMvcBuilders;
import java.time.Instant;
import java.util.Collections;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.not;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.*;

class AdminSubscriptionControllerTest {
    private final AdminAuthService auth = mock(AdminAuthService.class);
    private final AdminService service = mock(AdminService.class);
    private final AdminModels.AdminSession session = new AdminModels.AdminSession();

    @Test void subscriptionResponseContainsLifecycleWithoutBillingSecrets() throws Exception {
        AdminModels.BillingSubscription subscription = new AdminModels.BillingSubscription(); subscription.id = "period-1";
        subscription.licenseId = "license-1"; subscription.boundDeviceId = "device-1"; subscription.plan = "personal";
        subscription.quota = 200; subscription.used = 200; subscription.status = "active";
        subscription.periodStart = Instant.parse("2026-11-10T04:00:00Z"); subscription.periodEnd = Instant.parse("2026-12-10T04:00:00Z");
        when(service.billingSubscriptions(eq(session), isNull(), eq("active"), eq("personal"), eq(100), eq(0))).thenReturn(Collections.singletonList(subscription));
        when(service.billingSubscriptionCount(eq(session), isNull(), eq("active"), eq("personal"))).thenReturn(1L);
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new AdminController(auth, service)).build();
        mvc.perform(get("/v1/admin/billing/subscriptions").param("status", "active").param("plan", "personal").requestAttr(AdminAuthFilter.SESSION_ATTRIBUTE, session))
                .andExpect(status().isOk()).andExpect(jsonPath("$.total").value(1))
                .andExpect(jsonPath("$.items[0].id").value("period-1"))
                .andExpect(jsonPath("$.items[0].remaining").value(0))
                .andExpect(jsonPath("$.items[0].exhausted").value(true))
                .andExpect(jsonPath("$.items[0].autoRenew").value(false))
                .andExpect(content().string(not(containsString("licenseKey"))))
                .andExpect(content().string(not(containsString("keyCiphertext"))))
                .andExpect(content().string(not(containsString("deviceTokenHash"))));
    }

    @Test void summaryReturnsSubscriptionMetrics() throws Exception {
        AdminModels.BillingSubscriptionSummary summary = new AdminModels.BillingSubscriptionSummary();
        summary.total = 20; summary.active = 8; summary.scheduled = 3; summary.expired = 9; summary.expiring = 2; summary.exhausted = 1;
        summary.personalActive = 5; summary.highActive = 3;
        when(service.billingSubscriptionSummary(session)).thenReturn(summary);
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new AdminController(auth, service)).build();
        mvc.perform(get("/v1/admin/billing/subscriptions/summary").requestAttr(AdminAuthFilter.SESSION_ATTRIBUTE, session))
                .andExpect(status().isOk()).andExpect(jsonPath("$.total").value(20))
                .andExpect(jsonPath("$.active").value(8)).andExpect(jsonPath("$.scheduled").value(3))
                .andExpect(jsonPath("$.expiring").value(2)).andExpect(jsonPath("$.exhausted").value(1))
                .andExpect(jsonPath("$.personalActive").value(5)).andExpect(jsonPath("$.highActive").value(3));
    }
}
