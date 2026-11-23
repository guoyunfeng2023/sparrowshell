package com.sparrowshell.ai.platform.account;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import com.sparrowshell.ai.platform.job.BackgroundJobService;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import javax.crypto.SecretKeyFactory;
import javax.crypto.spec.PBEKeySpec;
import java.nio.charset.StandardCharsets;
import java.security.GeneralSecurityException;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.ArrayList;
import java.util.Base64;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

@Service
public class AccountService {
    private static final SecureRandom RANDOM = new SecureRandom();
    private static final ObjectMapper JSON = new ObjectMapper();
    private static final int PBKDF2_ITERATIONS = 120_000;
    private final AccountRepository repository;
    private final BackgroundJobService jobs;
    private final VerificationCodeSender verificationCodeSender;
    private final ConcurrentHashMap<String, LoginAttempts> attempts = new ConcurrentHashMap<String, LoginAttempts>();
    public AccountService(AccountRepository repository, BackgroundJobService jobs, VerificationCodeSender verificationCodeSender) {
        this.repository = repository; this.jobs = jobs; this.verificationCodeSender = verificationCodeSender;
    }

    @Transactional public void sendVerificationCode(String email, String purpose) {
        sendVerificationCode(email, purpose, "zh-CN");
    }

    @Transactional public void sendVerificationCode(String email, String purpose, String locale) {
        String normalized = normalizeEmail(email);
        if (!"register".equals(purpose) && !"password_reset".equals(purpose)) throw new AccountException("invalid_verification_purpose", "验证码用途无效");
        if ("register".equals(purpose) && repository.accountByEmail(normalized).isPresent()) throw new AccountException("email_registered", "该邮箱已注册");
        Instant now = Instant.now();
        Optional<AccountModels.VerificationCode> latest = repository.latestVerification(normalized, purpose);
        if (latest.isPresent() && latest.get().createdAt != null && latest.get().createdAt.plusSeconds(60).isAfter(now)) {
            throw new AccountException("verification_rate_limited", "验证码发送过于频繁，请 60 秒后重试");
        }
        String code = String.format("%06d", RANDOM.nextInt(1_000_000));
        AccountModels.VerificationCode verification = new AccountModels.VerificationCode();
        verification.id = UUID.randomUUID().toString(); verification.email = normalized; verification.purpose = purpose;
        verification.codeHash = hashToken(normalized + "|" + purpose + "|" + code);
        verification.createdAt = now; verification.expiresAt = now.plus(5, ChronoUnit.MINUTES);
        repository.replaceVerificationCode(verification);
        if ("register".equals(purpose) || repository.accountByEmail(normalized).isPresent()) verificationCodeSender.send(normalized, code, purpose, locale);
    }

    @Transactional public AccountModels.TokenResponse register(String email, String password, String code, String deviceId, String deviceName, String clientVersion) {
        String normalized = normalizeEmail(email); validatePassword(password); validateDevice(deviceId);
        if (repository.accountByEmail(normalized).isPresent()) throw new AccountException("email_registered", "该邮箱已注册");
        verifyVerificationCode(normalized, "register", code);
        byte[] salt = new byte[16]; RANDOM.nextBytes(salt);
        AccountModels.AccountRecord account = new AccountModels.AccountRecord(); account.id = UUID.randomUUID().toString(); account.email = normalized;
        account.nickname = normalized;
        account.passwordSalt = Base64.getEncoder().encodeToString(salt); account.passwordHash = hashPassword(password, account.passwordSalt);
        account.status = "active"; account.createdAt = Instant.now(); account.lastActiveAt = account.createdAt;
        repository.insertAccount(account); return issueSession(account, deviceId, clean(deviceName, 160), clean(clientVersion, 40));
    }

    @Transactional public AccountModels.TokenResponse login(String email, String password, String deviceId, String deviceName, String clientVersion) {
        String normalized;
        try { normalized = normalizeEmail(email); } catch (AccountException exception) { throw AccountException.unauthorized(); }
        validateDevice(deviceId); checkRateLimit(normalized);
        try {
            AccountModels.AccountRecord account = repository.accountByEmail(normalized).orElseThrow(AccountService::invalidLogin);
            if (!"active".equals(account.status) || !verifyPassword(password, account.passwordSalt, account.passwordHash)) throw invalidLogin();
            attempts.remove(normalized); return issueSession(account, deviceId, clean(deviceName, 160), clean(clientVersion, 40));
        } catch (AccountException exception) {
            if ("invalid_credentials".equals(exception.code)) recordFailure(normalized);
            throw exception;
        }
    }

