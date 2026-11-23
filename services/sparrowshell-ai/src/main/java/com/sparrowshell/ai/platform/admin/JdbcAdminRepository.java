package com.sparrowshell.ai.platform.admin;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.dao.DuplicateKeyException;
import org.springframework.dao.EmptyResultDataAccessException;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.jdbc.core.RowMapper;
import org.springframework.stereotype.Repository;
import org.springframework.transaction.annotation.Transactional;
import java.sql.Timestamp;
import java.time.Instant;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

@Repository
@ConditionalOnProperty(prefix = "sparrowshell.platform", name = "persistence-enabled", havingValue = "true")
class JdbcAdminRepository implements AdminRepository {
    private static final String ADMIN_COLUMNS = "id,email,password_hash,password_salt,mfa_secret,role,status,created_at,last_login_at";
    private static final String ACCOUNT_COLUMNS = "a.account_id,a.email,a.nickname,a.status,a.revision,a.disabled_reason,a.created_at,a.last_active_at";
    private static final String DEVICE_COLUMNS = "s.session_id,s.account_id,s.device_id,a.email AS account_email,a.status AS account_status,s.device_name,s.client_version,s.created_at,s.last_seen_at,s.expires_at,s.revoked_at,(SELECT MAX(r.generation) FROM app_refresh_token r WHERE r.session_id=s.session_id) AS refresh_generation,(SELECT MAX(r.rotated_at) FROM app_refresh_token r WHERE r.session_id=s.session_id) AS last_refresh_rotated_at";
    private static final RowMapper<AdminModels.AdminUser> ADMIN = (rs, n) -> {
        AdminModels.AdminUser value = new AdminModels.AdminUser();
        value.id = rs.getString("id"); value.email = rs.getString("email");
        value.passwordHash = rs.getString("password_hash"); value.passwordSalt = rs.getString("password_salt");
        value.mfaSecret = rs.getString("mfa_secret"); value.role = rs.getString("role"); value.status = rs.getString("status");
        value.createdAt = instant(rs.getTimestamp("created_at")); value.lastLoginAt = instant(rs.getTimestamp("last_login_at"));
        return value;
    };
    private static final RowMapper<AdminModels.Account> ACCOUNT = (rs, n) -> {
        AdminModels.Account value = new AdminModels.Account();
        value.id = rs.getString("account_id"); value.email = rs.getString("email"); value.nickname = rs.getString("nickname");
        value.status = rs.getString("status"); value.revision = rs.getLong("revision"); value.disabledReason = rs.getString("disabled_reason");
        value.createdAt = instant(rs.getTimestamp("created_at")); value.lastActiveAt = instant(rs.getTimestamp("last_active_at"));
        value.deviceCount = rs.getLong("device_count");
        return value;
    };
    private static final RowMapper<AdminModels.Device> DEVICE = (rs, n) -> {
        AdminModels.Device value = new AdminModels.Device();
        value.sessionId = rs.getString("session_id"); value.accountId = rs.getString("account_id"); value.deviceId = rs.getString("device_id");
        value.accountEmail = rs.getString("account_email"); value.deviceName = rs.getString("device_name"); value.clientVersion = rs.getString("client_version");
        value.createdAt = instant(rs.getTimestamp("created_at")); value.lastSeenAt = instant(rs.getTimestamp("last_seen_at")); value.revokedAt = instant(rs.getTimestamp("revoked_at"));
        value.expiresAt = instant(rs.getTimestamp("expires_at")); value.refreshGeneration = (Long) rs.getObject("refresh_generation");
        value.lastRefreshRotatedAt = instant(rs.getTimestamp("last_refresh_rotated_at"));
        String accountStatus = rs.getString("account_status");
        value.status = "deleting".equals(accountStatus) || "deleted".equals(accountStatus) ? "deleting" : value.revokedAt != null ? "revoked" : value.expiresAt != null && !value.expiresAt.isAfter(Instant.now()) ? "expired" : "valid";
        return value;
    };
    private static final RowMapper<AdminModels.Audit> AUDIT = (rs, n) -> {
        AdminModels.Audit value = new AdminModels.Audit();
        value.id = rs.getString("id"); value.operationId = rs.getString("operation_id"); value.adminId = rs.getString("admin_id");
        value.adminEmail = rs.getString("admin_email"); value.action = rs.getString("action"); value.targetType = rs.getString("target_type");
        value.targetId = rs.getString("target_id"); value.reason = rs.getString("reason"); value.result = rs.getString("result");
        value.errorCode = rs.getString("error_code"); value.createdAt = instant(rs.getTimestamp("created_at"));
        return value;
    };
    private final JdbcTemplate jdbc;
    JdbcAdminRepository(JdbcTemplate jdbc) { this.jdbc = jdbc; }

