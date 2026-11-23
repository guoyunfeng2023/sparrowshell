package com.sparrowshell.ai.platform.billing;

import java.time.Instant;
import java.util.List;
import java.util.Optional;

interface BillingRepository {
    BillingModels.Entitlement entitlement(String deviceId, String deviceTokenHash, Instant now);
    BillingModels.TaskReservation reserveTask(String deviceId, String deviceTokenHash, String taskId,
                                               Instant now, Instant expiresAt);
    void settleTask(String deviceId, String taskId, boolean succeeded, Instant now);
    void releaseTask(String deviceId, String taskId, Instant now);

    void insertOrder(BillingModels.Order order);
    Optional<BillingModels.Order> order(String token);
    void saveQrCode(String token, String qrCode, Instant expiresAt);
    BillingModels.Order completePayment(String token, String tradeNo, Instant paidAt,
                                        BillingModels.License newLicense, BillingModels.LicensePeriod period,
                                        String notificationId);
    Optional<BillingModels.License> licenseByKeyHash(String keyHash);
    Optional<BillingModels.License> licenseById(String id);
    Optional<BillingModels.License> licenseByBoundDevice(String deviceId, String deviceTokenHash);
    String bindLicense(String keyHash, String deviceId, String deviceTokenHash, Instant now);
    void unbindLicense(String deviceId, String deviceTokenHash, Instant now);
    Optional<BillingModels.LicensePeriod> activePeriod(String licenseId, Instant now);
    List<BillingModels.LicensePeriod> remainingPeriods(String licenseId, Instant now);
    List<BillingModels.LicensePeriod> subscriptionPeriods(String licenseId);
    Optional<BillingModels.LicensePeriod> latestPeriod(String licenseId);
}