    @Transactional public void resetPassword(String email, String code, String newPassword) {
        String normalized = normalizeEmail(email); validatePassword(newPassword);
        AccountModels.AccountRecord account = repository.accountByEmail(normalized).orElseThrow(AccountService::invalidVerificationCode);
        if (!"active".equals(account.status)) throw invalidVerificationCode();
        verifyVerificationCode(normalized, "password_reset", code);
        byte[] salt = new byte[16]; RANDOM.nextBytes(salt);
        String encodedSalt = Base64.getEncoder().encodeToString(salt);
        repository.updatePassword(account.id, hashPassword(newPassword, encodedSalt), encodedSalt, Instant.now());
        repository.revokeAllSessions(account.id, Instant.now());
    }

    @Transactional public AccountModels.ProfileResponse profile(String authorization) {
        AccountModels.DeviceSession session = authenticate(authorization);
        return profile(repository.accountById(session.accountId).orElseThrow(AccountException::unauthorized));
    }

    @Transactional public AccountModels.ProfileResponse updateNickname(String authorization, String nickname) {
        AccountModels.DeviceSession session = authenticate(authorization);
        String value = nickname == null ? "" : nickname.trim();
        if (value.isEmpty() || value.length() > 30) throw new AccountException("invalid_nickname", "用户名必须为 1～30 个字符");
        repository.updateNickname(session.accountId, value, Instant.now());
        AccountModels.AccountRecord account = repository.accountById(session.accountId).orElseThrow(AccountException::unauthorized);
        account.nickname = value;
        return profile(account);
    }

    @Transactional public void changePassword(String authorization, String currentPassword, String newPassword) {
        AccountModels.DeviceSession session = authenticate(authorization);
        AccountModels.AccountRecord account = repository.accountById(session.accountId).orElseThrow(AccountException::unauthorized);
        if (!verifyPassword(currentPassword, account.passwordSalt, account.passwordHash)) throw AccountException.unauthorized();
        validatePassword(newPassword);
        if (verifyPassword(newPassword, account.passwordSalt, account.passwordHash)) throw new AccountException("password_unchanged", "新密码不能与当前密码相同");
        byte[] salt = new byte[16]; RANDOM.nextBytes(salt);
        String encodedSalt = Base64.getEncoder().encodeToString(salt);
        repository.updatePassword(account.id, hashPassword(newPassword, encodedSalt), encodedSalt, Instant.now());
        repository.revokeAllSessions(account.id, Instant.now());
    }

    public AccountModels.DeviceSession authenticate(String authorization) {
        String token = bearer(authorization);
        AccountModels.DeviceSession session = repository.deviceSession(hashToken(token), Instant.now()).orElseThrow(AccountException::unauthorized);
        repository.touchDevice(session.sessionId, Instant.now()); return session;
    }
    public String authenticateAccountId(String authorization) { return authenticate(authorization).accountId; }
    @Transactional public void logout(String authorization) {
        String sessionId = authenticate(authorization).sessionId;
        repository.revokeDevice(sessionId); repository.revokeRefreshTokens(sessionId, Instant.now());
    }
    @Transactional public void deleteAccount(String authorization) {
        AccountModels.DeviceSession session = authenticate(authorization);
        repository.markAccountDeleted(session.accountId, Instant.now());
        java.util.Map<String, String> payload = new java.util.LinkedHashMap<String, String>();
        payload.put("accountId", session.accountId); payload.put("requestId", UUID.randomUUID().toString());
        jobs.enqueue("account_deletion", payload, session.accountId, 8);
    }
    @Transactional(noRollbackFor = AccountException.class) public AccountModels.TokenResponse refresh(String rawToken, String deviceId) {
        validateDevice(deviceId);
        if (rawToken == null || rawToken.trim().isEmpty() || rawToken.length() > 512) throw AccountException.unauthorized();
        AccountModels.RefreshToken stored = repository.refreshToken(hashToken(rawToken)).orElseThrow(AccountException::unauthorized);
        Instant now = Instant.now();
        if (stored.rotatedAt != null || stored.revokedAt != null || stored.sessionRevokedAt != null) {
            if (stored.sessionRevokedAt == null) {
                repository.revokeDevice(stored.sessionId); repository.revokeRefreshTokens(stored.sessionId, now);
            }
            throw AccountException.unauthorized();
        }
        if (!stored.deviceId.equals(deviceId) || !"active".equals(stored.accountStatus)
                || stored.expiresAt == null || !stored.expiresAt.isAfter(now)) throw AccountException.unauthorized();
        String accessToken = randomToken(); String refreshToken = randomToken();
        Instant accessExpiresAt = now.plus(30, ChronoUnit.MINUTES);
        AccountModels.RefreshToken next = new AccountModels.RefreshToken();
        next.tokenHash = hashToken(refreshToken); next.sessionId = stored.sessionId; next.accountId = stored.accountId;
        next.deviceId = stored.deviceId; next.generation = stored.generation + 1;
        next.expiresAt = now.plus(180, ChronoUnit.DAYS);
        if (!repository.rotateRefreshToken(stored.tokenHash, next, hashToken(accessToken), accessExpiresAt, now)) {
            repository.revokeDevice(stored.sessionId); repository.revokeRefreshTokens(stored.sessionId, now);
            throw AccountException.unauthorized();
        }
        AccountModels.AccountRecord account = repository.accountById(stored.accountId).orElseThrow(AccountException::unauthorized);
        return tokenResponse(account, deviceId, accessToken, refreshToken, accessExpiresAt, stored.sessionId);
    }

