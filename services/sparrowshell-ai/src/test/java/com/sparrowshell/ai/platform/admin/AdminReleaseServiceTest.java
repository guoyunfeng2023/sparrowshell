package com.sparrowshell.ai.platform.admin;

import com.sparrowshell.ai.platform.update.UpdateRelease;
import com.sparrowshell.ai.platform.update.UpdateService;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.transaction.PlatformTransactionManager;
import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;

class AdminReleaseServiceTest {
    private final AdminRepository repository = mock(AdminRepository.class);
    private final AdminAuthService auth = mock(AdminAuthService.class);
    private final UpdateService updates = mock(UpdateService.class);
    private final ObjectProvider<PlatformTransactionManager> provider = mock(ObjectProvider.class);
    private final AdminModels.AdminSession session = new AdminModels.AdminSession();
    private AdminReleaseService service;

    @BeforeEach void setUp() {
        when(provider.getIfAvailable()).thenReturn(null);
        AdminService operations = new AdminService(repository, auth, provider);
        service = new AdminReleaseService(updates, auth, operations);
        session.adminId = "admin-1"; session.email = "admin@example.com"; session.role = "SUPER_ADMIN";
    }

    @Test void releaseTransitionRequiresReasonAndConfirmationButNotMfa() {
        UpdateRelease published = new UpdateRelease(); published.id = "release-1"; published.state = "stable";
        when(updates.publish("release-1")).thenReturn(published);
        AdminService.DangerousRequest request = request();
        request.mfaCode = null;

        AdminService.ActionResult result = service.transition(session, "release-1", "stable", null, request);

        assertSame(published, result.release);
        verify(auth, never()).requireMfa(any(), any());
        verify(updates).publish("release-1");
        ArgumentCaptor<AdminModels.Audit> audit = ArgumentCaptor.forClass(AdminModels.Audit.class);
        verify(repository).insertAudit(audit.capture());
        assertEquals("release.stable", audit.getValue().action);
        assertEquals("确认全量发布", audit.getValue().reason);
        assertEquals("success", audit.getValue().result);
    }

    @Test void releaseTransitionStillRejectsMissingReason() {
        AdminService.DangerousRequest request = request();
        request.reason = "  ";

        AdminException error = assertThrows(AdminException.class,
                () -> service.transition(session, "release-1", "stable", null, request));

        assertEquals("invalid_operation", error.code);
        verify(updates, never()).publish(any());
        verify(repository, never()).insertAudit(any());
    }

    private AdminService.DangerousRequest request() {
        AdminService.DangerousRequest request = new AdminService.DangerousRequest();
        request.reason = "确认全量发布"; request.operationId = "operation-release-001"; return request;
    }
}
