package com.sparrowshell.ai.platform.admin;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.jdbc.core.JdbcTemplate;
import java.util.Map;
import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;

class AdminOperationsServiceTest {
    private final JdbcTemplate jdbc = mock(JdbcTemplate.class);
    private final AdminAuthService auth = mock(AdminAuthService.class);
    private AdminOperationsService service;
    private final AdminModels.AdminSession session = new AdminModels.AdminSession();

    @BeforeEach void setUp() {
        ObjectProvider<JdbcTemplate> provider = mock(ObjectProvider.class);
        when(provider.getIfAvailable()).thenReturn(jdbc);
        service = new AdminOperationsService(provider, auth, false, "");
        session.role = "OPS_ADMIN";
    }

    @Test void syncListUsesDomainRoleAndNeverSelectsPayload() {
        Map<String, Object> row = new java.util.LinkedHashMap<String, Object>();
        row.put("requestId", "request-1"); row.put("createdAt", java.time.LocalDateTime.of(2026, 10, 30, 1, 2, 3));
        when(jdbc.queryForList(anyString(), eq("account-1"), eq("failed"), eq("invalid_cursor"), eq(20), eq(0))).thenReturn(java.util.Collections.singletonList(row));
        Map<String, Object> result = service.syncRequests(session, "account-1", "failed", "invalid_cursor", 20, 0);
        assertEquals(true, result.get("payloadExcluded"));
        assertEquals("2026-10-30T01:02:03Z", ((java.util.List<Map<String, Object>>) result.get("items")).get(0).get("createdAt"));
        verify(auth).requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER", "OPS_ADMIN");
        org.mockito.ArgumentCaptor<String> sql = org.mockito.ArgumentCaptor.forClass(String.class);
        verify(jdbc).queryForList(sql.capture(), eq("account-1"), eq("failed"), eq("invalid_cursor"), eq(20), eq(0));
        assertTrue(sql.getValue().contains("request_id"));
        assertFalse(sql.getValue().contains("payload"));
    }

    @Test void aiAdminCannotReadSystemJobs() {
        session.role = "AI_ADMIN";
        doThrow(new AdminException("permission_denied", "denied")).when(auth).requireRole(eq(session), any());
        AdminException error = assertThrows(AdminException.class, () -> service.jobs(session, null, 20, 0));
        assertEquals("permission_denied", error.code);
        verify(jdbc, never()).queryForList(anyString(), any(Object[].class));
    }

    @Test void rejectsInvalidPaginationBeforeQuery() {
        assertEquals("invalid_pagination", assertThrows(AdminException.class,
                () -> service.syncRequests(session, null, null, null, 0, 0)).code);
        verify(jdbc, never()).queryForList(anyString(), any(Object[].class));
    }
}
