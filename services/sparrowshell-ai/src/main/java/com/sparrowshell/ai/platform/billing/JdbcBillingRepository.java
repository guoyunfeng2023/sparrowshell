package com.sparrowshell.ai.platform.billing;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.dao.EmptyResultDataAccessException;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.jdbc.core.RowMapper;
import org.springframework.stereotype.Repository;
import org.springframework.transaction.annotation.Transactional;
import java.security.MessageDigest;
import java.sql.Timestamp;
import java.time.Instant;
import java.time.YearMonth;
import java.time.ZoneId;
import java.util.List;
import java.util.Optional;

@Repository
@ConditionalOnProperty(prefix = "sparrowshell.platform", name = "persistence-enabled", havingValue = "true")
class JdbcBillingRepository implements BillingRepository {
    private static final ZoneId QUOTA_ZONE = ZoneId.of("Asia/Shanghai");
    private static final int FREE_QUOTA = 50;
    private final JdbcTemplate jdbc;
    JdbcBillingRepository(JdbcTemplate jdbc) { this.jdbc = jdbc; }

    private static final RowMapper<BillingModels.License> LICENSE = (rs, n) -> {
        BillingModels.License value = new BillingModels.License(); value.id = rs.getString("id");
        value.keyHash = rs.getString("key_hash"); value.keyCiphertext = rs.getString("key_ciphertext");
        value.status = rs.getString("status"); value.boundDeviceId = rs.getString("bound_device_id");
        value.deviceTokenHash = rs.getString("device_token_hash"); value.createdAt = instant(rs.getTimestamp("created_at"));
        value.updatedAt = instant(rs.getTimestamp("updated_at")); return value;
    };
    private static final RowMapper<BillingModels.LicensePeriod> PERIOD = (rs, n) -> {
        BillingModels.LicensePeriod value = new BillingModels.LicensePeriod(); value.id = rs.getString("id");
        value.licenseId = rs.getString("license_id"); value.plan = rs.getString("plan"); value.quota = rs.getInt("quota");
        value.used = rs.getInt("used"); value.startsAt = instant(rs.getTimestamp("starts_at"));
        value.endsAt = instant(rs.getTimestamp("ends_at")); value.createdAt = instant(rs.getTimestamp("created_at")); return value;
    };
    private static final RowMapper<BillingModels.Order> ORDER = (rs, n) -> {
        BillingModels.Order value = new BillingModels.Order(); value.id = rs.getString("id"); value.token = rs.getString("order_token");
        value.plan = rs.getString("plan"); value.amount = rs.getBigDecimal("amount"); value.status = rs.getString("status");
        value.licenseId = rs.getString("license_id"); value.qrCode = rs.getString("qr_code");
        value.qrExpiresAt = instant(rs.getTimestamp("qr_expires_at")); value.orderExpiresAt = instant(rs.getTimestamp("order_expires_at"));
        value.alipayTradeNo = rs.getString("alipay_trade_no"); value.createdAt = instant(rs.getTimestamp("created_at"));
        value.paidAt = instant(rs.getTimestamp("paid_at")); return value;
    };
    private static final RowMapper<TaskRow> TASK = (rs, n) -> {
        TaskRow value = new TaskRow(); value.taskId = rs.getString("task_id"); value.deviceId = rs.getString("device_id");
        value.periodId = rs.getString("period_id"); value.quotaMonth = rs.getString("quota_month"); value.status = rs.getString("status");
        value.expiresAt = instant(rs.getTimestamp("expires_at")); return value;
    };

    @Override @Transactional
    public BillingModels.Entitlement entitlement(String deviceId, String deviceTokenHash, Instant now) {
        releaseExpired(deviceId, now);
        return currentEntitlement(deviceId, deviceTokenHash, now, true);
    }

