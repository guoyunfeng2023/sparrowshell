package com.sparrowshell.ai.platform.admin;

import org.springframework.beans.factory.ObjectProvider;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.dao.EmptyResultDataAccessException;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.stereotype.Service;
import java.sql.Timestamp;
import java.time.Instant;
import java.time.LocalDate;
import java.time.ZoneId;
import java.time.temporal.ChronoUnit;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** P0 read models. Every query selects explicit non-sensitive metadata columns. */
@Service
class AdminOperationsService {
    private static final ZoneId DISPLAY_ZONE = ZoneId.of("Asia/Shanghai");
    private final JdbcTemplate jdbc;
    private final AdminAuthService auth;
    private final boolean workerEnabled;
    private final boolean providerConfigured;

    AdminOperationsService(ObjectProvider<JdbcTemplate> jdbc, AdminAuthService auth,
            @Value("${sparrowshell.platform.worker.enabled:false}") boolean workerEnabled,
            @Value("${sparrowshell.deepseek.api-key:}") String providerKey) {
        this.jdbc = jdbc.getIfAvailable(); this.auth = auth; this.workerEnabled = workerEnabled;
        this.providerConfigured = providerKey != null && !providerKey.trim().isEmpty();
    }

    Map<String, Object> dashboard(AdminModels.AdminSession session, String range) {
        readAll(session); Range window = window(range); Instant now = Instant.now();
        Map<String, Object> users = new LinkedHashMap<String, Object>();
        users.put("current", number("SELECT COUNT(*) FROM app_account WHERE status<>'deleted'"));
        users.put("cumulative", number("SELECT COUNT(*) FROM app_account"));
        users.put("new", number("SELECT COUNT(*) FROM app_account WHERE created_at>=? AND created_at<?", window.from, window.to));
        users.put("active", number("SELECT COUNT(DISTINCT x.user_id) FROM (SELECT account_id AS user_id FROM app_device_session WHERE last_seen_at>=? AND last_seen_at<? UNION SELECT account_id AS user_id FROM ai_usage_ledger WHERE created_at>=? AND created_at<?) x",
                window.from, window.to, window.from, window.to));
        users.put("states", rows("SELECT status AS `status`,COUNT(*) AS `count` FROM app_account WHERE status<>'deleted' GROUP BY status ORDER BY status"));
        Map<String, Object> devices = new LinkedHashMap<String, Object>();
        devices.put("validSessions", number("SELECT COUNT(*) FROM app_device_session s JOIN app_account a ON a.account_id=s.account_id WHERE s.revoked_at IS NULL AND s.expires_at>? AND a.status='active'", now));
        devices.put("activeDevices", number("SELECT COUNT(DISTINCT device_id) FROM app_device_session WHERE last_seen_at>=? AND last_seen_at<?", window.from, window.to));
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("users", users); result.put("devices", devices);
        result.put("sync", syncAggregate(window)); result.put("ai", aiAggregate(window));
        result.put("jobs", rows("SELECT status AS `status`,COUNT(*) AS `count` FROM app_background_job GROUP BY status ORDER BY status"));
        result.put("releases", rows("SELECT state AS `status`,COUNT(*) AS `count` FROM app_update_release GROUP BY state ORDER BY state"));
        return metadata(result, window);
    }

