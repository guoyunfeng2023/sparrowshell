package com.sparrowshell.ai.platform.account;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.Collections;
import java.util.Optional;
import java.util.UUID;
import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;

public class AccountServiceTest {
    private AccountRepository repository;
    private VerificationCodeSender verificationCodeSender;
    private AccountService service;
    private final String accountId = UUID.randomUUID().toString();
    private final String deviceId = UUID.randomUUID().toString();

    @BeforeEach public void setUp() {
        repository = mock(AccountRepository.class); verificationCodeSender = mock(VerificationCodeSender.class);
        service = new AccountService(repository, mock(com.sparrowshell.ai.platform.job.BackgroundJobService.class), verificationCodeSender);
    }

    @Test public void registrationStoresOnlyPasswordDerivativesAndReturnsSession() {
        when(repository.accountByEmail("person@example.test")).thenReturn(Optional.empty());
        when(repository.verificationForUpdate("person@example.test", "register")).thenReturn(Optional.of(verification("person@example.test", "register", "123456")));
        AccountModels.TokenResponse tokens = service.register("Person@Example.test", "correct-horse-42", "123456", deviceId, "Mac", "0.1.0");
        ArgumentCaptor<AccountModels.AccountRecord> account = ArgumentCaptor.forClass(AccountModels.AccountRecord.class);
        verify(repository).insertAccount(account.capture());
        assertNotEquals("correct-horse-42", account.getValue().passwordHash);
        assertNotEquals("correct-horse-42", account.getValue().passwordSalt);
        assertEquals(accountId.getClass(), account.getValue().id.getClass());
        assertNotNull(tokens.accessToken);
        assertNotNull(tokens.refreshToken);
        assertEquals(tokens.deviceId, deviceId);
        verify(repository).insertDevice(any(AccountModels.DeviceSession.class));
        ArgumentCaptor<AccountModels.RefreshToken> refresh = ArgumentCaptor.forClass(AccountModels.RefreshToken.class);
        verify(repository).insertRefreshToken(refresh.capture());
        long validityDays = ChronoUnit.DAYS.between(Instant.now(), refresh.getValue().expiresAt);
        assertTrue(validityDays >= 179 && validityDays <= 180);
    }

    @Test public void duplicateEmailIsRejected() {
        when(repository.accountByEmail("person@example.test")).thenReturn(Optional.of(new AccountModels.AccountRecord()));
        AccountException error = assertThrows(AccountException.class, () -> service.register("person@example.test", "correct-horse-42", "123456", deviceId, null, null));
        assertEquals("email_registered", error.code);
    }

    @Test public void loginAuthenticatesAgainstStoredHashAndRevokesOnLogout() {
        AccountModels.AccountRecord account = activeAccount();
        when(repository.accountByEmail(account.email)).thenReturn(Optional.of(account));
        AccountModels.TokenResponse tokens = service.login(account.email, "correct-horse-42", deviceId, null, null);
        AccountModels.DeviceSession stored = new AccountModels.DeviceSession();
        stored.accountId = account.id; stored.deviceId = deviceId; stored.sessionId = tokens.sessionId;
        when(repository.deviceSession(anyString(), any(Instant.class))).thenReturn(Optional.of(stored));
        assertEquals(account.id, service.authenticate("Bearer " + tokens.accessToken).accountId);
        service.logout("Bearer " + tokens.accessToken);
        verify(repository).revokeDevice(tokens.sessionId);
        verify(repository).revokeRefreshTokens(eq(tokens.sessionId), any(Instant.class));
    }

    @Test public void refreshRotatesBothTokensAndReuseRevokesTheSession() {
        AccountModels.RefreshToken stored = new AccountModels.RefreshToken(); stored.tokenHash = "stored-hash"; stored.accountId = accountId; stored.deviceId = deviceId;
        stored.sessionId = UUID.randomUUID().toString(); stored.accountStatus = "active"; stored.generation = 2;
        stored.expiresAt = Instant.now().plusSeconds(3600);
        when(repository.refreshToken(anyString())).thenReturn(Optional.of(stored));
        when(repository.accountById(accountId)).thenReturn(Optional.of(activeAccount()));
        when(repository.rotateRefreshToken(anyString(), any(), anyString(), any(), any())).thenReturn(true);
        AccountModels.TokenResponse response = service.refresh("old-refresh", deviceId);
        assertNotNull(response.accessToken); assertNotNull(response.refreshToken); assertNotEquals("old-refresh", response.refreshToken);
        ArgumentCaptor<AccountModels.RefreshToken> rotated = ArgumentCaptor.forClass(AccountModels.RefreshToken.class);
        verify(repository).rotateRefreshToken(anyString(), rotated.capture(), anyString(), any(), any());
        long validityDays = ChronoUnit.DAYS.between(Instant.now(), rotated.getValue().expiresAt);
        assertTrue(validityDays >= 179 && validityDays <= 180);

        stored.rotatedAt = Instant.now();
        assertThrows(AccountException.class, () -> service.refresh("old-refresh", deviceId));
        verify(repository).revokeDevice(stored.sessionId);
        verify(repository).revokeRefreshTokens(eq(stored.sessionId), any(Instant.class));
    }

