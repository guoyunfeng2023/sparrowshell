package com.sparrowshell.ai.platform.admin;

import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import java.time.Instant;
import java.util.Optional;
import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;

class AdminAuthServiceTest {
    private final AdminRepository repository = mock(AdminRepository.class);
    private final AdminAuthService service = new AdminAuthService(repository);

    @Test void acceptsRfcTotpAcrossAllowedWindowAndRejectsWrongCode() {
        String secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        assertTrue(AdminAuthService.verifyMfaCode(secret, "287082", Instant.ofEpochSecond(59)));
        assertTrue(AdminAuthService.verifyMfaCode(secret, "287082", Instant.ofEpochSecond(60)));
        assertFalse(AdminAuthService.verifyMfaCode(secret, "287083", Instant.ofEpochSecond(59)));
        assertFalse(AdminAuthService.verifyMfaCode(secret, "842916", Instant.ofEpochSecond(59)));
    }

    @Test void bootstrapHashesPasswordAndPasswordOnlyLoginStoresOnlyTokenDigest() {
        when(repository.adminCount()).thenReturn(0);
        AdminModels.AdminUser admin = service.bootstrap("Admin@example.com", "correct horse battery", "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assertNotEquals("correct horse battery", admin.passwordHash);
        assertTrue(admin.passwordHash.length() >= 40);
        ArgumentCaptor<String> tokenHash = ArgumentCaptor.forClass(String.class);
        when(repository.adminByEmail("admin@example.com")).thenReturn(Optional.of(admin));
        AdminAuthService.LoginResult login = service.login("admin@example.com", "correct horse battery");
        verify(repository).insertAdminSession(anyString(), eq(admin.id), tokenHash.capture(), any(Instant.class));
        assertEquals(AdminAuthService.hashToken(login.token), tokenHash.getValue());
        assertNotEquals(login.token, tokenHash.getValue());
        assertFalse(admin.passwordHash.contains("correct horse"));
    }

    @Test void dangerousOperationStillRejectsWrongMfa() {
        when(repository.adminCount()).thenReturn(0);
        AdminModels.AdminUser admin = service.bootstrap("admin@example.com", "correct horse battery", "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        when(repository.adminById(admin.id)).thenReturn(Optional.of(admin));
        AdminModels.AdminSession session = new AdminModels.AdminSession();
        session.adminId = admin.id;
        AdminException error = assertThrows(AdminException.class, () -> service.requireMfa(session, "000000"));
        assertEquals("mfa_invalid", error.code);
    }

    @Test void repeatedBadPasswordsAreRateLimited() {
        when(repository.adminCount()).thenReturn(0);
        AdminModels.AdminUser admin = service.bootstrap("admin@example.com", "correct horse battery", "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        when(repository.adminByEmail(admin.email)).thenReturn(Optional.of(admin));
        for (int count = 0; count < 5; count++) assertEquals("invalid_credentials", assertThrows(AdminException.class, () -> service.login(admin.email, "wrong")).code);
        assertEquals("login_rate_limited", assertThrows(AdminException.class, () -> service.login(admin.email, "correct horse battery")).code);
        verify(repository, never()).insertAdminSession(any(), any(), any(), any());
    }

    @Test void unavailableRepositoryRemainsExplicit() {
        AdminRepository unavailable = new UnavailableAdminRepository();
        assertEquals("unavailable", assertThrows(AdminException.class, () -> new AdminAuthService(unavailable).login("a@example.com", "password")).code);
    }
}