    private BillingModels.Entitlement currentEntitlement(String deviceId, String deviceTokenHash, Instant now, boolean lock) {
        BillingModels.License license = one("SELECT * FROM ai_license WHERE bound_device_id=? AND status='active'" + (lock ? " FOR UPDATE" : ""), LICENSE, deviceId).orElse(null);
        boolean validLicense = license != null && sameHash(license.deviceTokenHash, deviceTokenHash);
        if (validLicense) {
            BillingModels.LicensePeriod period = activePeriod(license.id, now).orElse(null);
            if (period != null) {
                BillingModels.Entitlement value = new BillingModels.Entitlement(); value.plan = period.plan; value.quota = period.quota;
                value.used = period.used; value.startsAt = period.startsAt; value.endsAt = period.endsAt; value.licensed = true;
                value.bound = true; value.licenseId = license.id; value.periodId = period.id; return value;
            }
        }
        String month = YearMonth.now(QUOTA_ZONE).toString();
        jdbc.update("INSERT IGNORE INTO ai_free_quota(device_id,quota_month,quota,used,updated_at) VALUES(?,?,?,0,?)", deviceId, month, FREE_QUOTA, timestamp(now));
        jdbc.queryForObject("SELECT device_id FROM ai_free_quota WHERE device_id=? AND quota_month=?" + (lock ? " FOR UPDATE" : ""), String.class, deviceId, month);
        Integer used = jdbc.queryForObject("SELECT used FROM ai_free_quota WHERE device_id=? AND quota_month=?", Integer.class, deviceId, month);
        BillingModels.Entitlement value = new BillingModels.Entitlement(); value.plan = "free"; value.quota = FREE_QUOTA;
        value.used = used == null ? 0 : used; value.quotaMonth = month; value.licensed = false; value.bound = license != null;
        if (validLicense) {
            value.licenseId = license.id;
            BillingModels.LicensePeriod previous = latestPeriod(license.id).orElse(null);
            if (previous != null) {
                value.expired = true; value.previousPlan = previous.plan; value.previousPeriodEnd = previous.endsAt;
            }
        }
        return value;
    }

    @Override @Transactional
    public BillingModels.TaskReservation reserveTask(String deviceId, String deviceTokenHash, String taskId,
                                                       Instant now, Instant expiresAt) {
        releaseExpired(deviceId, now);
        TaskRow existing = taskForUpdate(deviceId, taskId);
        if (existing != null && "succeeded".equals(existing.status))
            throw new BillingException("duplicate_task", "该 AI 任务已经完成，请开始新任务");
        if (existing != null && "reserved".equals(existing.status)) {
            jdbc.update("UPDATE ai_task_usage SET updated_at=?,expires_at=? WHERE task_id=? AND device_id=? AND status='reserved'",
                    timestamp(now), timestamp(expiresAt), taskId, deviceId);
            BillingModels.TaskReservation result = new BillingModels.TaskReservation(); result.taskId = taskId; result.deviceId = deviceId;
            result.status = "reserved"; result.expiresAt = expiresAt; return result;
        }
        BillingModels.Entitlement quota = currentEntitlement(deviceId, deviceTokenHash, now, true);
        if (quota.remaining() <= 0) throw new BillingException("quota_exceeded", "本周期 AI 次数已用完，请充值或等待下个周期");
        if (quota.licensed) {
            jdbc.update("UPDATE ai_license_period SET used=used+1 WHERE id=? AND used<quota", quota.periodId);
        } else {
            jdbc.update("UPDATE ai_free_quota SET used=used+1,updated_at=? WHERE device_id=? AND quota_month=? AND used<quota",
                    timestamp(now), deviceId, quota.quotaMonth);
        }
        if (existing == null) {
            jdbc.update("INSERT INTO ai_task_usage(task_id,device_id,period_id,quota_month,status,created_at,updated_at,expires_at) VALUES(?,?,?,?,'reserved',?,?,?)",
                    taskId, deviceId, quota.periodId, quota.quotaMonth, timestamp(now), timestamp(now), timestamp(expiresAt));
        } else {
            jdbc.update("UPDATE ai_task_usage SET period_id=?,quota_month=?,status='reserved',updated_at=?,expires_at=? WHERE task_id=? AND device_id=?",
                    quota.periodId, quota.quotaMonth, timestamp(now), timestamp(expiresAt), taskId, deviceId);
        }
        BillingModels.TaskReservation result = new BillingModels.TaskReservation(); result.taskId = taskId; result.deviceId = deviceId;
        result.status = "reserved"; result.expiresAt = expiresAt; return result;
    }

    @Override @Transactional
    public void settleTask(String deviceId, String taskId, boolean succeeded, Instant now) {
        TaskRow task = taskForUpdate(deviceId, taskId);
        if (task == null || !"reserved".equals(task.status)) return;
        if (succeeded) {
            jdbc.update("UPDATE ai_task_usage SET status='succeeded',updated_at=? WHERE task_id=? AND device_id=? AND status='reserved'",
                    timestamp(now), taskId, deviceId);
        } else {
            releaseLocked(task, now);
        }
    }