    Map<String, Object> userDetail(AdminModels.AdminSession session, String accountId) {
        userRead(session);
        Map<String, Object> account = one("SELECT account_id AS `id`,email AS `email`,nickname AS `nickname`,status AS `status`,revision AS `revision`,disabled_reason AS `disabledReason`,created_at AS `createdAt`,last_active_at AS `lastActiveAt`,deleted_at AS `deletedAt` FROM app_account WHERE account_id=?", accountId);
        if (account == null) throw new AdminException("not_found", "用户不存在");
        Map<String, Object> identity = new LinkedHashMap<String, Object>();
        identity.put("type", "email_password"); identity.put("email", account.get("email"));
        identity.put("nickname", account.get("nickname")); identity.put("status", account.get("status"));
        identity.put("createdAt", account.get("createdAt")); identity.put("credentialFieldsExcluded", true);
        Map<String, Object> sync = one("SELECT COUNT(*) AS `entityCount`,COALESCE(MAX(change_seq),0) AS `lastChangeSeq`,MAX(updated_at) AS `lastSyncedAt`,COALESCE(SUM(deleted_at IS NOT NULL),0) AS `tombstones` FROM app_sync_entity WHERE user_id=?", accountId);
        sync.putAll(one("SELECT COUNT(*) AS `requestCount`,COALESCE(SUM(status='failed'),0) AS `failedRequests`,COALESCE(SUM(conflict_count),0) AS `conflicts`,MAX(created_at) AS `lastRequestAt` FROM app_sync_request WHERE user_id=?", accountId));
        Map<String, Object> quota = one("SELECT quota_limit AS `quotaLimit`,reserved_cost AS `reservedCost`,updated_at AS `updatedAt` FROM ai_account_quota WHERE account_id=?", accountId);
        if (quota == null) { quota = new LinkedHashMap<String, Object>(); quota.put("status", "not_configured"); }
        else quota.put("status", "configured");
        Map<String, Object> usage = one("SELECT COUNT(*) AS `requestCount`,COALESCE(SUM(estimated_cost),0) AS `estimatedCost`,COALESCE(SUM(actual_cost),0) AS `actualCost`,COALESCE(SUM(actual_cost IS NULL),0) AS `unknownCost`,MAX(created_at) AS `lastUsedAt` FROM ai_usage_ledger WHERE account_id=?", accountId);
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("account", account); result.put("identity", identity); result.put("sync", sync);
        result.put("aiQuota", quota); result.put("aiUsage", usage);
        result.put("managementRecords", rows("SELECT id AS `id`,operation_id AS `operationId`,admin_id AS `adminId`,admin_email AS `adminEmail`,action AS `action`,target_type AS `targetType`,target_id AS `targetId`,reason AS `reason`,result AS `result`,error_code AS `errorCode`,created_at AS `createdAt` FROM admin_audit WHERE target_id=? ORDER BY created_at DESC,id DESC LIMIT 50", accountId));
        result.put("generatedAt", Instant.now().toString());
        return result;
    }

    Map<String, Object> syncOverview(AdminModels.AdminSession session, String range) {
        syncRead(session); Range window = window(range);
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("summary", syncAggregate(window));
        result.put("operations", rows("SELECT operation AS `operation`,COUNT(*) AS `requests`,COALESCE(SUM(status='succeeded'),0) AS `succeeded`,COALESCE(SUM(status='failed'),0) AS `failed`,COALESCE(SUM(conflict_count),0) AS `conflicts` FROM app_sync_request WHERE created_at>=? AND created_at<? GROUP BY operation ORDER BY operation", window.from, window.to));
        result.put("errors", rows("SELECT error_code AS `errorCode`,COUNT(*) AS `count`,MAX(created_at) AS `lastOccurredAt` FROM app_sync_request WHERE created_at>=? AND created_at<? AND error_code IS NOT NULL GROUP BY error_code ORDER BY `count` DESC LIMIT 20", window.from, window.to));
        return metadata(result, window);
    }

    Map<String, Object> syncRequests(AdminModels.AdminSession session, String accountId, String status,
                                     String errorCode, int limit, int offset) {
        syncRead(session); pagination(limit, offset, 100);
        StringBuilder where = new StringBuilder(" WHERE 1=1"); List<Object> args = new ArrayList<Object>();
        if (present(accountId)) { where.append(" AND user_id=?"); args.add(accountId); }
        if (present(status)) { where.append(" AND status=?"); args.add(status); }
        if (present(errorCode)) { where.append(" AND error_code=?"); args.add(errorCode); }
        args.add(limit); args.add(offset);
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("items", rows("SELECT request_id AS `requestId`,user_id AS `accountId`,device_id AS `deviceId`,operation AS `operation`,status AS `status`,error_code AS `errorCode`,http_status AS `httpStatus`,revision AS `revision`,change_seq AS `changeSeq`,conflict_count AS `conflictCount`,cursor_expired AS `cursorExpired`,batch_size AS `batchSize`,duration_ms AS `durationMs`,client_version AS `clientVersion`,created_at AS `createdAt` FROM app_sync_request" + where + " ORDER BY created_at DESC,request_id DESC LIMIT ? OFFSET ?", args.toArray()));
        result.put("limit", limit); result.put("offset", offset); result.put("payloadExcluded", true);
        return result;
    }

