package com.sparrowshell.ai;

import com.sparrowshell.ai.platform.account.AccountException;
import com.sparrowshell.ai.platform.account.AccountService;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.*;

class AiIdentityServiceTest {
    private final AccountService accounts = mock(AccountService.class);
    private final AiIdentityService service = new AiIdentityService(accounts);

    @Test void normalizesAnonymousUuidWithoutCallingAccountAuthentication() {
        String subject = service.subject(null, " D2D5BAFA-76CE-4A72-9DB7-A82F527DD17B ");
        assertEquals("anonymous:d2d5bafa-76ce-4a72-9db7-a82f527dd17b", subject);
        verifyNoInteractions(accounts);
    }

    @Test void rejectsMissingOrMalformedAnonymousIds() {
        for (String value : new String[]{null, "", "not-a-uuid", "d2d5bafa-76ce-4a72-9db7-a82f527dd17b-extra"}) {
            ProviderException error = assertThrows(ProviderException.class, () -> service.subject(null, value));
            assertEquals("invalid_identity", error.code());
        }
        verifyNoInteractions(accounts);
    }

    @Test void bearerUsesAuthenticatedAccountAndNeverFallsBackToAnonymous() {
        when(accounts.authenticateAccountId("Bearer valid")).thenReturn("account-123");
        assertEquals("account-123", service.subject("Bearer valid", "d2d5bafa-76ce-4a72-9db7-a82f527dd17b"));

        when(accounts.authenticateAccountId("Bearer expired")).thenThrow(mock(AccountException.class));
        ProviderException error = assertThrows(ProviderException.class,
                () -> service.subject("Bearer expired", "d2d5bafa-76ce-4a72-9db7-a82f527dd17b"));
        assertEquals("unauthorized", error.code());
        verify(accounts, never()).authenticateAccountId(null);
    }
}