    @Override public Optional<AdminModels.AdminUser> adminByEmail(String email) { return one("SELECT " + ADMIN_COLUMNS + " FROM admin_user WHERE email=?", ADMIN, email); }
    @Override public Optional<AdminModels.AdminUser> adminById(String id) { return one("SELECT " + ADMIN_COLUMNS + " FROM admin_user WHERE id=?", ADMIN, id); }
    @Override public int adminCount() { return jdbc.queryForObject("SELECT COUNT(*) FROM admin_user", Integer.class); }
    @Override public void insertAdmin(AdminModels.AdminUser admin) {
        try { jdbc.update("INSERT INTO admin_user(" + ADMIN_COLUMNS + ") VALUES(?,?,?,?,?,?,?,?,?)", admin.id, admin.email, admin.passwordHash, admin.passwordSalt,
                admin.mfaSecret, admin.role, admin.status, timestamp(admin.createdAt), timestamp(admin.lastLoginAt)); }
        catch (DuplicateKeyException exception) { throw new AdminException("bootstrap_closed", "管理员邮箱已存在"); }
    }
    @Override public void updateAdminLogin(String id, Instant at) { jdbc.update("UPDATE admin_user SET last_login_at=? WHERE id=?", timestamp(at), id); }
    @Override public void insertAdminSession(String sessionId, String adminId, String tokenHash, Instant expiresAt) {
        jdbc.update("INSERT INTO admin_session(session_id,admin_id,token_hash,created_at,expires_at) VALUES(?,?,?,?,?)", sessionId, adminId, tokenHash, timestamp(Instant.now()), timestamp(expiresAt));
    }
    @Override public Optional<AdminModels.AdminSession> adminSession(String tokenHash, Instant now) {
        String sql = "SELECT s.session_id,s.admin_id,s.token_hash,u.role,u.email,s.expires_at FROM admin_session s JOIN admin_user u ON u.id=s.admin_id WHERE s.token_hash=? AND s.revoked_at IS NULL AND s.expires_at>? AND u.status='active'";
        return one(sql, (rs, n) -> {
            AdminModels.AdminSession value = new AdminModels.AdminSession();
            value.sessionId = rs.getString("session_id"); value.adminId = rs.getString("admin_id"); value.tokenHash = rs.getString("token_hash");
            value.role = rs.getString("role"); value.email = rs.getString("email"); value.expiresAt = instant(rs.getTimestamp("expires_at")); return value;
        }, tokenHash, timestamp(now));
    }
    @Override public void revokeAdminSession(String sessionId) { jdbc.update("UPDATE admin_session SET revoked_at=? WHERE session_id=? AND revoked_at IS NULL", timestamp(Instant.now()), sessionId); }