    @Transactional public List<AccountModels.SyncPushResult> push(AccountModels.DeviceSession session, String deviceId, List<AccountModels.SyncMutation> changes) {
        if (!session.deviceId.equals(deviceId) || changes == null || changes.isEmpty() || changes.size() > 100) throw new AccountException("invalid_sync_request", "同步设备或批次无效");
        repository.lockAccount(session.accountId);
        long sequence = repository.maxChangeSeq(session.accountId);
        List<AccountModels.SyncPushResult> results = new ArrayList<AccountModels.SyncPushResult>();
        for (AccountModels.SyncMutation mutation : changes) {
            Optional<AccountModels.SyncPushResult> receipt = repository.receipt(session.accountId, requireUuid(mutation.mutationId, "mutation_id"));
            if (receipt.isPresent() && receipt.get().accepted) { results.add(receipt.get()); continue; }
            validateMutation(mutation);
            ObjectNode data = objectPayload(mutation.entityType, mutation.data);
            if (!mutation.entityId.equals(data.path("id").asText())) throw new AccountException("entity_mismatch", "实体编号与载荷不一致");
            long payloadRevision = data.path("revision").asLong();
            if (payloadRevision < 1 || mutation.baseRevision == Long.MAX_VALUE || payloadRevision != mutation.baseRevision + 1) {
                throw new AccountException("invalid_sync_payload", "载荷版本必须等于基础版本加一");
            }
            Optional<AccountModels.SyncEntity> existing = repository.entityForUpdate(session.accountId, mutation.entityType, mutation.entityId);
            AccountModels.SyncPushResult result = new AccountModels.SyncPushResult(); result.mutationId = mutation.mutationId;
            long currentRevision = existing.isPresent() ? existing.get().revision : 0;
            if (existing.isPresent() && currentRevision != mutation.baseRevision) {
                result.accepted = false; result.errorCode = "revision_conflict"; result.revision = currentRevision;
                result.current = existing.map(AccountService::change).orElse(null);
            } else {
                Instant now = Instant.now(); sequence++;
                AccountModels.SyncEntity entity = new AccountModels.SyncEntity(); entity.userId = session.accountId; entity.entityType = mutation.entityType;
                entity.entityId = mutation.entityId; entity.revision = existing.isPresent() ? currentRevision + 1 : payloadRevision; entity.schemaVersion = mutation.schemaVersion;
                entity.changeSeq = sequence; entity.deletedAt = mutation.deleted ? now : null; entity.updatedAt = now;
                data.put("revision", entity.revision);
                if (!"server".equals(mutation.entityType)) {
                    data.put("updated_at", now.toString()); data.put("deleted_at", entity.deletedAt == null ? null : now.toString());
                } else {
                    data.put("updatedAt", now.toString()); data.put("deletedAt", entity.deletedAt == null ? null : now.toString());
                }
                entity.payload = json(data); repository.saveEntity(entity);
                result.accepted = true; result.revision = entity.revision; result.changeSeq = entity.changeSeq;
            }
            try { repository.insertReceipt(session.accountId, mutation.mutationId, JSON.writeValueAsString(result)); }
            catch (java.io.IOException exception) { throw new IllegalStateException(exception); }
            results.add(result);
        }
        return results;
    }

