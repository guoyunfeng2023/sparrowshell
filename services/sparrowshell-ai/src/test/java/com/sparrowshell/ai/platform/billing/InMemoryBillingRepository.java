package com.sparrowshell.ai.platform.billing;

import java.time.Instant;
import java.time.YearMonth;
import java.time.ZoneId;
import java.util.*;

class InMemoryBillingRepository implements BillingRepository {
    final Map<String, BillingModels.Order> orders = new HashMap<String, BillingModels.Order>();
    final Map<String, BillingModels.License> licenses = new HashMap<String, BillingModels.License>();
    final Map<String, BillingModels.LicensePeriod> periods = new HashMap<String, BillingModels.LicensePeriod>();
    final Map<String, Integer> freeUsed = new HashMap<String, Integer>();
    final Map<String, Task> tasks = new HashMap<String, Task>();
    final Set<String> notifications = new HashSet<String>();

    @Override public synchronized BillingModels.Entitlement entitlement(String deviceId, String tokenHash, Instant now) {
        releaseExpired(deviceId, now); return current(deviceId, tokenHash, now);
    }
    private BillingModels.Entitlement current(String deviceId, String tokenHash, Instant now) {
        for (BillingModels.License license : licenses.values()) if (deviceId.equals(license.boundDeviceId)) {
            BillingModels.Entitlement value = free(deviceId); value.bound = true;
            if (Objects.equals(license.deviceTokenHash, tokenHash)) {
                BillingModels.LicensePeriod period = activePeriod(license.id, now).orElse(null);
                if (period != null) {
                    value.plan = period.plan; value.quota = period.quota;
                    value.used = period.used; value.startsAt = period.startsAt; value.endsAt = period.endsAt; value.licensed = true;
                    value.licenseId = license.id; value.periodId = period.id; return value;
                }
                value.licenseId = license.id;
                BillingModels.LicensePeriod previous = latestPeriod(license.id).orElse(null);
                if (previous != null) {
                    value.expired = true; value.previousPlan = previous.plan; value.previousPeriodEnd = previous.endsAt;
                }
            }
            return value;
        }
        return free(deviceId);
    }
    private BillingModels.Entitlement free(String deviceId) {
        String month = YearMonth.now(ZoneId.of("Asia/Shanghai")).toString(); BillingModels.Entitlement value = new BillingModels.Entitlement();
        value.plan = "free"; value.quota = 50; value.used = freeUsed.getOrDefault(deviceId + month, 0); value.quotaMonth = month; return value;
    }
    @Override public synchronized BillingModels.TaskReservation reserveTask(String deviceId, String tokenHash, String taskId, Instant now, Instant expiresAt) {
        releaseExpired(deviceId, now); Task existing = tasks.get(deviceId + taskId);
        if (existing != null && "succeeded".equals(existing.status)) throw new BillingException("duplicate_task", "duplicate");
        if (existing != null && "reserved".equals(existing.status)) { existing.expiresAt = expiresAt; return reservation(taskId, deviceId, expiresAt); }
        BillingModels.Entitlement quota = current(deviceId, tokenHash, now);
        if (quota.remaining() <= 0) throw new BillingException("quota_exceeded", "exhausted");
        if (quota.licensed) periods.get(quota.periodId).used++;
        else freeUsed.put(deviceId + quota.quotaMonth, quota.used + 1);
        Task task = new Task(); task.deviceId = deviceId; task.taskId = taskId; task.periodId = quota.periodId; task.month = quota.quotaMonth;
        task.status = "reserved"; task.expiresAt = expiresAt; tasks.put(deviceId + taskId, task); return reservation(taskId, deviceId, expiresAt);
    }
    private BillingModels.TaskReservation reservation(String taskId, String deviceId, Instant expiresAt) {
        BillingModels.TaskReservation value = new BillingModels.TaskReservation(); value.taskId = taskId; value.deviceId = deviceId;
        value.status = "reserved"; value.expiresAt = expiresAt; return value;
    }
    @Override public synchronized void settleTask(String deviceId, String taskId, boolean succeeded, Instant now) {
        Task task = tasks.get(deviceId + taskId); if (task == null || !"reserved".equals(task.status)) return;
        if (succeeded) task.status = "succeeded"; else release(task);
    }
    @Override public synchronized void releaseTask(String deviceId, String taskId, Instant now) {
        Task task = tasks.get(deviceId + taskId); if (task != null && "reserved".equals(task.status)) release(task);
    }
    private void releaseExpired(String deviceId, Instant now) { for (Task task : tasks.values()) if (task.deviceId.equals(deviceId) && "reserved".equals(task.status) && !task.expiresAt.isAfter(now)) release(task); }
    private void release(Task task) { task.status = "released"; if (task.periodId != null) periods.get(task.periodId).used--; else { String key = task.deviceId + task.month; freeUsed.put(key, Math.max(0, freeUsed.getOrDefault(key, 0) - 1)); } }

