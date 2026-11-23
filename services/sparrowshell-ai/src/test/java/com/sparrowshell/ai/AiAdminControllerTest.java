package com.sparrowshell.ai;

import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.setup.MockMvcBuilders;
import java.math.BigDecimal;
import java.time.Instant;
import com.sparrowshell.ai.platform.admin.AdminAccessGuard;
import static org.mockito.Mockito.mock;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.containsString;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.*;

class AiAdminControllerTest {
    @Test void returnsOnlyRealAggregatesAndPreservesUnknownUsage() throws Exception {
        AiUsageServiceTest.FakeRepository repository = new AiUsageServiceTest.FakeRepository();
        AiUsage row = new AiUsage();
        row.accountId = "private-account";
        row.clientRequestId = "5c4e6dca-e41b-4a08-a9af-c256cb67c676";
        row.requestFingerprint = "private-fingerprint";
        row.routeId = "deepseek.chat.completions";
        row.operation = "proposal";
        row.status = "succeeded";
        row.usageStatus = "unknown";
        row.priceVersion = "prices-v1";
        row.estimatedCost = new BigDecimal("0.05");
        row.createdAt = Instant.now();
        repository.only = row;
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new AiAdminController(repository, mock(AdminAccessGuard.class))).build();
        mvc.perform(get("/v1/admin/ai/overview").param("range", "today"))
                .andExpect(status().isOk()).andExpect(jsonPath("$.ai.status").value("available"))
                .andExpect(jsonPath("$.ai.requests").value(1))
                .andExpect(jsonPath("$.ai.costStatus").value("partial"))
                .andExpect(jsonPath("$.ai.unknownUsage").value(1))
                .andExpect(jsonPath("$.ai.priceVersions[0]").value("prices-v1"))
                .andExpect(jsonPath("$.users.status").value("unavailable"))
                .andExpect(jsonPath("$.sync.status").value("unavailable"))
                .andExpect(content().string(not(containsString("private-account"))))
                .andExpect(content().string(not(containsString("private-fingerprint"))));
    }

    @Test void missingLedgerIsUnavailableRatherThanZero() throws Exception {
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new AiAdminController((AiUsageLedgerRepository) null, mock(AdminAccessGuard.class))).build();
        mvc.perform(get("/v1/admin/ai/overview")).andExpect(status().isOk())
                .andExpect(jsonPath("$.status").value("unavailable"))
                .andExpect(jsonPath("$.ai.reason").value("usage_ledger_not_configured"))
                .andExpect(jsonPath("$.ai.requests").doesNotExist());
    }

    @Test void rejectsUnsupportedTimeRange() throws Exception {
        MockMvc mvc = MockMvcBuilders.standaloneSetup(new AiAdminController((AiUsageLedgerRepository) null, mock(AdminAccessGuard.class))).build();
        mvc.perform(get("/v1/admin/ai/overview").param("range", "forever"))
                .andExpect(status().isBadRequest());
    }
}
