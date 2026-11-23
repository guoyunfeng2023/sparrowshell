package com.sparrowshell.ai.platform.admin;

import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.setup.MockMvcBuilders;
import java.math.BigDecimal;
import java.time.Instant;
import java.util.Collections;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.containsString;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.*;

class AdminBillingControllerTest {
    private final AdminAuthService auth = mock(AdminAuthService.class);
    private final AdminService service = mock(AdminService.class);
    private final AdminModels.AdminSession session = new AdminModels.AdminSession();

    @Test void orderResponseContainsOperationalFieldsWithoutPaymentSecrets() throws Exception {
        AdminModels.BillingOrder order = new AdminModels.BillingOrder();
        order.id = "order-1"; order.plan = "personal"; order.amount = new BigDecimal("9.90"); order.status = "paid";
        order.licenseId = "license-1"; order.licenseBoundDeviceId = "device-1";
        order.alipayTradeNo = "trade-1"; order.createdAt = Instant.parse("2026-11-10T07:00:00Z");
        when(service.billingOrders(eq(session), isNull(), eq("paid"), eq(100), eq(0))).thenReturn(Collections.singletonList(order));
        when(service.billingOrderCount(eq(session), isNull(), eq("paid"))).thenReturn(1L);
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new AdminController(auth, service)).build();
        mvc.perform(get("/v1/admin/billing/orders").param("status", "paid").requestAttr(AdminAuthFilter.SESSION_ATTRIBUTE, session))
                .andExpect(status().isOk()).andExpect(jsonPath("$.total").value(1))
                .andExpect(jsonPath("$.items[0].id").value("order-1"))
                .andExpect(jsonPath("$.items[0].status").value("paid"))
                .andExpect(jsonPath("$.items[0].licenseBound").value(true))
                .andExpect(jsonPath("$.items[0].alipayTradeNo").value("trade-1"))
                .andExpect(content().string(not(containsString("licenseBoundDeviceId"))))
                .andExpect(content().string(not(containsString("orderToken"))))
                .andExpect(content().string(not(containsString("qrCode"))))
                .andExpect(content().string(not(containsString("licenseKey"))));
    }

    @Test void licenseResponseContainsCurrentPeriodWithoutLicenseOrDeviceSecrets() throws Exception {
        AdminModels.BillingLicense license = new AdminModels.BillingLicense();
        license.id = "license-1"; license.status = "active"; license.boundDeviceId = "device-1";
        license.activatedAt = Instant.parse("2026-11-10T06:00:00Z");
        license.plan = "high"; license.quota = 1200; license.used = 25;
        license.periodStart = Instant.parse("2026-11-10T07:00:00Z"); license.periodEnd = Instant.parse("2026-12-10T07:00:00Z");
        when(service.billingLicenses(eq(session), isNull(), eq("bound"), eq(100), eq(0))).thenReturn(Collections.singletonList(license));
        when(service.billingLicenseCount(eq(session), isNull(), eq("bound"))).thenReturn(1L);
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new AdminController(auth, service)).build();
        mvc.perform(get("/v1/admin/billing/licenses").param("status", "bound").requestAttr(AdminAuthFilter.SESSION_ATTRIBUTE, session))
                .andExpect(status().isOk()).andExpect(jsonPath("$.items[0].bound").value(true))
                .andExpect(jsonPath("$.items[0].activated").value(true))
                .andExpect(jsonPath("$.items[0].activatedAt").value("2026-11-10T06:00:00Z"))
                .andExpect(jsonPath("$.items[0].plan").value("high"))
                .andExpect(jsonPath("$.items[0].remaining").value(1175))
                .andExpect(jsonPath("$.items[0].periodEnd").value("2026-12-10T07:00:00Z"))
                .andExpect(content().string(not(containsString("keyHash"))))
                .andExpect(content().string(not(containsString("keyCiphertext"))))
                .andExpect(content().string(not(containsString("deviceTokenHash"))))
                .andExpect(content().string(not(containsString("licenseKey"))));
    }

    @Test void invalidPaginationIsRejected() throws Exception {
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new AdminController(auth, service)).build();
        mvc.perform(get("/v1/admin/billing/orders").param("limit", "101").requestAttr(AdminAuthFilter.SESSION_ATTRIBUTE, session))
                .andExpect(status().isBadRequest()).andExpect(jsonPath("$.error.code").value("invalid_pagination"));
        verify(service, never()).billingOrders(any(), any(), any(), anyInt(), anyInt());
    }
}