    Map<String, Object> quotas(AdminModels.AdminSession session, String accountId, int limit) {
        aiRead(session); if (limit < 1 || limit > 200) throw new AdminException("invalid_pagination", "分页参数无效");
        String filter = present(accountId) ? " WHERE q.account_id=?" : "";
        List<Object> args = new ArrayList<Object>(); if (present(accountId)) args.add(accountId); args.add(limit);
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("items", rows("SELECT q.account_id AS `accountId`,a.email AS `email`,q.quota_limit AS `quotaLimit`,q.reserved_cost AS `reservedCost`,q.updated_at AS `updatedAt`,COALESCE(u.request_count,0) AS `requestCount`,COALESCE(u.estimated_cost,0) AS `estimatedCost`,COALESCE(u.unknown_usage,0) AS `unknownUsage` FROM ai_account_quota q LEFT JOIN app_account a ON a.account_id=q.account_id LEFT JOIN (SELECT account_id,COUNT(*) AS request_count,SUM(estimated_cost) AS estimated_cost,SUM(usage_status<>'reported') AS unknown_usage FROM ai_usage_ledger GROUP BY account_id) u ON u.account_id=q.account_id" + filter + " ORDER BY q.updated_at DESC,q.account_id LIMIT ?", args.toArray()));
        result.put("generatedAt", Instant.now().toString()); result.put("contentExcluded", true);
        return result;
    }

    Map<String, Object> jobs(AdminModels.AdminSession session, String status, int limit, int offset) {
        systemRead(session); pagination(limit, offset, 200);
        String filter = present(status) ? " WHERE status=?" : "";
        List<Object> args = new ArrayList<Object>(); if (present(status)) args.add(status); args.add(limit); args.add(offset);
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("items", rows("SELECT job_id AS `jobId`,job_type AS `jobType`,status AS `status`,available_at AS `availableAt`,attempts AS `attempts`,max_attempts AS `maxAttempts`,lease_owner IS NOT NULL AS `leased`,lease_until AS `leaseUntil`,last_error_code AS `lastErrorCode`,created_at AS `createdAt`,completed_at AS `completedAt` FROM app_background_job" + filter + " ORDER BY created_at DESC,job_id DESC LIMIT ? OFFSET ?", args.toArray()));
        result.put("limit", limit); result.put("offset", offset); result.put("payloadExcluded", true);
        return result;
    }

    Map<String, Object> health(AdminModels.AdminSession session) {
        systemRead(session); Instant now = Instant.now();
        List<Map<String, Object>> services = new ArrayList<Map<String, Object>>();
        services.add(service("api", "available", "serving", now));
        String databaseStatus = "available"; String databaseReason = "query_succeeded";
        try { jdbc().queryForObject("SELECT 1", Integer.class); }
        catch (RuntimeException exception) { databaseStatus = "unavailable"; databaseReason = "database_query_failed"; }
        services.add(service("mysql", databaseStatus, databaseReason, now));
        services.add(service("background_worker", workerEnabled ? "configured" : "not_configured", workerEnabled ? "scheduler_enabled" : "scheduler_disabled", now));
        services.add(service("ai_provider", providerConfigured ? "configured" : "not_configured", "configuration_only_not_a_live_probe", now));
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("services", services); result.put("generatedAt", now.toString());
        result.put("detailsExcluded", true); return result;
    }

    Map<String, Object> accessControl(AdminModels.AdminSession session) {
        auth.requireRole(session, "SUPER_ADMIN", "AUDITOR");
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("administrators", rows("SELECT id AS `id`,email AS `email`,role AS `role`,status AS `status`,created_at AS `createdAt`,last_login_at AS `lastLoginAt` FROM admin_user ORDER BY created_at,id"));
        List<Map<String, Object>> roles = new ArrayList<Map<String, Object>>();
        roles.add(role("SUPER_ADMIN", "紧急处置与角色授权，账号与设备危险操作需 MFA，全部操作保留审计"));
        roles.add(role("SUPPORT_ADMIN", "用户查看、禁用恢复和设备会话撤销"));
        roles.add(role("VIEWER", "运营指标和脱敏明细只读"));
        roles.add(role("OPS_ADMIN", "同步、服务状态和后台任务只读"));
        roles.add(role("AI_ADMIN", "AI 用量与配额只读"));
        roles.add(role("RELEASE_ADMIN", "客户端版本受控发布"));
        roles.add(role("AUDITOR", "管理员角色和不可变审计只读"));
        result.put("roles", roles); result.put("managementChangesEnabled", false);
        return result;
    }