    @Transactional(readOnly = true) public AccountModels.SyncPullPage pull(AccountModels.DeviceSession session, long cursor, int limit) {
        if (cursor < 0 || limit < 1 || limit > 100) throw new AccountException("invalid_pagination", "同步分页参数无效");
        List<AccountModels.SyncEntity> rows = repository.changes(session.accountId, cursor, limit + 1);
        AccountModels.SyncPullPage page = new AccountModels.SyncPullPage(); page.hasMore = rows.size() > limit;
        if (page.hasMore) rows = rows.subList(0, limit);
        for (AccountModels.SyncEntity row : rows) page.changes.add(change(row));
        page.nextCursor = rows.isEmpty() ? cursor : rows.get(rows.size() - 1).changeSeq; return page;
    }

    @Transactional(readOnly = true) public AccountModels.SyncPullPage snapshot(AccountModels.DeviceSession session, long cursor, int limit) {
        if (cursor < 0 || limit < 1 || limit > 100) throw new AccountException("invalid_pagination", "快照分页参数无效");
        List<AccountModels.SyncEntity> rows = repository.snapshot(session.accountId, cursor, limit + 1);
        AccountModels.SyncPullPage page = new AccountModels.SyncPullPage(); page.hasMore = rows.size() > limit;
        if (page.hasMore) rows = rows.subList(0, limit);
        for (AccountModels.SyncEntity row : rows) page.changes.add(change(row));
        page.nextCursor = rows.isEmpty() ? cursor : rows.get(rows.size() - 1).changeSeq; return page;
    }

    private AccountModels.TokenResponse issueSession(AccountModels.AccountRecord account, String deviceId, String deviceName, String clientVersion) {
        String accessToken = randomToken(); String refreshToken = randomToken();
        AccountModels.DeviceSession session = new AccountModels.DeviceSession(); session.sessionId = UUID.randomUUID().toString(); session.accountId = account.id;
        session.deviceId = deviceId; session.deviceName = deviceName; session.clientVersion = clientVersion; session.tokenHash = hashToken(accessToken);
        session.createdAt = Instant.now(); session.lastSeenAt = session.createdAt; session.expiresAt = session.createdAt.plus(30, ChronoUnit.MINUTES);
        repository.insertDevice(session);
        AccountModels.RefreshToken refresh = new AccountModels.RefreshToken(); refresh.tokenHash = hashToken(refreshToken);
        refresh.sessionId = session.sessionId; refresh.accountId = account.id; refresh.deviceId = deviceId; refresh.generation = 1;
        refresh.expiresAt = session.createdAt.plus(180, ChronoUnit.DAYS); repository.insertRefreshToken(refresh);
        return tokenResponse(account, deviceId, accessToken, refreshToken, session.expiresAt, session.sessionId);
    }

    private static AccountModels.TokenResponse tokenResponse(AccountModels.AccountRecord account, String deviceId, String accessToken, String refreshToken, Instant expiresAt, String sessionId) {
        AccountModels.TokenResponse response = new AccountModels.TokenResponse(); response.userId = account.id; response.deviceId = deviceId;
        response.accessToken = accessToken; response.refreshToken = refreshToken; response.expiresAt = expiresAt.toString(); response.sessionId = sessionId;
        response.email = account.email; response.nickname = account.nickname; return response;
    }

    private static AccountModels.ProfileResponse profile(AccountModels.AccountRecord account) {
        AccountModels.ProfileResponse response = new AccountModels.ProfileResponse();
        response.email = account.email; response.nickname = account.nickname; return response;
    }
    private static String randomToken() {
        byte[] token = new byte[32]; RANDOM.nextBytes(token); return Base64.getUrlEncoder().withoutPadding().encodeToString(token);
    }