    @Override @Transactional(readOnly = true) public List<AdminModels.Account> accounts(String query, String status, int limit, int offset) {
        StringBuilder sql = accountSelect().append(" WHERE 1=1"); List<Object> args = accountFilters(query, status, sql);
        sql.append(" ORDER BY a.created_at DESC LIMIT ? OFFSET ?"); args.add(limit); args.add(offset);
        return jdbc.query(sql.toString(), ACCOUNT, args.toArray());
    }
    @Override @Transactional(readOnly = true) public long accountCount(String query, String status) {
        StringBuilder sql = new StringBuilder("SELECT COUNT(*) FROM app_account a WHERE 1=1"); List<Object> args = accountFilters(query, status, sql);
        return jdbc.queryForObject(sql.toString(), Long.class, args.toArray());
    }
    @Override @Transactional(readOnly = true) public Optional<AdminModels.Account> account(String id) {
        return one(accountSelect().append(" WHERE a.account_id=?").toString(), ACCOUNT, id);
    }
    @Override @Transactional public AdminModels.Account changeAccountStatus(String id, String status, String reason, long expectedRevision) {
        AdminModels.Account current = accountForUpdate(id);
        if (current.revision != expectedRevision) throw AdminException.conflict();
        if (!current.status.equals(status)) {
            int changed = jdbc.update("UPDATE app_account SET status=?,disabled_reason=?,revision=revision+1 WHERE account_id=? AND revision=? AND status=?",
                    status, "disabled".equals(status) ? reason : null, id, expectedRevision, current.status);
            if (changed != 1) throw AdminException.conflict();
            if ("disabled".equals(status)) revokeAllDevices(id);
        }
        return account(id).orElseThrow(() -> new AdminException("not_found", "用户不存在"));
    }
    @Override @Transactional(readOnly = true) public List<AdminModels.Device> devices(String accountId, String query, String status, int limit, int offset) {
        StringBuilder sql = new StringBuilder("SELECT " + DEVICE_COLUMNS + " FROM app_device_session s JOIN app_account a ON a.account_id=s.account_id WHERE 1=1");
        List<Object> args = deviceFilters(accountId, query, status, sql);
        sql.append(" ORDER BY s.created_at DESC LIMIT ? OFFSET ?"); args.add(limit); args.add(offset);
        return jdbc.query(sql.toString(), DEVICE, args.toArray());
    }
    @Override public Optional<AdminModels.Device> device(String sessionId) {
        return one("SELECT " + DEVICE_COLUMNS + " FROM app_device_session s JOIN app_account a ON a.account_id=s.account_id WHERE s.session_id=?", DEVICE, sessionId);
    }
    @Override @Transactional public AdminModels.Device revokeDevice(String sessionId, String expectedStatus) {
        AdminModels.Device current = device(sessionId).orElseThrow(() -> new AdminException("not_found", "设备会话不存在"));
        if (!"valid".equals(expectedStatus) || current.revokedAt != null) throw new AdminException("state_conflict", "设备会话状态已变化，请刷新后重试");
        Instant now = Instant.now();
        int changed = jdbc.update("UPDATE app_device_session SET revoked_at=? WHERE session_id=? AND revoked_at IS NULL", timestamp(now), sessionId);
        if (changed != 1) throw new AdminException("state_conflict", "设备会话状态已变化，请刷新后重试");
        jdbc.update("UPDATE app_refresh_token SET revoked_at=COALESCE(revoked_at,?) WHERE session_id=?", timestamp(now), sessionId);
        return device(sessionId).orElseThrow(() -> new AdminException("not_found", "设备会话不存在"));
    }
    @Override @Transactional public int revokeAllDevices(String accountId) {
        Instant now = Instant.now();
        int count = jdbc.update("UPDATE app_device_session SET revoked_at=? WHERE account_id=? AND revoked_at IS NULL", timestamp(now), accountId);
        jdbc.update("UPDATE app_refresh_token SET revoked_at=COALESCE(revoked_at,?) WHERE session_id IN (SELECT session_id FROM app_device_session WHERE account_id=?)", timestamp(now), accountId);
        return count;
    }
    private static final RowMapper<AdminModels.BillingOrder> BILLING_ORDER = (rs, n) -> {
        AdminModels.BillingOrder value = new AdminModels.BillingOrder();
        value.id = rs.getString("id"); value.plan = rs.getString("plan"); value.amount = rs.getBigDecimal("amount");
        value.status = rs.getString("display_status"); value.licenseId = rs.getString("license_id");
        value.licenseBoundDeviceId = rs.getString("license_bound_device_id");
        value.alipayTradeNo = rs.getString("alipay_trade_no"); value.createdAt = instant(rs.getTimestamp("created_at"));
        value.paidAt = instant(rs.getTimestamp("paid_at")); value.orderExpiresAt = instant(rs.getTimestamp("order_expires_at"));
        return value;
    };
    private static final RowMapper<AdminModels.BillingLicense> BILLING_LICENSE = (rs, n) -> {
        AdminModels.BillingLicense value = new AdminModels.BillingLicense();
        value.id = rs.getString("id"); value.status = rs.getString("status"); value.boundDeviceId = rs.getString("bound_device_id");
        value.createdAt = instant(rs.getTimestamp("created_at")); value.updatedAt = instant(rs.getTimestamp("updated_at"));
        value.activatedAt = instant(rs.getTimestamp("activated_at"));
        value.plan = rs.getString("period_plan"); value.quota = (Integer) rs.getObject("period_quota"); value.used = (Integer) rs.getObject("period_used");
        value.periodStart = instant(rs.getTimestamp("period_starts_at")); value.periodEnd = instant(rs.getTimestamp("period_ends_at"));
        value.latestPlan = rs.getString("latest_plan"); value.latestPeriodEnd = instant(rs.getTimestamp("latest_period_end"));
        return value;
    };
    private static final String BILLING_ORDER_SELECT = "SELECT o.id,o.plan,o.amount,CASE WHEN o.status='pending' AND o.order_expires_at<=UTC_TIMESTAMP(6) THEN 'expired' ELSE o.status END AS display_status,o.license_id,l.bound_device_id AS license_bound_device_id,o.alipay_trade_no,o.created_at,o.paid_at,o.order_expires_at FROM ai_billing_order o LEFT JOIN ai_license l ON l.id=o.license_id";
    private static final String BILLING_LICENSE_SELECT = "SELECT l.id,l.status,l.bound_device_id,l.created_at,l.updated_at,l.activated_at,p.plan AS period_plan,p.quota AS period_quota,p.used AS period_used,p.starts_at AS period_starts_at,p.ends_at AS period_ends_at,latest.plan AS latest_plan,latest.ends_at AS latest_period_end FROM ai_license l LEFT JOIN ai_license_period p ON p.id=(SELECT active.id FROM ai_license_period active WHERE active.license_id=l.id AND active.starts_at<=UTC_TIMESTAMP(6) AND active.ends_at>UTC_TIMESTAMP(6) ORDER BY active.starts_at DESC LIMIT 1) LEFT JOIN ai_license_period latest ON latest.id=(SELECT recent.id FROM ai_license_period recent WHERE recent.license_id=l.id ORDER BY recent.ends_at DESC LIMIT 1)";

