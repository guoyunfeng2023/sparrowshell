package com.sparrowshell.ai.platform.account;

import com.fasterxml.jackson.databind.ObjectMapper;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.dao.DuplicateKeyException;
import org.springframework.dao.EmptyResultDataAccessException;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.jdbc.core.RowMapper;
import org.springframework.stereotype.Repository;
import java.sql.Timestamp;
import java.time.Instant;
import java.util.List;
import java.util.Optional;

@Repository
@ConditionalOnProperty(prefix = "sparrowshell.platform", name = "persistence-enabled", havingValue = "true")
class JdbcAccountRepository implements AccountRepository {
    private static final ObjectMapper JSON = new ObjectMapper();
    private static final String ACCOUNT = "account_id,email,nickname,password_hash,password_salt,status,created_at,last_active_at";
    private static final String DEVICE = "session_id,account_id,device_id,device_name,client_version,token_hash,created_at,expires_at,last_seen_at";
    private static final String ENTITY = "user_id,entity_type,entity_id,revision,schema_version,change_seq,deleted_at,updated_at,payload";
    private static final RowMapper<AccountModels.AccountRecord> ACCOUNT_MAPPER = (rs, n) -> {
        AccountModels.AccountRecord value = new AccountModels.AccountRecord();
        value.id = rs.getString("account_id"); value.email = rs.getString("email"); value.nickname = rs.getString("nickname");
        value.passwordHash = rs.getString("password_hash"); value.passwordSalt = rs.getString("password_salt"); value.status = rs.getString("status");
        value.createdAt = instant(rs.getTimestamp("created_at")); value.lastActiveAt = instant(rs.getTimestamp("last_active_at")); return value;
    };
    private static final RowMapper<AccountModels.DeviceSession> DEVICE_MAPPER = (rs, n) -> {
        AccountModels.DeviceSession value = new AccountModels.DeviceSession();
        value.sessionId = rs.getString("session_id"); value.accountId = rs.getString("account_id"); value.deviceId = rs.getString("device_id");
        value.deviceName = rs.getString("device_name"); value.clientVersion = rs.getString("client_version"); value.tokenHash = rs.getString("token_hash");
        value.createdAt = instant(rs.getTimestamp("created_at")); value.expiresAt = instant(rs.getTimestamp("expires_at")); value.lastSeenAt = instant(rs.getTimestamp("last_seen_at")); return value;
    };
    private static final RowMapper<AccountModels.RefreshToken> REFRESH_MAPPER = (rs, n) -> {
        AccountModels.RefreshToken value = new AccountModels.RefreshToken();
        value.tokenHash = rs.getString("token_hash"); value.sessionId = rs.getString("session_id");
        value.accountId = rs.getString("account_id"); value.deviceId = rs.getString("device_id");
        value.accountStatus = rs.getString("account_status"); value.generation = rs.getLong("generation");
        value.expiresAt = instant(rs.getTimestamp("expires_at")); value.rotatedAt = instant(rs.getTimestamp("rotated_at"));
        value.revokedAt = instant(rs.getTimestamp("revoked_at")); value.sessionRevokedAt = instant(rs.getTimestamp("session_revoked_at"));
        return value;
    };
    private static final RowMapper<AccountModels.SyncEntity> ENTITY_MAPPER = (rs, n) -> {
        AccountModels.SyncEntity value = new AccountModels.SyncEntity();
        value.userId = rs.getString("user_id"); value.entityType = rs.getString("entity_type"); value.entityId = rs.getString("entity_id");
        value.revision = rs.getLong("revision"); value.schemaVersion = rs.getInt("schema_version"); value.changeSeq = rs.getLong("change_seq");
        value.deletedAt = instant(rs.getTimestamp("deleted_at")); value.updatedAt = instant(rs.getTimestamp("updated_at")); value.payload = rs.getString("payload"); return value;
    };
    private final JdbcTemplate jdbc;
    JdbcAccountRepository(JdbcTemplate jdbc) { this.jdbc = jdbc; }