    private static void validateMutation(AccountModels.SyncMutation mutation) {
        requireUuid(mutation.entityId, "entity_id");
        if (!java.util.Arrays.asList("folder", "server", "batch_template", "workflow_definition").contains(mutation.entityType)
                || mutation.schemaVersion != 1 || mutation.baseRevision < 0) throw new AccountException("invalid_sync_request", "同步实体或版本无效");
    }
    private static ObjectNode objectPayload(String entityType, Object value) {
        try {
            JsonNode node = value instanceof String ? JSON.readTree((String) value) : JSON.valueToTree(value);
            if (node == null || !node.isObject() || JSON.writeValueAsBytes(node).length > 131_072) throw new AccountException("invalid_sync_payload", "同步载荷必须是大小受限的对象");
            rejectSecrets(node); validateStrictDto(entityType, (ObjectNode) node); return (ObjectNode) node;
        } catch (java.io.IOException | IllegalArgumentException exception) { throw new AccountException("invalid_sync_payload", "同步载荷不是有效 JSON"); }
    }
    private static void validateStrictDto(String entityType, ObjectNode data) {
        List<String> fields;
        if ("folder".equals(entityType)) fields = java.util.Arrays.asList("id", "parent_id", "name", "sort_order", "revision", "updated_at", "deleted_at");
        else if ("server".equals(entityType)) fields = java.util.Arrays.asList("id", "folderId", "name", "host", "port", "username", "authType", "privateKeyPathHint", "jumpServerId", "revision", "updatedAt", "deletedAt");
        else fields = java.util.Arrays.asList("id", "name", "definition", "revision", "updated_at", "deleted_at");
        if ("server".equals(entityType)) requireExactFields(data, fields, "sortOrder");
        else requireExactFields(data, fields);
        if (!data.path("id").isTextual() || !data.path("name").isTextual() || !data.path("revision").isIntegralNumber()) invalidPayload();
        if ("folder".equals(entityType) && !data.path("sort_order").isIntegralNumber()) invalidPayload();
        if ("server".equals(entityType) && data.has("sortOrder") && !data.path("sortOrder").isIntegralNumber()) invalidPayload();
        if ("server".equals(entityType)) {
            if (!data.path("host").isTextual() || !data.path("username").isTextual() || !data.path("authType").isTextual()
                    || !data.path("port").isIntegralNumber() || data.path("port").asInt() < 1 || data.path("port").asInt() > 65535
                    || !java.util.Arrays.asList("password", "private_key").contains(data.path("authType").asText())) invalidPayload();
        } else if (!"folder".equals(entityType)) {
            JsonNode definition = data.path("definition");
            if (!definition.isObject()) invalidPayload();
            List<String> definitionFields = "batch_template".equals(entityType)
                    ? java.util.Arrays.asList("command", "serverIds", "maxConcurrent")
                    : java.util.Arrays.asList("definitionId", "version", "startStep", "steps", "maxTransitions");
            requireExactFields((ObjectNode) definition, definitionFields);
        }
    }
    private static void requireExactFields(ObjectNode data, List<String> expected, String... optional) {
        for (String field : expected) if (!data.has(field)) invalidPayload();
        List<String> allowed = new java.util.ArrayList<String>(expected);
        allowed.addAll(java.util.Arrays.asList(optional));
        Iterator<String> actual = data.fieldNames();
        while (actual.hasNext()) if (!allowed.contains(actual.next())) invalidPayload();
    }
    private static void invalidPayload() { throw new AccountException("invalid_sync_payload", "同步载荷不符合实体 DTO"); }
    private static void rejectSecrets(JsonNode node) {
        Iterator<Map.Entry<String, JsonNode>> fields = node.fields();
        while (fields.hasNext()) {
            Map.Entry<String, JsonNode> field = fields.next(); String key = field.getKey();
            if (java.util.Arrays.asList("password", "passphrase", "privateKey", "private_key", "credentialRef", "credential_ref", "token", "secret").contains(key))
                throw new AccountException("sensitive_payload", "密码、私钥和凭据引用禁止同步");
            rejectSecrets(field.getValue());
        }
        if (node.isArray()) for (JsonNode child : node) rejectSecrets(child);
    }
    private static AccountModels.SyncChange change(AccountModels.SyncEntity entity) {
        AccountModels.SyncChange change = new AccountModels.SyncChange(); change.changeSeq = entity.changeSeq; change.entityType = entity.entityType;
        change.entityId = entity.entityId; change.revision = entity.revision; change.deleted = entity.deletedAt != null;
        try { change.data = JSON.readTree(entity.payload); } catch (java.io.IOException exception) { throw new IllegalStateException("Invalid stored sync payload", exception); }
        return change;
    }
    private static String json(Object value) { try { return JSON.writeValueAsString(value); } catch (java.io.IOException exception) { throw new IllegalStateException(exception); } }
    private static String requireUuid(String value, String field) { try { return UUID.fromString(value).toString(); } catch (RuntimeException exception) { throw new AccountException("invalid_sync_request", field + " 无效"); } }
    private static void validateDevice(String value) { requireUuid(value, "device_id"); }
    private static String normalizeEmail(String email) {
        String value = email == null ? "" : email.trim().toLowerCase(Locale.ROOT);
        if (!value.matches("[^@\\s]+@[^@\\s]+\\.[^@\\s]+") || value.length() > 254) throw new AccountException("invalid_email", "邮箱格式无效"); return value;
    }
    private void verifyVerificationCode(String email, String purpose, String code) {
        String provided = code == null ? "" : code.trim();
        if (!provided.matches("\\d{6}")) throw invalidVerificationCode();
        AccountModels.VerificationCode stored = repository.verificationForUpdate(email, purpose).orElseThrow(AccountService::invalidVerificationCode);
        Instant now = Instant.now();
        if (stored.usedAt != null || stored.attempts >= 5 || stored.expiresAt == null || !stored.expiresAt.isAfter(now)) throw invalidVerificationCode();
        String actual = hashToken(email + "|" + purpose + "|" + provided);
        if (stored.codeHash == null || !MessageDigest.isEqual(actual.getBytes(StandardCharsets.UTF_8), stored.codeHash.getBytes(StandardCharsets.UTF_8))) {
            repository.failVerification(stored.id, now);
            throw invalidVerificationCode();
        }
        repository.markVerificationUsed(stored.id, now);
    }