    @Override @Transactional(readOnly = true) public List<AdminModels.BillingOrder> billingOrders(String query, String status, int limit, int offset) {
        StringBuilder sql = new StringBuilder(BILLING_ORDER_SELECT).append(" WHERE 1=1"); List<Object> args = billingOrderFilters(query, status, sql);
        sql.append(" ORDER BY o.created_at DESC,o.id DESC LIMIT ? OFFSET ?"); args.add(limit); args.add(offset);
        return jdbc.query(sql.toString(), BILLING_ORDER, args.toArray());
    }
    @Override @Transactional(readOnly = true) public long billingOrderCount(String query, String status) {
        StringBuilder sql = new StringBuilder("SELECT COUNT(*) FROM ai_billing_order o WHERE 1=1"); List<Object> args = billingOrderFilters(query, status, sql);
        return jdbc.queryForObject(sql.toString(), Long.class, args.toArray());
    }
    @Override @Transactional(readOnly = true) public List<AdminModels.BillingLicense> billingLicenses(String query, String status, int limit, int offset) {
        StringBuilder sql = new StringBuilder(BILLING_LICENSE_SELECT).append(" WHERE 1=1"); List<Object> args = billingLicenseFilters(query, status, sql);
        sql.append(" ORDER BY l.created_at DESC,l.id DESC LIMIT ? OFFSET ?"); args.add(limit); args.add(offset);
        return jdbc.query(sql.toString(), BILLING_LICENSE, args.toArray());
    }
    @Override @Transactional(readOnly = true) public long billingLicenseCount(String query, String status) {
        StringBuilder sql = new StringBuilder("SELECT COUNT(*) FROM ai_license l WHERE 1=1"); List<Object> args = billingLicenseFilters(query, status, sql);
        return jdbc.queryForObject(sql.toString(), Long.class, args.toArray());
    }
    @Override @Transactional(readOnly = true) public List<AdminModels.BillingLicense> billingLicensesByKeyHash(String keyHash, String status, int limit, int offset) {
        StringBuilder sql = new StringBuilder(BILLING_LICENSE_SELECT).append(" WHERE l.key_hash=?"); List<Object> args = new ArrayList<Object>(); args.add(keyHash);
        appendLicenseStatus(status, sql, args); sql.append(" ORDER BY l.created_at DESC,l.id DESC LIMIT ? OFFSET ?"); args.add(limit); args.add(offset);
        return jdbc.query(sql.toString(), BILLING_LICENSE, args.toArray());
    }
    @Override @Transactional(readOnly = true) public long billingLicenseCountByKeyHash(String keyHash, String status) {
        StringBuilder sql = new StringBuilder("SELECT COUNT(*) FROM ai_license l WHERE l.key_hash=?"); List<Object> args = new ArrayList<Object>(); args.add(keyHash);
        appendLicenseStatus(status, sql, args); return jdbc.queryForObject(sql.toString(), Long.class, args.toArray());
    }
    private List<Object> billingOrderFilters(String query, String status, StringBuilder sql) {
        List<Object> args = new ArrayList<Object>();
        if (query != null && !query.isEmpty()) { sql.append(" AND (o.id LIKE ? OR o.alipay_trade_no LIKE ? OR o.license_id LIKE ?)"); String like = "%" + query + "%"; args.add(like); args.add(like); args.add(like); }
        if ("paid".equals(status)) sql.append(" AND o.status='paid'");
        else if ("pending".equals(status)) sql.append(" AND o.status='pending' AND o.order_expires_at>UTC_TIMESTAMP(6)");
        else if ("expired".equals(status)) sql.append(" AND o.status='pending' AND o.order_expires_at<=UTC_TIMESTAMP(6)");
        return args;
    }
    private List<Object> billingLicenseFilters(String query, String status, StringBuilder sql) {
        List<Object> args = new ArrayList<Object>();
        if (query != null && !query.isEmpty()) { sql.append(" AND (l.id LIKE ? OR l.bound_device_id LIKE ?)"); String like = "%" + query + "%"; args.add(like); args.add(like); }
        appendLicenseStatus(status, sql, args); return args;
    }
    private void appendLicenseStatus(String status, StringBuilder sql, List<Object> args) {
        if ("bound".equals(status)) sql.append(" AND l.bound_device_id IS NOT NULL");
        else if ("unbound".equals(status)) sql.append(" AND l.bound_device_id IS NULL");
    }