    @Override public synchronized void insertOrder(BillingModels.Order order) { orders.put(order.token, order.copy()); }
    @Override public synchronized Optional<BillingModels.Order> order(String token) { BillingModels.Order value = orders.get(token); return value == null ? Optional.empty() : Optional.of(value.copy()); }
    @Override public synchronized void saveQrCode(String token, String qrCode, Instant expiresAt) { BillingModels.Order value = orders.get(token); value.qrCode = qrCode; value.qrExpiresAt = expiresAt; }
    @Override public synchronized BillingModels.Order completePayment(String token, String tradeNo, Instant paidAt, BillingModels.License license, BillingModels.LicensePeriod period, String notificationId) {
        BillingModels.Order order = orders.get(token); if ("paid".equals(order.status)) return order.copy();
        if (license != null) licenses.put(license.id, license.copy());
        if (period.supersedePersonalPeriods) for (BillingModels.LicensePeriod value : periods.values()) {
            if (value.licenseId.equals(period.licenseId) && "personal".equals(value.plan) && value.endsAt.isAfter(paidAt)) value.endsAt = paidAt;
        }
        periods.put(period.id, period); order.status = "paid"; order.licenseId = period.licenseId;
        order.alipayTradeNo = tradeNo; order.paidAt = paidAt; if (notificationId != null && !notifications.add(notificationId)) throw new BillingException("duplicate_notification", "duplicate"); return order.copy();
    }
    @Override public synchronized Optional<BillingModels.License> licenseByKeyHash(String hash) { for (BillingModels.License value : licenses.values()) if (value.keyHash.equals(hash)) return Optional.of(value.copy()); return Optional.empty(); }
    @Override public synchronized Optional<BillingModels.License> licenseById(String id) { BillingModels.License value = licenses.get(id); return value == null ? Optional.empty() : Optional.of(value.copy()); }
    @Override public synchronized Optional<BillingModels.License> licenseByBoundDevice(String deviceId, String tokenHash) {
        for (BillingModels.License value : licenses.values()) {
            if (deviceId.equals(value.boundDeviceId) && Objects.equals(value.deviceTokenHash, tokenHash) && "active".equals(value.status)) return Optional.of(value.copy());
        }
        return Optional.empty();
    }
    @Override public synchronized String bindLicense(String hash, String deviceId, String tokenHash, Instant now) {
        BillingModels.License license = licenseByKeyHash(hash).orElseThrow(() -> new BillingException("license_not_found", "missing"));
        if (license.boundDeviceId != null && !deviceId.equals(license.boundDeviceId)) throw new BillingException("license_bound", "bound");
        BillingModels.License stored = licenses.get(license.id); stored.boundDeviceId = deviceId; stored.deviceTokenHash = tokenHash; stored.updatedAt = now; return license.id;
    }
    @Override public synchronized void unbindLicense(String deviceId, String tokenHash, Instant now) {
        for (BillingModels.License value : licenses.values()) if (deviceId.equals(value.boundDeviceId)) {
            if (!Objects.equals(value.deviceTokenHash, tokenHash)) throw new BillingException("invalid_device_token", "token");
            value.boundDeviceId = null; value.deviceTokenHash = null; return;
        } throw new BillingException("license_not_bound", "missing");
    }
    @Override public synchronized Optional<BillingModels.LicensePeriod> activePeriod(String licenseId, Instant now) {
        for (BillingModels.LicensePeriod value : periods.values()) if (value.licenseId.equals(licenseId) && !value.startsAt.isAfter(now) && value.endsAt.isAfter(now)) return Optional.of(value); return Optional.empty();
    }
    @Override public synchronized List<BillingModels.LicensePeriod> remainingPeriods(String licenseId, Instant now) {
        List<BillingModels.LicensePeriod> result = new ArrayList<BillingModels.LicensePeriod>();
        for (BillingModels.LicensePeriod value : periods.values()) if (value.licenseId.equals(licenseId) && value.endsAt.isAfter(now)) result.add(value);
        return result;
    }
    @Override public synchronized List<BillingModels.LicensePeriod> subscriptionPeriods(String licenseId) {
        List<BillingModels.LicensePeriod> result = new ArrayList<BillingModels.LicensePeriod>();
        for (BillingModels.LicensePeriod value : periods.values()) if (value.licenseId.equals(licenseId)) result.add(value);
        result.sort(new Comparator<BillingModels.LicensePeriod>() {
            public int compare(BillingModels.LicensePeriod left, BillingModels.LicensePeriod right) {
                int time = left.startsAt.compareTo(right.startsAt); return time != 0 ? time : left.id.compareTo(right.id);
            }
        });
        return result;
    }
    @Override public synchronized Optional<BillingModels.LicensePeriod> latestPeriod(String licenseId) {
        BillingModels.LicensePeriod latest = null;
        for (BillingModels.LicensePeriod value : periods.values()) if (value.licenseId.equals(licenseId) && (latest == null || value.endsAt.isAfter(latest.endsAt))) latest = value;
        return Optional.ofNullable(latest);
    }
    static class Task { String deviceId, taskId, periodId, month, status; Instant expiresAt; }
}