    private static void validatePassword(String password) {
        boolean valid = password != null && password.length() >= 8 && password.length() <= 256;
        boolean hasLetter = false; boolean hasDigitOrSymbol = false;
        if (valid) for (int index = 0; index < password.length(); index++) {
            char value = password.charAt(index);
            if (value < 33 || value > 126) { valid = false; break; }
            if ((value >= 'A' && value <= 'Z') || (value >= 'a' && value <= 'z')) hasLetter = true;
            else hasDigitOrSymbol = true;
        }
        if (!valid || !hasLetter || !hasDigitOrSymbol) throw new AccountException("weak_password", "密码至少 8 位，必须包含字母，并同时包含数字或符号，且不能包含空格或中文");
    }
    static void validatePasswordForTest(String password) { validatePassword(password); }
    private static AccountException invalidVerificationCode() { return new AccountException("invalid_verification_code", "验证码无效、已过期或已达到错误次数上限"); }
    private static String clean(String value, int max) { if (value == null || value.trim().isEmpty()) return null; String result = value.trim(); if (result.length() > max) throw new AccountException("invalid_device", "设备信息过长"); return result; }
    private static String bearer(String authorization) {
        if (authorization == null || !authorization.startsWith("Bearer ") || authorization.length() > 4096) throw AccountException.unauthorized();
        String value = authorization.substring(7).trim(); if (value.isEmpty()) throw AccountException.unauthorized(); return value;
    }
    private static String hashToken(String token) { try { return Base64.getEncoder().encodeToString(MessageDigest.getInstance("SHA-256").digest(token.getBytes(StandardCharsets.UTF_8))); } catch (GeneralSecurityException exception) { throw new IllegalStateException(exception); } }
    private static boolean verifyPassword(String password, String salt, String expected) {
        if (password == null || password.length() > 256 || salt == null || expected == null) return false;
        return MessageDigest.isEqual(hashPassword(password, salt).getBytes(StandardCharsets.UTF_8), expected.getBytes(StandardCharsets.UTF_8));
    }
    private static String hashPassword(String password, String salt) {
        try { PBEKeySpec spec = new PBEKeySpec(password.toCharArray(), Base64.getDecoder().decode(salt), PBKDF2_ITERATIONS, 256);
            return Base64.getEncoder().encodeToString(SecretKeyFactory.getInstance("PBKDF2WithHmacSHA256").generateSecret(spec).getEncoded());
        } catch (GeneralSecurityException exception) { throw new IllegalStateException(exception); }
    }
    static String hashPasswordForTest(String password, String salt) { return hashPassword(password, salt); }
    private void checkRateLimit(String email) { LoginAttempts value = attempts.get(email); if (value != null && value.blockedUntil > System.currentTimeMillis()) throw new AccountException("login_rate_limited", "登录失败次数过多，请稍后重试"); }
    private void recordFailure(String email) { LoginAttempts value = attempts.computeIfAbsent(email, ignored -> new LoginAttempts()); synchronized (value) { if (++value.count >= 5) value.blockedUntil = System.currentTimeMillis() + 15 * 60_000L; } }
    private static final class LoginAttempts { int count; long blockedUntil; }
    private static AccountException invalidLogin() { return AccountException.unauthorized(); }
}