    private static final RowMapper<AdminModels.BillingSubscription> BILLING_SUBSCRIPTION = (rs, n) -> {
        AdminModels.BillingSubscription value = new AdminModels.BillingSubscription();
        value.id = rs.getString("period_id"); value.licenseId = rs.getString("license_id"); value.boundDeviceId = rs.getString("bound_device_id");
        value.plan = rs.getString("plan"); value.quota = rs.getInt("quota"); value.used = rs.getInt("used"); value.status = rs.getString("display_status");
        value.periodStart = instant(rs.getTimestamp("starts_at")); value.periodEnd = instant(rs.getTimestamp("ends_at")); value.createdAt = instant(rs.getTimestamp("created_at")); return value;
    };
    private static final String SUBSCRIPTION_STATUS = "CASE WHEN p.starts_at>UTC_TIMESTAMP(6) THEN 'scheduled' WHEN p.ends_at<=UTC_TIMESTAMP(6) THEN 'expired' ELSE 'active' END";
    private static final String BILLING_SUBSCRIPTION_SELECT = "SELECT p.id AS period_id,p.license_id,l.bound_device_id,p.plan,p.quota,p.used,p.starts_at,p.ends_at,p.created_at," + SUBSCRIPTION_STATUS + " AS display_status FROM ai_license_period p JOIN ai_license l ON l.id=p.license_id";