    private Map<String, Object> syncAggregate(Range window) {
        return one("SELECT COUNT(*) AS `requests`,COALESCE(SUM(status='succeeded'),0) AS `succeeded`,COALESCE(SUM(status='failed'),0) AS `failed`,COALESCE(SUM(conflict_count),0) AS `conflicts`,COALESCE(SUM(cursor_expired),0) AS `cursorExpired`,MAX(created_at) AS `lastRequestAt` FROM app_sync_request WHERE created_at>=? AND created_at<?", window.from, window.to);
    }
    private Map<String, Object> aiAggregate(Range window) {
        return one("SELECT COUNT(*) AS `requests`,COALESCE(SUM(status='succeeded'),0) AS `succeeded`,COALESCE(SUM(status='failed'),0) AS `failed`,COALESCE(SUM(input_tokens),0) AS `inputTokens`,COALESCE(SUM(output_tokens),0) AS `outputTokens`,COALESCE(SUM(usage_status<>'reported'),0) AS `unknownUsage`,COALESCE(SUM(actual_cost),0) AS `estimatedCost` FROM ai_usage_ledger WHERE created_at>=? AND created_at<?", window.from, window.to);
    }
    private Map<String, Object> metadata(Map<String, Object> result, Range window) {
        result.put("range", window.name); result.put("from", window.from.toString()); result.put("to", window.to.toString());
        result.put("timezone", DISPLAY_ZONE.getId()); result.put("generatedAt", Instant.now().toString());
        result.put("metricVersion", "p0-v1"); return result;
    }
    private static Range window(String value) {
        Instant to = Instant.now();
        if ("today".equals(value)) {
            Instant from = LocalDate.now(DISPLAY_ZONE).atStartOfDay(DISPLAY_ZONE).toInstant();
            return new Range(value, from, to);
        }
        if ("7d".equals(value) || "30d".equals(value)) {
            int days = "7d".equals(value) ? 7 : 30;
            return new Range(value, to.minus(days, ChronoUnit.DAYS), to);
        }
        throw new AdminException("invalid_range", "仅支持 today、7d 或 30d");
    }
    private JdbcTemplate jdbc() {
        if (jdbc == null) throw new AdminException("unavailable", "管理端持久化尚未配置");
        return jdbc;
    }
    private long number(String sql, Object... args) {
        Number value = jdbc().queryForObject(sql, Number.class, toTimestamps(args)); return value == null ? 0 : value.longValue();
    }
    private Map<String, Object> one(String sql, Object... args) {
        try { return normalize(jdbc().queryForMap(sql, toTimestamps(args))); }
        catch (EmptyResultDataAccessException exception) { return null; }
    }
    private List<Map<String, Object>> rows(String sql, Object... args) {
        List<Map<String, Object>> result = new ArrayList<Map<String, Object>>();
        for (Map<String, Object> row : jdbc().queryForList(sql, toTimestamps(args))) result.add(normalize(row));
        return result;
    }
    private static Object[] toTimestamps(Object[] args) {
        Object[] converted = new Object[args.length];
        for (int i = 0; i < args.length; i++) converted[i] = args[i] instanceof Instant ? Timestamp.from((Instant) args[i]) : args[i];
        return converted;
    }
    private static Map<String, Object> normalize(Map<String, Object> source) {
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        for (Map.Entry<String, Object> entry : source.entrySet()) {
            Object value = entry.getValue();
            if (value instanceof Timestamp) value = ((Timestamp) value).toInstant().toString();
            else if (value instanceof java.time.LocalDateTime)
                value = ((java.time.LocalDateTime) value).toInstant(java.time.ZoneOffset.UTC).toString();
            result.put(entry.getKey(), value);
        }
        return result;
    }
    private static Map<String, Object> service(String name, String status, String reason, Instant checkedAt) {
        Map<String, Object> result = new LinkedHashMap<String, Object>(); result.put("name", name);
        result.put("status", status); result.put("reason", reason); result.put("checkedAt", checkedAt.toString()); return result;
    }
    private static Map<String, Object> role(String name, String description) {
        Map<String, Object> result = new LinkedHashMap<String, Object>(); result.put("name", name); result.put("description", description); return result;
    }
    private static void pagination(int limit, int offset, int maximum) {
        if (limit < 1 || limit > maximum || offset < 0 || offset > 1_000_000) throw new AdminException("invalid_pagination", "分页参数无效");
    }
    private static boolean present(String value) { return value != null && !value.trim().isEmpty(); }
    private void readAll(AdminModels.AdminSession session) { auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER", "OPS_ADMIN", "AI_ADMIN", "RELEASE_ADMIN", "AUDITOR"); }
    private void userRead(AdminModels.AdminSession session) { auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER"); }
    private void syncRead(AdminModels.AdminSession session) { auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER", "OPS_ADMIN"); }
    private void aiRead(AdminModels.AdminSession session) { auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER", "AI_ADMIN"); }
    private void systemRead(AdminModels.AdminSession session) { auth.requireRole(session, "SUPER_ADMIN", "VIEWER", "OPS_ADMIN", "AUDITOR"); }

    private static final class Range {
        final String name; final Instant from; final Instant to;
        Range(String name, Instant from, Instant to) { this.name = name; this.from = from; this.to = to; }
    }
}