    @Test public void pushRejectsSecretsBeforeWriting() {
        AccountModels.SyncMutation mutation = mutation("{\"name\":\"bad\",\"password\":\"never-upload\"}");
        try {
            service.push(session(), deviceId, Collections.singletonList(mutation));
            fail("secret payload must be rejected");
        } catch (AccountException exception) { assertEquals("sensitive_payload", exception.code); }
        verify(repository, never()).saveEntity(any());
    }

    @Test public void pushRejectsUnknownDtoFieldsBeforeWriting() {
        String entityId = UUID.randomUUID().toString();
        AccountModels.SyncMutation mutation = mutation(serverPayload(entityId).replace("\"name\":\"local\"", "\"name\":\"local\",\"unexpected\":true"));
        mutation.entityId = entityId;
        AccountException error = assertThrows(AccountException.class, () -> service.push(session(), deviceId, Collections.singletonList(mutation)));
        assertEquals("invalid_sync_payload", error.code); verify(repository, never()).saveEntity(any());
    }

    @Test public void stalePushReturnsCurrentWithoutOverwriting() {
        AccountModels.SyncEntity current = new AccountModels.SyncEntity();
        current.userId = accountId; current.entityType = "server"; current.entityId = UUID.randomUUID().toString(); current.revision = 2;
        current.changeSeq = 8; current.payload = serverPayload(current.entityId);
        AccountModels.SyncMutation mutation = mutation(serverPayload(current.entityId).replace("\"revision\":1", "\"revision\":2"));
        mutation.entityId = current.entityId; mutation.baseRevision = 1;
        when(repository.receipt(eq(accountId), anyString())).thenReturn(Optional.empty());
        when(repository.entityForUpdate(accountId, "server", current.entityId)).thenReturn(Optional.of(current));
        java.util.List<AccountModels.SyncPushResult> result = service.push(session(), deviceId, Collections.singletonList(mutation));
        assertFalse(result.get(0).accepted);
        assertEquals("revision_conflict", result.get(0).errorCode);
        assertEquals(Long.valueOf(2), result.get(0).revision);
        verify(repository, never()).saveEntity(any());
        verify(repository).insertReceipt(eq(accountId), eq(mutation.mutationId), anyString());
    }

    @Test public void acceptedServerUsesCamelCaseMetadataForRustDto() {
        String entityId = UUID.randomUUID().toString();
        AccountModels.SyncMutation mutation = mutation(serverPayload(entityId));
        mutation.entityId = entityId;
        service.push(session(), deviceId, Collections.singletonList(mutation));
        ArgumentCaptor<AccountModels.SyncEntity> saved = ArgumentCaptor.forClass(AccountModels.SyncEntity.class);
        verify(repository).saveEntity(saved.capture());
        assertTrue(saved.getValue().payload.contains("\"updatedAt\""));
        assertTrue(saved.getValue().payload.contains("\"deletedAt\":null"));
        assertFalse(saved.getValue().payload.contains("\"updated_at\""));
    }

    @Test public void acceptedServerAllowsOptionalSortOrder() {
        String entityId = UUID.randomUUID().toString();
        AccountModels.SyncMutation mutation = mutation(serverPayload(entityId).replace("\"revision\":1", "\"sortOrder\":12,\"revision\":1"));
        mutation.entityId = entityId;
        service.push(session(), deviceId, Collections.singletonList(mutation));
        ArgumentCaptor<AccountModels.SyncEntity> saved = ArgumentCaptor.forClass(AccountModels.SyncEntity.class);
        verify(repository).saveEntity(saved.capture());
        assertTrue(saved.getValue().payload.contains("\"sortOrder\":12"));
    }

    @Test public void firstPushAcceptsLocallyAdvancedRevisionWhenCloudEntityIsAbsent() {
        String entityId = UUID.randomUUID().toString();
        AccountModels.SyncMutation mutation = mutation(serverPayload(entityId).replace("\"revision\":1", "\"revision\":11"));
        mutation.entityId = entityId; mutation.baseRevision = 10;
        java.util.List<AccountModels.SyncPushResult> result = service.push(session(), deviceId, Collections.singletonList(mutation));
        assertTrue(result.get(0).accepted);
        assertEquals(Long.valueOf(11), result.get(0).revision);
        ArgumentCaptor<AccountModels.SyncEntity> saved = ArgumentCaptor.forClass(AccountModels.SyncEntity.class);
        verify(repository).saveEntity(saved.capture());
        assertEquals(11, saved.getValue().revision);
    }

