package com.sparrowshell.ai.platform.account;

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.jdbc.core.JdbcTemplate;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;

class SyncRequestMonitorTest {
    @Test void writesOnlyProtocolMetadataAndSurvivesStorageFailure() {
        JdbcTemplate jdbc = mock(JdbcTemplate.class);
        ObjectProvider<JdbcTemplate> provider = mock(ObjectProvider.class);
        when(provider.getIfAvailable()).thenReturn(jdbc);
        SyncRequestMonitor monitor = new SyncRequestMonitor(provider);
        AccountModels.DeviceSession session = new AccountModels.DeviceSession();
        session.accountId = "account-1"; session.deviceId = "device-1"; session.clientVersion = "0.1.0";
        monitor.success("request-1", session, "push", System.nanoTime(), 3L, 8L, 1, 2);
        verify(jdbc).update(startsWith("INSERT INTO app_sync_request"), eq("request-1"), eq("account-1"), eq("device-1"),
                eq("push"), eq("succeeded"), isNull(), eq(200), eq(3L), eq(8L), eq(1), eq(false), eq(2),
                anyLong(), eq("0.1.0"), any());
        when(jdbc.update(anyString(), any(Object[].class))).thenThrow(new RuntimeException("storage down"));
        monitor.failure("request-2", session, "pull", System.nanoTime(), "invalid_cursor", 400, 0);
    }
}