    @Override public Optional<AccountModels.AccountRecord> accountByEmail(String email) { return one("SELECT " + ACCOUNT + " FROM app_account WHERE email=? AND deleted_at IS NULL", ACCOUNT_MAPPER, email); }
    @Override public Optional<AccountModels.AccountRecord> accountById(String id) { return one("SELECT " + ACCOUNT + " FROM app_account WHERE account_id=? AND deleted_at IS NULL", ACCOUNT_MAPPER, id); }
    @Override public void insertAccount(AccountModels.AccountRecord a) {
        try { jdbc.update("INSERT INTO app_account(" + ACCOUNT + ",revision) VALUES(?,?,?,?,?,?,?,?,1)", a.id, a.email, a.nickname, a.passwordHash, a.passwordSalt, a.status, timestamp(a.createdAt), timestamp(a.lastActiveAt)); }
        catch (DuplicateKeyException exception) { throw new AccountException("email_registered", "该邮箱已注册"); }
    }
    @Override public void insertDevice(AccountModels.DeviceSession s) {
        jdbc.update("INSERT INTO app_device_session(" + DEVICE + ") VALUES(?,?,?,?,?,?,?,?,?)", s.sessionId, s.accountId, s.deviceId, s.deviceName, s.clientVersion, s.tokenHash, timestamp(s.createdAt), timestamp(s.expiresAt), timestamp(s.lastSeenAt));
    }
    @Override public void insertRefreshToken(AccountModels.RefreshToken token) {
        jdbc.update("INSERT INTO app_refresh_token(token_hash,session_id,generation,expires_at,created_at) VALUES(?,?,?,?,?)",
                token.tokenHash, token.sessionId, token.generation, timestamp(token.expiresAt), timestamp(Instant.now()));
    }
    @Override public Optional<AccountModels.DeviceSession> deviceSession(String tokenHash, Instant now) {
        return one("SELECT s.session_id,s.account_id,s.device_id,s.device_name,s.client_version,s.token_hash,s.created_at,s.expires_at,s.last_seen_at FROM app_device_session s JOIN app_account a ON a.account_id=s.account_id WHERE s.token_hash=? AND s.revoked_at IS NULL AND s.expires_at>? AND a.status='active' AND a.deleted_at IS NULL", DEVICE_MAPPER, tokenHash, timestamp(now));
    }
    @Override public Optional<AccountModels.RefreshToken> refreshToken(String tokenHash) {
        return one("SELECT r.token_hash,r.session_id,s.account_id,s.device_id,a.status AS account_status,r.generation,r.expires_at,r.rotated_at,r.revoked_at,s.revoked_at AS session_revoked_at FROM app_refresh_token r JOIN app_device_session s ON s.session_id=r.session_id JOIN app_account a ON a.account_id=s.account_id WHERE r.token_hash=?", REFRESH_MAPPER, tokenHash);
    }
    @Override @org.springframework.transaction.annotation.Transactional
    public boolean rotateRefreshToken(String previousHash, AccountModels.RefreshToken next, String accessHash, Instant accessExpiresAt, Instant now) {
        int changed = jdbc.update("UPDATE app_refresh_token SET rotated_at=? WHERE token_hash=? AND rotated_at IS NULL AND revoked_at IS NULL AND expires_at>?", timestamp(now), previousHash, timestamp(now));
        if (changed != 1) return false;
        insertRefreshToken(next);
        if (jdbc.update("UPDATE app_device_session SET token_hash=?,expires_at=?,last_seen_at=? WHERE session_id=? AND revoked_at IS NULL",
                accessHash, timestamp(accessExpiresAt), timestamp(now), next.sessionId) != 1) throw new IllegalStateException("refresh session changed during rotation");
        return true;
    }
    @Override public void touchDevice(String sessionId, Instant at) { jdbc.update("UPDATE app_device_session SET last_seen_at=? WHERE session_id=?", timestamp(at), sessionId); jdbc.update("UPDATE app_account SET last_active_at=? WHERE account_id=(SELECT account_id FROM (SELECT account_id FROM app_device_session WHERE session_id=?) AS d)", timestamp(at), sessionId); }
    @Override public void revokeDevice(String sessionId) { jdbc.update("UPDATE app_device_session SET revoked_at=? WHERE session_id=? AND revoked_at IS NULL", timestamp(Instant.now()), sessionId); }
    @Override public void revokeRefreshTokens(String sessionId, Instant at) { jdbc.update("UPDATE app_refresh_token SET revoked_at=COALESCE(revoked_at,?) WHERE session_id=?", timestamp(at), sessionId); }
    @Override public void markAccountDeleted(String accountId, Instant at) {
        jdbc.update("UPDATE app_account SET status='deleting',deleted_at=?,revision=revision+1 WHERE account_id=? AND deleted_at IS NULL", timestamp(at), accountId);
        jdbc.update("UPDATE app_device_session SET revoked_at=? WHERE account_id=? AND revoked_at IS NULL", timestamp(at), accountId);
        jdbc.update("UPDATE app_refresh_token SET revoked_at=COALESCE(revoked_at,?) WHERE session_id IN (SELECT session_id FROM app_device_session WHERE account_id=?)", timestamp(at), accountId);
    }
    @Override public void replaceVerificationCode(AccountModels.VerificationCode code) {
        jdbc.update("UPDATE app_email_verification SET used_at=? WHERE email=? AND purpose=? AND used_at IS NULL", timestamp(code.createdAt), code.email, code.purpose);
        jdbc.update("INSERT INTO app_email_verification(id,email,purpose,code_hash,attempts,created_at,expires_at,used_at) VALUES(?,?,?,?,0,?,?,NULL)",
                code.id, code.email, code.purpose, code.codeHash, timestamp(code.createdAt), timestamp(code.expiresAt));
    }
    @Override public Optional<AccountModels.VerificationCode> latestVerification(String email, String purpose) {
        return verification("SELECT id,email,purpose,code_hash,attempts,created_at,expires_at,used_at FROM app_email_verification WHERE email=? AND purpose=? ORDER BY created_at DESC,id DESC LIMIT 1", email, purpose);
    }
    @Override public Optional<AccountModels.VerificationCode> verificationForUpdate(String email, String purpose) {
        return verification("SELECT id,email,purpose,code_hash,attempts,created_at,expires_at,used_at FROM app_email_verification WHERE email=? AND purpose=? ORDER BY created_at DESC,id DESC LIMIT 1 FOR UPDATE", email, purpose);
    }
    @Override public void failVerification(String id, Instant at) {
        jdbc.update("UPDATE app_email_verification SET attempts=attempts+1,used_at=IF(attempts>=4,?,used_at) WHERE id=? AND used_at IS NULL", timestamp(at), id);
    }
    @Override public void markVerificationUsed(String id, Instant at) {
        jdbc.update("UPDATE app_email_verification SET used_at=? WHERE id=? AND used_at IS NULL", timestamp(at), id);
    }
    @Override public void updateNickname(String accountId, String nickname, Instant at) {
        jdbc.update("UPDATE app_account SET nickname=?,last_active_at=?,revision=revision+1 WHERE account_id=? AND status='active' AND deleted_at IS NULL", nickname, timestamp(at), accountId);
    }
    @Override public void updatePassword(String accountId, String passwordHash, String passwordSalt, Instant at) {
        jdbc.update("UPDATE app_account SET password_hash=?,password_salt=?,last_active_at=?,revision=revision+1 WHERE account_id=? AND status='active' AND deleted_at IS NULL", passwordHash, passwordSalt, timestamp(at), accountId);
    }
    @Override public void revokeAllSessions(String accountId, Instant at) {
        jdbc.update("UPDATE app_device_session SET revoked_at=? WHERE account_id=? AND revoked_at IS NULL", timestamp(at), accountId);
        jdbc.update("UPDATE app_refresh_token SET revoked_at=COALESCE(revoked_at,?) WHERE session_id IN (SELECT session_id FROM app_device_session WHERE account_id=?)", timestamp(at), accountId);
    }
    private Optional<AccountModels.VerificationCode> verification(String sql, Object... args) {
        return one(sql, (rs, n) -> {
            AccountModels.VerificationCode value = new AccountModels.VerificationCode();
            value.id = rs.getString("id"); value.email = rs.getString("email"); value.purpose = rs.getString("purpose");
            value.codeHash = rs.getString("code_hash"); value.attempts = rs.getInt("attempts");
            value.createdAt = instant(rs.getTimestamp("created_at")); value.expiresAt = instant(rs.getTimestamp("expires_at")); value.usedAt = instant(rs.getTimestamp("used_at"));
            return value;
        }, args);
    }
    @Override public void lockAccount(String accountId) { jdbc.queryForObject("SELECT account_id FROM app_account WHERE account_id=? AND status='active' AND deleted_at IS NULL FOR UPDATE", String.class, accountId); }
    @Override public Optional<AccountModels.SyncPushResult> receipt(String accountId, String mutationId) {
        try {
            String json = jdbc.queryForObject("SELECT response_json FROM app_sync_receipt WHERE user_id=? AND mutation_id=?", String.class, accountId, mutationId);
            return Optional.of(JSON.readValue(json, AccountModels.SyncPushResult.class));
        } catch (EmptyResultDataAccessException exception) { return Optional.empty(); }
        catch (java.io.IOException exception) { throw new IllegalStateException("Invalid sync receipt", exception); }
    }
    @Override public void insertReceipt(String accountId, String mutationId, String responseJson) { jdbc.update("INSERT INTO app_sync_receipt(user_id,mutation_id,response_json,created_at) VALUES(?,?,?,?) ON DUPLICATE KEY UPDATE response_json=VALUES(response_json)", accountId, mutationId, responseJson, timestamp(Instant.now())); }
    @Override public Optional<AccountModels.SyncEntity> entityForUpdate(String accountId, String type, String id) { return one("SELECT " + ENTITY + " FROM app_sync_entity WHERE user_id=? AND entity_type=? AND entity_id=? FOR UPDATE", ENTITY_MAPPER, accountId, type, id); }
    @Override public void saveEntity(AccountModels.SyncEntity e) {
        jdbc.update("INSERT INTO app_sync_entity(" + ENTITY + ") VALUES(?,?,?,?,?,?,?,?,?) ON DUPLICATE KEY UPDATE revision=VALUES(revision),schema_version=VALUES(schema_version),change_seq=VALUES(change_seq),deleted_at=VALUES(deleted_at),updated_at=VALUES(updated_at),payload=VALUES(payload)",
                e.userId, e.entityType, e.entityId, e.revision, e.schemaVersion, e.changeSeq, timestamp(e.deletedAt), timestamp(e.updatedAt), e.payload);
    }
    @Override public long maxChangeSeq(String accountId) { Long value = jdbc.queryForObject("SELECT COALESCE(MAX(change_seq),0) FROM app_sync_entity WHERE user_id=?", Long.class, accountId); return value == null ? 0 : value; }
    @Override public List<AccountModels.SyncEntity> changes(String accountId, long cursor, int limit) { return jdbc.query("SELECT " + ENTITY + " FROM app_sync_entity WHERE user_id=? AND change_seq>? ORDER BY change_seq LIMIT ?", ENTITY_MAPPER, accountId, cursor, limit); }
    @Override public List<AccountModels.SyncEntity> snapshot(String accountId, long cursor, int limit) { return jdbc.query("SELECT " + ENTITY + " FROM app_sync_entity WHERE user_id=? AND change_seq>? ORDER BY change_seq LIMIT ?", ENTITY_MAPPER, accountId, cursor, limit); }

    private <T> Optional<T> one(String sql, RowMapper<T> mapper, Object... args) { try { return Optional.of(jdbc.queryForObject(sql, mapper, args)); } catch (EmptyResultDataAccessException exception) { return Optional.empty(); } }
    private static Timestamp timestamp(Instant value) { return value == null ? null : Timestamp.from(value); }
    private static Instant instant(Timestamp value) { return value == null ? null : value.toInstant(); }
}