    @Test public void firstPushAcceptsDeleteWhenCloudEntityIsAlreadyAbsent() {
        String entityId = UUID.randomUUID().toString();
        AccountModels.SyncMutation mutation = mutation(serverPayload(entityId).replace("\"revision\":1", "\"revision\":4"));
        mutation.entityId = entityId; mutation.baseRevision = 3; mutation.deleted = true;
        java.util.List<AccountModels.SyncPushResult> result = service.push(session(), deviceId, Collections.singletonList(mutation));
        assertTrue(result.get(0).accepted);
        assertEquals(Long.valueOf(4), result.get(0).revision);
    }

    @Test public void rejectedReceiptIsReevaluatedWithoutChangingMutationId() {
        String entityId = UUID.randomUUID().toString();
        AccountModels.SyncMutation mutation = mutation(serverPayload(entityId).replace("\"revision\":1", "\"revision\":4"));
        mutation.entityId = entityId; mutation.baseRevision = 3; mutation.deleted = true;
        AccountModels.SyncPushResult rejected = new AccountModels.SyncPushResult(); rejected.mutationId = mutation.mutationId; rejected.accepted = false; rejected.revision = 0L; rejected.errorCode = "revision_conflict";
        when(repository.receipt(accountId, mutation.mutationId)).thenReturn(Optional.of(rejected));
        java.util.List<AccountModels.SyncPushResult> result = service.push(session(), deviceId, Collections.singletonList(mutation));
        assertTrue(result.get(0).accepted);
        assertEquals(mutation.mutationId, result.get(0).mutationId);
        assertEquals(Long.valueOf(4), result.get(0).revision);
        verify(repository).insertReceipt(eq(accountId), eq(mutation.mutationId), contains("\"accepted\":true"));
    }

    @Test public void replayReturnsStoredResultWithoutRaisingRevision() {
        AccountModels.SyncMutation mutation = mutation("{\"name\":\"local\"}");
        AccountModels.SyncPushResult stored = new AccountModels.SyncPushResult(); stored.mutationId = mutation.mutationId; stored.accepted = true; stored.revision = 1L; stored.changeSeq = 4L;
        when(repository.receipt(accountId, mutation.mutationId)).thenReturn(Optional.of(stored));
        assertSame(stored, service.push(session(), deviceId, Collections.singletonList(mutation)).get(0));
        verify(repository, never()).saveEntity(any()); verify(repository, never()).insertReceipt(anyString(), anyString(), anyString());
    }

    @Test public void snapshotUsesCursorAndReportsMorePages() {
        AccountModels.SyncEntity first = new AccountModels.SyncEntity(); first.changeSeq = 4; first.entityType = "server";
        first.entityId = UUID.randomUUID().toString(); first.revision = 1; first.payload = serverPayload(first.entityId);
        AccountModels.SyncEntity second = new AccountModels.SyncEntity(); second.changeSeq = 5; second.entityType = "server";
        second.entityId = UUID.randomUUID().toString(); second.revision = 1; second.payload = serverPayload(second.entityId);
        when(repository.snapshot(accountId, 0, 2)).thenReturn(java.util.Arrays.asList(first, second));
        AccountModels.SyncPullPage page = service.snapshot(session(), 0, 1);
        assertTrue(page.hasMore); assertEquals(4, page.nextCursor); assertEquals(1, page.changes.size());
    }

    @Test public void passwordPolicyRequiresLettersAndDigitsOrSymbols() {
        assertDoesNotThrow(() -> AccountService.validatePasswordForTest("abcdefg1"));
        assertDoesNotThrow(() -> AccountService.validatePasswordForTest("abcdefg!"));
        assertDoesNotThrow(() -> AccountService.validatePasswordForTest("abc123!@#"));
        for (String password : new String[]{"abcdefgh", "12345678", "1234!@#$", "abc123", "abcd 123", "密码abcd123"}) {
            assertThrows(AccountException.class, () -> AccountService.validatePasswordForTest(password));
        }
    }

    @Test public void verificationCodeIsStoredAsHashAndSentForRegistration() {
        when(repository.accountByEmail("person@example.test")).thenReturn(Optional.empty());
        service.sendVerificationCode("Person@Example.test", "register");
        ArgumentCaptor<AccountModels.VerificationCode> stored = ArgumentCaptor.forClass(AccountModels.VerificationCode.class);
        verify(repository).replaceVerificationCode(stored.capture());
        assertEquals(5 * 60, stored.getValue().expiresAt.getEpochSecond() - stored.getValue().createdAt.getEpochSecond());
        verify(verificationCodeSender).send(eq("person@example.test"), argThat(code -> code.matches("\\d{6}") && !stored.getValue().codeHash.contains(code)), eq("register"), eq("zh-CN"));
    }

