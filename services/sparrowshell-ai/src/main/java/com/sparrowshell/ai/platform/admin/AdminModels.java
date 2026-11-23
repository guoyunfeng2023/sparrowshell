package com.sparrowshell.ai.platform.admin;

import java.math.BigDecimal;
import java.time.Instant;

final class AdminModels {
    private AdminModels() { }

    static final class AdminUser {
        String id;
        String email;
        String passwordHash;
        String passwordSalt;
        String mfaSecret;
        String role;
        String status;
        Instant createdAt;
        Instant lastLoginAt;
    }

    static final class AdminSession {
        String sessionId;
        String adminId;
        String tokenHash;
        String role;
        String email;
        Instant expiresAt;
    }

    static final class Account {
        String id;
        String email;
        String nickname;
        String status;
        long revision;
        String disabledReason;
        Instant createdAt;
        Instant lastActiveAt;
        long deviceCount;
    }

    static final class Device {
        String sessionId;
        String accountId;
        String deviceId;
        String accountEmail;
        String deviceName;
        String clientVersion;
        String status;
        Instant createdAt;
        Instant lastSeenAt;
        Instant expiresAt;
        Instant revokedAt;
        Long refreshGeneration;
        Instant lastRefreshRotatedAt;
    }

    static final class BillingOrder {
        String id;
        String plan;
        BigDecimal amount;
        String status;
        String licenseId;
        String licenseBoundDeviceId;
        String alipayTradeNo;
        Instant createdAt;
        Instant paidAt;
        Instant orderExpiresAt;
    }

    static final class BillingLicense {
        String id;
        String status;
        String boundDeviceId;
        Instant createdAt;
        Instant updatedAt;
        Instant activatedAt;
        String plan;
        Integer quota;
        Integer used;
        Instant periodStart;
        Instant periodEnd;
        String latestPlan;
        Instant latestPeriodEnd;
    }

    static final class BillingSubscription {
        String id;
        String licenseId;
        String boundDeviceId;
        String plan;
        int quota;
        int used;
        String status;
        Instant periodStart;
        Instant periodEnd;
        Instant createdAt;
    }

    static final class BillingSubscriptionSummary {
        long total;
        long active;
        long scheduled;
        long expired;
        long expiring;
        long exhausted;
        long personalActive;
        long highActive;
    }

    static final class Audit {
        String id;
        String operationId;
        String adminId;
        String adminEmail;
        String action;
        String targetType;
        String targetId;
        String reason;
        String result;
        String errorCode;
        Instant createdAt;
    }
}
