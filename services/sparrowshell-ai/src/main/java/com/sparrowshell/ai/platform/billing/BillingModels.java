package com.sparrowshell.ai.platform.billing;

import java.math.BigDecimal;
import java.time.Instant;
import java.util.UUID;

/** Persistence-facing values. API responses are assembled by BillingService. */
public final class BillingModels {
    private BillingModels() { }

    public enum Plan {
        PERSONAL("personal", "9.90", 200), HIGH("high", "49.00", 1000);
        public final String code;
        public final BigDecimal price;
        public final int quota;
        Plan(String code, String price, int quota) {
            this.code = code; this.price = new BigDecimal(price); this.quota = quota;
        }
        public static Plan parse(String value) {
            for (Plan plan : values()) if (plan.code.equals(value)) return plan;
            throw new BillingException("invalid_plan", "套餐无效，请选择个人版或高频版");
        }
    }

    public static class Entitlement {
        public String plan;
        public int quota;
        public int used;
        public Instant startsAt;
        public Instant endsAt;
        public boolean licensed;
        public boolean bound;
        public String licenseId;
        public String periodId;
        public String quotaMonth;
        public boolean expired;
        public String previousPlan;
        public Instant previousPeriodEnd;
        public int remaining() { return Math.max(0, quota - used); }
    }

    public static class TaskReservation {
        public String taskId;
        public String deviceId;
        public String status;
        public Instant expiresAt;
        public TaskReservation copy() {
            TaskReservation value = new TaskReservation(); value.taskId = taskId; value.deviceId = deviceId;
            value.status = status; value.expiresAt = expiresAt; return value;
        }
    }

    public static class Order {
        public String id = UUID.randomUUID().toString();
        public String token;
        public String plan;
        public BigDecimal amount;
        public String status = "pending";
        public String licenseId;
        public String qrCode;
        public Instant qrExpiresAt;
        public Instant orderExpiresAt;
        public String alipayTradeNo;
        public Instant createdAt;
        public Instant paidAt;
        public Order copy() {
            Order value = new Order(); value.id = id; value.token = token; value.plan = plan; value.amount = amount;
            value.status = status; value.licenseId = licenseId; value.qrCode = qrCode; value.qrExpiresAt = qrExpiresAt;
            value.orderExpiresAt = orderExpiresAt; value.alipayTradeNo = alipayTradeNo; value.createdAt = createdAt;
            value.paidAt = paidAt; return value;
        }
    }

    public static class License {
        public String id = UUID.randomUUID().toString();
        public String keyHash;
        public String keyCiphertext;
        public String status = "active";
        public String boundDeviceId;
        public String deviceTokenHash;
        public Instant createdAt;
        public Instant updatedAt;
        public License copy() {
            License value = new License(); value.id = id; value.keyHash = keyHash; value.keyCiphertext = keyCiphertext;
            value.status = status; value.boundDeviceId = boundDeviceId; value.deviceTokenHash = deviceTokenHash;
            value.createdAt = createdAt; value.updatedAt = updatedAt; return value;
        }
    }

    public static class LicensePeriod {
        public String id = UUID.randomUUID().toString();
        public String licenseId;
        public String plan;
        public int quota;
        public int used;
        public Instant startsAt;
        public Instant endsAt;
        public Instant createdAt;
        public boolean supersedePersonalPeriods;
    }
}