    @Test public void invalidRegistrationCodeIsRejectedAndCounted() {
        when(repository.accountByEmail("person@example.test")).thenReturn(Optional.empty());
        when(repository.verificationForUpdate("person@example.test", "register")).thenReturn(Optional.of(verification("person@example.test", "register", "123456")));
        AccountException error = assertThrows(AccountException.class, () -> service.register("person@example.test", "correct-horse-42", "654321", deviceId, null, null));
        assertEquals("invalid_verification_code", error.code);
        verify(repository).failVerification(anyString(), any(Instant.class));
        verify(repository, never()).insertAccount(any());
    }

    @Test public void passwordResetUpdatesDerivativesAndRevokesAllSessions() {
        AccountModels.AccountRecord account = activeAccount();
        when(repository.accountByEmail(account.email)).thenReturn(Optional.of(account));
        when(repository.verificationForUpdate(account.email, "password_reset")).thenReturn(Optional.of(verification(account.email, "password_reset", "123456")));
        service.resetPassword(account.email, "123456", "new-pass-1");
        verify(repository).updatePassword(eq(account.id), anyString(), anyString(), any(Instant.class));
        verify(repository).markVerificationUsed(anyString(), any(Instant.class));
        verify(repository).revokeAllSessions(eq(account.id), any(Instant.class));
    }

    @Test public void nicknameCanBeChangedWithoutChangingLoginEmail() {
        AccountModels.AccountRecord account = activeAccount();
        when(repository.deviceSession(anyString(), any(Instant.class))).thenReturn(Optional.of(session()));
        when(repository.accountById(accountId)).thenReturn(Optional.of(account));
        AccountModels.ProfileResponse profile = service.updateNickname("Bearer token", "Sparrow");
        assertEquals("Sparrow", profile.nickname); assertEquals(account.email, profile.email);
        verify(repository).updateNickname(eq(account.id), eq("Sparrow"), any(Instant.class));
    }

    @Test public void passwordChangeRequiresCurrentPasswordAndRevokesAllSessions() {
        AccountModels.AccountRecord account = activeAccount();
        when(repository.deviceSession(anyString(), any(Instant.class))).thenReturn(Optional.of(session()));
        when(repository.accountById(accountId)).thenReturn(Optional.of(account));
        service.changePassword("Bearer token", "correct-horse-42", "new-pass-1");
        verify(repository).updatePassword(eq(account.id), anyString(), anyString(), any(Instant.class));
        verify(repository).revokeAllSessions(eq(account.id), any(Instant.class));
    }

    private AccountModels.VerificationCode verification(String email, String purpose, String code) {
        AccountModels.VerificationCode value = new AccountModels.VerificationCode(); value.id = UUID.randomUUID().toString();
        value.email = email; value.purpose = purpose; value.createdAt = Instant.now(); value.expiresAt = value.createdAt.plusSeconds(300);
        try {
            java.security.MessageDigest digest = java.security.MessageDigest.getInstance("SHA-256");
            value.codeHash = java.util.Base64.getEncoder().encodeToString(digest.digest((email + "|" + purpose + "|" + code).getBytes(java.nio.charset.StandardCharsets.UTF_8)));
            return value;
        } catch (java.security.GeneralSecurityException exception) { throw new IllegalStateException(exception); }
    }

    private AccountModels.AccountRecord activeAccount() {
        AccountModels.AccountRecord value = new AccountModels.AccountRecord(); value.id = accountId; value.email = "person@example.test"; value.nickname = "person@example.test"; value.status = "active";
        value.passwordSalt = java.util.Base64.getEncoder().encodeToString("0123456789abcdef".getBytes(java.nio.charset.StandardCharsets.UTF_8));
        value.passwordHash = AccountService.hashPasswordForTest("correct-horse-42", value.passwordSalt); return value;
    }
    private AccountModels.DeviceSession session() { AccountModels.DeviceSession value = new AccountModels.DeviceSession(); value.accountId = accountId; value.deviceId = deviceId; return value; }
    private AccountModels.SyncMutation mutation(String json) {
        AccountModels.SyncMutation value = new AccountModels.SyncMutation(); value.mutationId = UUID.randomUUID().toString(); value.entityType = "server"; value.entityId = UUID.randomUUID().toString();
        value.baseRevision = 0; value.schemaVersion = 1; value.data = json; return value;
    }
    private static String serverPayload(String id) {
        return "{\"id\":\"" + id + "\",\"folderId\":null,\"name\":\"local\",\"host\":\"127.0.0.1\",\"port\":22,\"username\":\"tester\",\"authType\":\"password\",\"privateKeyPathHint\":null,\"jumpServerId\":null,\"revision\":1,\"updatedAt\":\"2026-10-30T00:00:00Z\",\"deletedAt\":null}";
    }
}
