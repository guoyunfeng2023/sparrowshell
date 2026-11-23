package com.sparrowshell.ai.platform.admin;

import org.springframework.stereotype.Service;
import javax.crypto.Mac;
import javax.crypto.SecretKeyFactory;
import javax.crypto.spec.PBEKeySpec;
import javax.crypto.spec.SecretKeySpec;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.GeneralSecurityException;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.Arrays;
import java.util.Base64;
import java.util.UUID;

@Service
class AdminAuthService {
    private static final SecureRandom RANDOM = new SecureRandom();
    private static final int PBKDF2_ITERATIONS = 120_000;
    private final AdminRepository repository;
    private final java.util.concurrent.ConcurrentHashMap<String, LoginAttempts> attempts = new java.util.concurrent.ConcurrentHashMap<String, LoginAttempts>();

    AdminAuthService(AdminRepository repository) { this.repository = repository; }

    LoginResult login(String email, String password) {
        String normalized = normalizeEmail(email);
        LoginAttempts attempt = attempts.get(normalized);
        if (attempt != null && attempt.blockedUntil > System.currentTimeMillis()) throw new AdminException("login_rate_limited", "登录失败次数过多，请 15 分钟后重试");
        try {
            AdminModels.AdminUser admin = repository.adminByEmail(normalized).orElseThrow(AdminAuthService::invalidLogin);
            if (!"active".equals(admin.status) || !verifyPassword(password, admin.passwordSalt, admin.passwordHash)) throw invalidLogin();
            byte[] token = new byte[32];
            RANDOM.nextBytes(token);
            String rawToken = Base64.getUrlEncoder().withoutPadding().encodeToString(token);
            String sessionId = UUID.randomUUID().toString();
            Instant expiresAt = Instant.now().plus(8, ChronoUnit.HOURS);
            repository.insertAdminSession(sessionId, admin.id, hashToken(rawToken), expiresAt);
            repository.updateAdminLogin(admin.id, Instant.now());
            attempts.remove(normalized);
            return new LoginResult(rawToken, sessionId, admin, expiresAt);
        } catch (AdminException exception) {
            if ("invalid_credentials".equals(exception.code)) recordLoginFailure(normalized);
            throw exception;
        }
    }

    private void recordLoginFailure(String email) {
        LoginAttempts attempt = attempts.computeIfAbsent(email, ignored -> new LoginAttempts());
        synchronized (attempt) {
            long now = System.currentTimeMillis();
            if (attempt.blockedUntil <= now && now - attempt.firstFailureAt > 15 * 60_000L) { attempt.count = 0; attempt.firstFailureAt = now; }
            attempt.count++;
            if (attempt.count >= 5) attempt.blockedUntil = now + 15 * 60_000L;
        }
    }
    private static final class LoginAttempts { int count; long firstFailureAt = System.currentTimeMillis(); long blockedUntil; }

    AdminModels.AdminSession authenticate(String authorization) {
        if (authorization == null || !authorization.startsWith("Bearer ") || authorization.length() > 4096) throw AdminException.unauthorized();
        String token = authorization.substring(7).trim();
        if (token.isEmpty()) throw AdminException.unauthorized();
        return repository.adminSession(hashToken(token), Instant.now()).orElseThrow(AdminException::unauthorized);
    }

    void logout(String authorization) {
        AdminModels.AdminSession session = authenticate(authorization);
        repository.revokeAdminSession(session.sessionId);
    }

    void requireRole(AdminModels.AdminSession session, String... roles) {
        for (String role : roles) if (role.equals(session.role)) return;
        throw new AdminException("permission_denied", "当前管理员角色无权执行此操作");
    }

    void requireMfa(AdminModels.AdminSession session, String code) {
        AdminModels.AdminUser admin = repository.adminById(session.adminId).orElseThrow(AdminAuthService::invalidLogin);
        if (!"active".equals(admin.status) || !verifyMfaCode(admin.mfaSecret, code, Instant.now()))
            throw new AdminException("mfa_invalid", "MFA 验证码无效，危险操作未执行");
    }