    @Override @Transactional
    public void releaseTask(String deviceId, String taskId, Instant now) {
        TaskRow task = taskForUpdate(deviceId, taskId);
        if (task != null && "reserved".equals(task.status)) releaseLocked(task, now);
    }

    private void releaseExpired(String deviceId, Instant now) {
        List<TaskRow> tasks = jdbc.query("SELECT * FROM ai_task_usage WHERE device_id=? AND status='reserved' AND expires_at<=? FOR UPDATE",
                TASK, deviceId, timestamp(now));
        for (TaskRow task : tasks) releaseLocked(task, now);
    }

    private void releaseLocked(TaskRow task, Instant now) {
        int changed = jdbc.update("UPDATE ai_task_usage SET status='released',updated_at=? WHERE task_id=? AND device_id=? AND status='reserved'",
                timestamp(now), task.taskId, task.deviceId);
        if (changed != 1) return;
        if (task.periodId != null) jdbc.update("UPDATE ai_license_period SET used=GREATEST(0,used-1) WHERE id=?", task.periodId);
        else jdbc.update("UPDATE ai_free_quota SET used=GREATEST(0,used-1),updated_at=? WHERE device_id=? AND quota_month=?",
                timestamp(now), task.deviceId, task.quotaMonth);
    }

    private TaskRow taskForUpdate(String deviceId, String taskId) {
        return one("SELECT * FROM ai_task_usage WHERE task_id=? AND device_id=? FOR UPDATE", TASK, taskId, deviceId).orElse(null);
    }

    @Override public void insertOrder(BillingModels.Order o) {
        jdbc.update("INSERT INTO ai_billing_order(id,order_token,plan,amount,status,license_id,qr_code,qr_expires_at,order_expires_at,alipay_trade_no,created_at,paid_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
                o.id, o.token, o.plan, o.amount, o.status, o.licenseId, o.qrCode, timestamp(o.qrExpiresAt), timestamp(o.orderExpiresAt), o.alipayTradeNo, timestamp(o.createdAt), timestamp(o.paidAt));
    }
    @Override public Optional<BillingModels.Order> order(String token) { return one("SELECT * FROM ai_billing_order WHERE order_token=?", ORDER, token); }
    @Override public void saveQrCode(String token, String qrCode, Instant expiresAt) {
        if (jdbc.update("UPDATE ai_billing_order SET qr_code=?,qr_expires_at=? WHERE order_token=? AND status='pending'", qrCode, timestamp(expiresAt), token) != 1)
            throw new BillingException("order_not_pending", "订单已支付或不存在");
    }

    @Override @Transactional
    public BillingModels.Order completePayment(String token, String tradeNo, Instant paidAt,
                                                 BillingModels.License newLicense, BillingModels.LicensePeriod period,
                                                 String notificationId) {
        BillingModels.Order order = one("SELECT * FROM ai_billing_order WHERE order_token=? FOR UPDATE", ORDER, token)
                .orElseThrow(() -> new BillingException("order_not_found", "订单不存在"));
        if ("paid".equals(order.status)) return order;
        if (!"pending".equals(order.status)) throw new BillingException("order_not_pending", "订单当前不可支付");
        if (newLicense != null) {
            jdbc.update("INSERT INTO ai_license(id,key_hash,key_ciphertext,status,bound_device_id,device_token_hash,created_at,updated_at) VALUES(?,?,?,'active',NULL,NULL,?,?)",
                    newLicense.id, newLicense.keyHash, newLicense.keyCiphertext, timestamp(newLicense.createdAt), timestamp(newLicense.updatedAt));
        }
        if (period.supersedePersonalPeriods) {
            jdbc.update("UPDATE ai_license_period SET ends_at=? WHERE license_id=? AND plan='personal' AND ends_at>?",
                    timestamp(paidAt), period.licenseId, timestamp(paidAt));
        }
        jdbc.update("INSERT INTO ai_license_period(id,license_id,plan,quota,used,starts_at,ends_at,created_at) VALUES(?,?,?,?,0,?,?,?)",
                period.id, period.licenseId, period.plan, period.quota, timestamp(period.startsAt), timestamp(period.endsAt), timestamp(period.createdAt));
        jdbc.update("UPDATE ai_billing_order SET status='paid',license_id=?,alipay_trade_no=?,paid_at=? WHERE order_token=?",
                period.licenseId, tradeNo, timestamp(paidAt), token);
        if (notificationId != null && jdbc.update("INSERT IGNORE INTO ai_payment_notification(notification_id,order_token,trade_no,received_at) VALUES(?,?,?,?)",
                notificationId, token, tradeNo, timestamp(paidAt)) != 1)
            throw new BillingException("duplicate_notification", "支付通知已处理");
        return order(token).orElseThrow(() -> new BillingException("order_not_found", "订单不存在"));
    }

