package com.sparrowshell.ai.platform.admin;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.transaction.PlatformTransactionManager;
import java.util.Optional;
import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;

class AdminServiceTest {
    private final AdminRepository repository = mock(AdminRepository.class);
    private final AdminAuthService auth = mock(AdminAuthService.class);
    private final ObjectProvider<PlatformTransactionManager> provider = mock(ObjectProvider.class);
    private AdminService service;
    private final AdminModels.AdminSession session = new AdminModels.AdminSession();

    @BeforeEach void setUp() {
        when(provider.getIfAvailable()).thenReturn(null);
        service = new AdminService(repository, auth, provider);
        session.adminId = "admin-1"; session.email = "admin@example.com"; session.role = "SUPER_ADMIN";
    }

    @Test void disablingAccountWritesSuccessAuditAndRevision() {
        AdminModels.Account account = new AdminModels.Account(); account.id = "user-1"; account.status = "disabled"; account.revision = 2;
        when(repository.changeAccountStatus("user-1", "disabled", "安全复核", 1)).thenReturn(account);
        AdminService.ActionResult result = service.changeAccountStatus(session, "user-1", "disabled", 1, request());
        assertEquals(2, result.account.revision); assertFalse(result.replayed);
        ArgumentCaptor<AdminModels.Audit> audit = ArgumentCaptor.forClass(AdminModels.Audit.class);
        verify(repository).insertAudit(audit.capture());
        assertEquals("success", audit.getValue().result); assertEquals("account.disabled", audit.getValue().action);
        verify(auth).requireMfa(session, "123456");
    }

    @Test void revisionConflictIsAuditedAsFailure() {
        when(repository.changeAccountStatus(any(), any(), any(), anyLong())).thenThrow(AdminException.conflict());
        AdminException error = assertThrows(AdminException.class, () -> service.changeAccountStatus(session, "user-1", "disabled", 1, request()));
        assertEquals("revision_conflict", error.code);
        ArgumentCaptor<AdminModels.Audit> audit = ArgumentCaptor.forClass(AdminModels.Audit.class);
        verify(repository, times(2)).insertAudit(audit.capture());
        assertEquals("failed", audit.getAllValues().get(1).result);
        assertEquals("revision_conflict", audit.getAllValues().get(1).errorCode);
    }

    @Test void successfulOperationIdReplaysWithoutRepeatingBusinessChange() {
        AdminModels.Audit previous = new AdminModels.Audit(); previous.result = "success"; previous.targetId = "user-1"; previous.reason = "安全复核";
        when(repository.auditByOperation(session.adminId, "operation-001", "account.disabled")).thenReturn(Optional.of(previous));
        AdminService.ActionResult result = service.changeAccountStatus(session, "user-1", "disabled", 1, request());
        assertTrue(result.replayed);
        verify(repository, never()).changeAccountStatus(any(), any(), any(), anyLong());
        verify(repository, never()).insertAudit(any()); verify(auth, never()).requireMfa(any(), any());
    }

    @Test void viewerCannotDisableAccount() {
        doThrow(new AdminException("permission_denied", "denied")).when(auth).requireRole(eq(session), any());
        assertEquals("permission_denied", assertThrows(AdminException.class, () -> service.changeAccountStatus(session, "user-1", "disabled", 1, request())).code);
        verify(repository, never()).changeAccountStatus(any(), any(), any(), anyLong());
    }

    @Test void revokedDeviceIsPersisted() {
        AdminModels.Device device = new AdminModels.Device(); device.sessionId = "session-1"; device.status = "revoked";
        when(repository.revokeDevice("session-1", "valid")).thenReturn(device);
        assertEquals("revoked", service.revokeDevice(session, "session-1", request()).device.status);
        verify(repository).insertAudit(any(AdminModels.Audit.class));
    }

    private AdminService.DangerousRequest request() {
        AdminService.DangerousRequest request = new AdminService.DangerousRequest();
        request.reason = "安全复核"; request.mfaCode = "123456"; request.operationId = "operation-001"; return request;
    }
}
