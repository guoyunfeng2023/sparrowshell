package com.sparrowshell.ai.platform.account;

import java.time.Instant;
import java.util.List;
import java.util.Optional;

interface AccountRepository {
    default Optional<AccountModels.AccountRecord> accountByEmail(String email) { throw AccountException.unavailable(); }
    default Optional<AccountModels.AccountRecord> accountById(String id) { throw AccountException.unavailable(); }
    default void insertAccount(AccountModels.AccountRecord account) { throw AccountException.unavailable(); }
    default void insertDevice(AccountModels.DeviceSession session) { throw AccountException.unavailable(); }
    default void insertRefreshToken(AccountModels.RefreshToken token) { throw AccountException.unavailable(); }
    default Optional<AccountModels.DeviceSession> deviceSession(String tokenHash, Instant now) { throw AccountException.unavailable(); }
    default Optional<AccountModels.RefreshToken> refreshToken(String tokenHash) { throw AccountException.unavailable(); }
    default boolean rotateRefreshToken(String previousHash, AccountModels.RefreshToken next, String accessHash, Instant accessExpiresAt, Instant now) { throw AccountException.unavailable(); }
    default void touchDevice(String sessionId, Instant at) { throw AccountException.unavailable(); }
    default void revokeDevice(String sessionId) { throw AccountException.unavailable(); }
    default void revokeRefreshTokens(String sessionId, Instant at) { throw AccountException.unavailable(); }
    default void markAccountDeleted(String accountId, Instant at) { throw AccountException.unavailable(); }
    default void replaceVerificationCode(AccountModels.VerificationCode code) { throw AccountException.unavailable(); }
    default Optional<AccountModels.VerificationCode> latestVerification(String email, String purpose) { throw AccountException.unavailable(); }
    default Optional<AccountModels.VerificationCode> verificationForUpdate(String email, String purpose) { throw AccountException.unavailable(); }
    default void failVerification(String id, Instant at) { throw AccountException.unavailable(); }
    default void markVerificationUsed(String id, Instant at) { throw AccountException.unavailable(); }
    default void updateNickname(String accountId, String nickname, Instant at) { throw AccountException.unavailable(); }
    default void updatePassword(String accountId, String passwordHash, String passwordSalt, Instant at) { throw AccountException.unavailable(); }
    default void revokeAllSessions(String accountId, Instant at) { throw AccountException.unavailable(); }
    default void lockAccount(String accountId) { throw AccountException.unavailable(); }
    default Optional<AccountModels.SyncPushResult> receipt(String accountId, String mutationId) { throw AccountException.unavailable(); }
    default void insertReceipt(String accountId, String mutationId, String responseJson) { throw AccountException.unavailable(); }
    default Optional<AccountModels.SyncEntity> entityForUpdate(String accountId, String entityType, String entityId) { throw AccountException.unavailable(); }
    default void saveEntity(AccountModels.SyncEntity entity) { throw AccountException.unavailable(); }
    default long maxChangeSeq(String accountId) { throw AccountException.unavailable(); }
    default List<AccountModels.SyncEntity> changes(String accountId, long cursor, int limit) { throw AccountException.unavailable(); }
    default List<AccountModels.SyncEntity> snapshot(String accountId, long cursor, int limit) { throw AccountException.unavailable(); }
}