    @Override public Optional<BillingModels.License> licenseByKeyHash(String keyHash) { return one("SELECT * FROM ai_license WHERE key_hash=? AND status='active'", LICENSE, keyHash); }
    @Override public Optional<BillingModels.License> licenseById(String id) { return one("SELECT * FROM ai_license WHERE id=? AND status='active'", LICENSE, id); }
    @Override public Optional<BillingModels.License> licenseByBoundDevice(String deviceId, String deviceTokenHash) {
        return one("SELECT * FROM ai_license WHERE bound_device_id=? AND device_token_hash=? AND status='active'", LICENSE, deviceId, deviceTokenHash);
    }

    @Override @Transactional
    public String bindLicense(String keyHash, String deviceId, String deviceTokenHash, Instant now) {
        BillingModels.License license = one("SELECT * FROM ai_license WHERE key_hash=? AND status='active' FOR UPDATE", LICENSE, keyHash)
                .orElseThrow(() -> new BillingException("license_not_found", "许可证不存在或已停用"));
        if (license.boundDeviceId != null && !license.boundDeviceId.equals(deviceId))
            throw new BillingException("license_bound", "该许可证已绑定其他电脑，请先在原电脑解绑");
        jdbc.update("UPDATE ai_license SET bound_device_id=?,device_token_hash=?,activated_at=COALESCE(activated_at,?),updated_at=? WHERE id=?", deviceId, deviceTokenHash, timestamp(now), timestamp(now), license.id);
        return license.id;
    }

    @Override @Transactional
    public void unbindLicense(String deviceId, String deviceTokenHash, Instant now) {
        BillingModels.License license = one("SELECT * FROM ai_license WHERE bound_device_id=? AND status='active' FOR UPDATE", LICENSE, deviceId)
                .orElseThrow(() -> new BillingException("license_not_bound", "当前电脑没有已绑定的许可证"));
        if (!sameHash(license.deviceTokenHash, deviceTokenHash)) throw new BillingException("invalid_device_token", "本机许可证凭据无效，请使用许可证密钥恢复后再解绑");
        jdbc.update("UPDATE ai_license SET bound_device_id=NULL,device_token_hash=NULL,updated_at=? WHERE id=?", timestamp(now), license.id);
    }

    @Override public Optional<BillingModels.LicensePeriod> activePeriod(String licenseId, Instant now) {
        return one("SELECT * FROM ai_license_period WHERE license_id=? AND starts_at<=? AND ends_at>? ORDER BY starts_at DESC LIMIT 1" + " FOR UPDATE", PERIOD, licenseId, timestamp(now), timestamp(now));
    }
    @Override public List<BillingModels.LicensePeriod> remainingPeriods(String licenseId, Instant now) {
        return jdbc.query("SELECT * FROM ai_license_period WHERE license_id=? AND ends_at>? ORDER BY starts_at FOR UPDATE", PERIOD, licenseId, timestamp(now));
    }
    @Override public List<BillingModels.LicensePeriod> subscriptionPeriods(String licenseId) {
        return jdbc.query("SELECT * FROM ai_license_period WHERE license_id=? ORDER BY starts_at ASC,id ASC", PERIOD, licenseId);
    }
    @Override public Optional<BillingModels.LicensePeriod> latestPeriod(String licenseId) {
        return one("SELECT * FROM ai_license_period WHERE license_id=? ORDER BY ends_at DESC LIMIT 1", PERIOD, licenseId);
    }

    private <T> Optional<T> one(String sql, RowMapper<T> mapper, Object... args) {
        try { return Optional.of(jdbc.queryForObject(sql, mapper, args)); }
        catch (EmptyResultDataAccessException exception) { return Optional.empty(); }
    }
    private static boolean sameHash(String expected, String actual) {
        if (expected == null || actual == null) return false;
        return MessageDigest.isEqual(expected.getBytes(java.nio.charset.StandardCharsets.UTF_8), actual.getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    private static Timestamp timestamp(Instant value) { return value == null ? null : Timestamp.from(value); }
    private static Instant instant(Timestamp value) { return value == null ? null : value.toInstant(); }
    private static class TaskRow { String taskId, deviceId, periodId, quotaMonth, status; Instant expiresAt; }
}