    @Override @Transactional(readOnly = true) public List<AdminModels.BillingSubscription> billingSubscriptions(String query, String status, String plan, int limit, int offset) {
        StringBuilder sql = new StringBuilder(BILLING_SUBSCRIPTION_SELECT).append(" WHERE 1=1"); List<Object> args = billingSubscriptionFilters(query, status, plan, sql);
        sql.append(" ORDER BY p.ends_at DESC,p.id DESC LIMIT ? OFFSET ?"); args.add(limit); args.add(offset);
        return jdbc.query(sql.toString(), BILLING_SUBSCRIPTION, args.toArray());
    }
    @Override @Transactional(readOnly = true) public long billingSubscriptionCount(String query, String status, String plan) {
        StringBuilder sql = new StringBuilder("SELECT COUNT(*) FROM ai_license_period p JOIN ai_license l ON l.id=p.license_id WHERE 1=1");
        List<Object> args = billingSubscriptionFilters(query, status, plan, sql); return jdbc.queryForObject(sql.toString(), Long.class, args.toArray());
    }
    @Override @Transactional(readOnly = true) public AdminModels.BillingSubscriptionSummary billingSubscriptionSummary() {
        String sql = "SELECT COUNT(*) AS total,"
                + "COALESCE(SUM(CASE WHEN p.starts_at<=UTC_TIMESTAMP(6) AND p.ends_at>UTC_TIMESTAMP(6) THEN 1 ELSE 0 END),0) AS active,"
                + "COALESCE(SUM(CASE WHEN p.starts_at>UTC_TIMESTAMP(6) THEN 1 ELSE 0 END),0) AS scheduled,"
                + "COALESCE(SUM(CASE WHEN p.ends_at<=UTC_TIMESTAMP(6) THEN 1 ELSE 0 END),0) AS expired,"
                + "COALESCE(SUM(CASE WHEN p.starts_at<=UTC_TIMESTAMP(6) AND p.ends_at>UTC_TIMESTAMP(6) AND p.ends_at<=DATE_ADD(UTC_TIMESTAMP(6),INTERVAL 7 DAY) THEN 1 ELSE 0 END),0) AS expiring,"
                + "COALESCE(SUM(CASE WHEN p.starts_at<=UTC_TIMESTAMP(6) AND p.ends_at>UTC_TIMESTAMP(6) AND p.used>=p.quota THEN 1 ELSE 0 END),0) AS exhausted,"
                + "COALESCE(SUM(CASE WHEN p.plan='personal' AND p.starts_at<=UTC_TIMESTAMP(6) AND p.ends_at>UTC_TIMESTAMP(6) THEN 1 ELSE 0 END),0) AS personal_active,"
                + "COALESCE(SUM(CASE WHEN p.plan='high' AND p.starts_at<=UTC_TIMESTAMP(6) AND p.ends_at>UTC_TIMESTAMP(6) THEN 1 ELSE 0 END),0) AS high_active FROM ai_license_period p";
        return jdbc.queryForObject(sql, (rs, n) -> {
            AdminModels.BillingSubscriptionSummary value = new AdminModels.BillingSubscriptionSummary(); value.total = rs.getLong("total");
            value.active = rs.getLong("active"); value.scheduled = rs.getLong("scheduled"); value.expired = rs.getLong("expired");
            value.expiring = rs.getLong("expiring"); value.exhausted = rs.getLong("exhausted");
            value.personalActive = rs.getLong("personal_active"); value.highActive = rs.getLong("high_active"); return value;
        });
    }
    private List<Object> billingSubscriptionFilters(String query, String status, String plan, StringBuilder sql) {
        List<Object> args = new ArrayList<Object>();
        if (query != null && !query.isEmpty()) { sql.append(" AND (p.id LIKE ? OR p.license_id LIKE ? OR l.bound_device_id LIKE ?)"); String like = "%" + query + "%"; args.add(like); args.add(like); args.add(like); }
        if ("active".equals(status)) sql.append(" AND p.starts_at<=UTC_TIMESTAMP(6) AND p.ends_at>UTC_TIMESTAMP(6)");
        else if ("scheduled".equals(status)) sql.append(" AND p.starts_at>UTC_TIMESTAMP(6)");
        else if ("expired".equals(status)) sql.append(" AND p.ends_at<=UTC_TIMESTAMP(6)");
        else if ("expiring".equals(status)) sql.append(" AND p.starts_at<=UTC_TIMESTAMP(6) AND p.ends_at>UTC_TIMESTAMP(6) AND p.ends_at<=DATE_ADD(UTC_TIMESTAMP(6),INTERVAL 7 DAY)");
        else if ("exhausted".equals(status)) sql.append(" AND p.starts_at<=UTC_TIMESTAMP(6) AND p.ends_at>UTC_TIMESTAMP(6) AND p.used>=p.quota");
        if (plan != null && !plan.isEmpty()) { sql.append(" AND p.plan=?"); args.add(plan); }
        return args;
    }