    AdminModels.AdminUser bootstrap(String email, String password, String mfaSecret) {
        if (repository.adminCount() != 0) throw new AdminException("bootstrap_closed", "已存在管理员，不能重复引导");
        if (password == null || password.length() < 12 || password.length() > 256 || mfaSecret == null || !mfaSecret.matches("[A-Z2-7]{16,128}={0,6}"))
            throw new AdminException("invalid_bootstrap", "引导密码或 MFA 密钥不符合要求");
        byte[] salt = new byte[16];
        RANDOM.nextBytes(salt);
        AdminModels.AdminUser admin = new AdminModels.AdminUser();
        admin.id = UUID.randomUUID().toString();
        admin.email = normalizeEmail(email);
        admin.passwordSalt = Base64.getEncoder().encodeToString(salt);
        admin.passwordHash = hashPassword(password, admin.passwordSalt);
        admin.mfaSecret = mfaSecret;
        admin.role = "SUPER_ADMIN";
        admin.status = "active";
        admin.createdAt = Instant.now();
        repository.insertAdmin(admin);
        return admin;
    }

    static String normalizeEmail(String email) {
        if (email == null || !email.matches("[^@\\s]+@[^@\\s]+\\.[^@\\s]+") || email.length() > 254)
            throw new AdminException("invalid_credentials", "管理员邮箱或密码无效");
        return email.trim().toLowerCase(java.util.Locale.ROOT);
    }

    static String hashToken(String token) {
        try {
            byte[] digest = MessageDigest.getInstance("SHA-256").digest(token.getBytes(StandardCharsets.UTF_8));
            return Base64.getEncoder().encodeToString(digest);
        } catch (GeneralSecurityException exception) { throw new IllegalStateException(exception); }
    }

    static boolean verifyMfaCode(String secret, String code, Instant now) {
        if (code == null || !code.matches("[0-9]{6}")) return false;
        long counter = now.getEpochSecond() / 30;
        for (long offset = -1; offset <= 1; offset++) {
            String expected = totp(secret, counter + offset);
            if (MessageDigest.isEqual(expected.getBytes(StandardCharsets.US_ASCII), code.getBytes(StandardCharsets.US_ASCII))) return true;
        }
        return false;
    }

    private static String totp(String secret, long counter) {
        try {
            byte[] key = base32(secret);
            Mac mac = Mac.getInstance("HmacSHA1");
            mac.init(new SecretKeySpec(key, "HmacSHA1"));
            byte[] hash = mac.doFinal(ByteBuffer.allocate(8).putLong(counter).array());
            int offset = hash[hash.length - 1] & 15;
            int value = ((hash[offset] & 127) << 24) | ((hash[offset + 1] & 255) << 16)
                    | ((hash[offset + 2] & 255) << 8) | (hash[offset + 3] & 255);
            return String.format(java.util.Locale.ROOT, "%06d", value % 1_000_000);
        } catch (GeneralSecurityException | IllegalArgumentException exception) { return "invalid"; }
    }

    private static byte[] base32(String value) {
        String normalized = value.replace("=", "").toUpperCase(java.util.Locale.ROOT);
        ByteBuffer output = ByteBuffer.allocate(normalized.length() * 5 / 8 + 1);
        int buffer = 0, bits = 0;
        for (char character : normalized.toCharArray()) {
            int index = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".indexOf(character);
            if (index < 0) throw new IllegalArgumentException("invalid base32");
            buffer = (buffer << 5) | index;
            bits += 5;
            if (bits >= 8) {
                bits -= 8;
                output.put((byte) ((buffer >> bits) & 255));
            }
        }
        return Arrays.copyOf(output.array(), output.position());
    }

    private static boolean verifyPassword(String password, String salt, String expected) {
        if (password == null || password.length() > 256) return false;
        return MessageDigest.isEqual(hashPassword(password, salt).getBytes(StandardCharsets.UTF_8), expected.getBytes(StandardCharsets.UTF_8));
    }

    private static String hashPassword(String password, String salt) {
        try {
            PBEKeySpec spec = new PBEKeySpec(password.toCharArray(), Base64.getDecoder().decode(salt), PBKDF2_ITERATIONS, 256);
            return Base64.getEncoder().encodeToString(SecretKeyFactory.getInstance("PBKDF2WithHmacSHA256").generateSecret(spec).getEncoded());
        } catch (GeneralSecurityException exception) { throw new IllegalStateException(exception); }
    }

    private static AdminException invalidLogin() { return new AdminException("invalid_credentials", "管理员邮箱或密码无效"); }

    static final class LoginResult {
        final String token;
        final String sessionId;
        final AdminModels.AdminUser admin;
        final Instant expiresAt;
        LoginResult(String token, String sessionId, AdminModels.AdminUser admin, Instant expiresAt) {
            this.token = token; this.sessionId = sessionId; this.admin = admin; this.expiresAt = expiresAt;
        }
    }
}