    @Override @Transactional(readOnly = true) public List<AdminModels.Audit> audits(int limit) { return jdbc.query("SELECT * FROM admin_audit ORDER BY created_at DESC,id DESC LIMIT ?", AUDIT, limit); }
    @Override public Optional<AdminModels.Audit> auditByOperation(String adminId, String operationId, String action) { return one("SELECT * FROM admin_audit WHERE admin_id=? AND operation_id=? AND action=?", AUDIT, adminId, operationId, action); }
    @Override public void insertAudit(AdminModels.Audit audit) {
        try { jdbc.update("INSERT INTO admin_audit(id,operation_id,admin_id,admin_email,action,target_type,target_id,reason,result,error_code,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
                audit.id, audit.operationId, audit.adminId, audit.adminEmail, audit.action, audit.targetType, audit.targetId, audit.reason, audit.result, audit.errorCode, timestamp(audit.createdAt)); }
        catch (DuplicateKeyException exception) { throw new AdminException("operation_conflict", "操作编号已存在，请刷新后重试"); }
    }

    private AdminModels.Account accountForUpdate(String id) {
        return one("SELECT " + ACCOUNT_COLUMNS + ",0 AS device_count FROM app_account a WHERE a.account_id=? FOR UPDATE", ACCOUNT, id)
                .orElseThrow(() -> new AdminException("not_found", "用户不存在"));
    }
    private StringBuilder accountSelect() {
        return new StringBuilder("SELECT " + ACCOUNT_COLUMNS + ",(SELECT COUNT(*) FROM app_device_session s WHERE s.account_id=a.account_id AND s.revoked_at IS NULL AND s.expires_at>UTC_TIMESTAMP(6)) AS device_count FROM app_account a");
    }
    private List<Object> accountFilters(String query, String status, StringBuilder sql) {
        List<Object> args = new ArrayList<Object>();
        if (query != null && !query.isEmpty()) { sql.append(" AND (a.account_id LIKE ? OR a.email LIKE ? OR a.nickname LIKE ?)"); String like = "%" + query + "%"; args.add(like); args.add(like); args.add(like); }
        if (status != null && !status.isEmpty()) { sql.append(" AND a.status=?"); args.add(status); }
        return args;
    }
    private List<Object> deviceFilters(String accountId, String query, String status, StringBuilder sql) {
        List<Object> args = new ArrayList<Object>();
        if (accountId != null && !accountId.isEmpty()) { sql.append(" AND s.account_id=?"); args.add(accountId); }
        if (query != null && !query.isEmpty()) { sql.append(" AND (s.device_id LIKE ? OR s.device_name LIKE ? OR a.email LIKE ?)"); String like = "%" + query + "%"; args.add(like); args.add(like); args.add(like); }
        if ("valid".equals(status)) sql.append(" AND s.revoked_at IS NULL AND s.expires_at>UTC_TIMESTAMP(6) AND a.status='active'");
        else if ("expired".equals(status)) sql.append(" AND s.revoked_at IS NULL AND s.expires_at<=UTC_TIMESTAMP(6)");
        else if ("revoked".equals(status)) sql.append(" AND s.revoked_at IS NOT NULL");
        else if ("deleting".equals(status)) sql.append(" AND a.status IN ('deleting','deleted')");
        return args;
    }
    private <T> Optional<T> one(String sql, RowMapper<T> mapper, Object... args) {
        try { return Optional.of(jdbc.queryForObject(sql, mapper, args)); }
        catch (EmptyResultDataAccessException exception) { return Optional.empty(); }
    }
    private static Timestamp timestamp(Instant value) { return value == null ? null : Timestamp.from(value); }
    private static Instant instant(Timestamp value) { return value == null ? null : value.toInstant(); }
}
