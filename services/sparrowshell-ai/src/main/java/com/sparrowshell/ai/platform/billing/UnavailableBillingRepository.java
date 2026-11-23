package com.sparrowshell.ai.platform.billing;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Repository;
import java.time.Instant;
import java.util.Optional;

@Repository
@ConditionalOnProperty(prefix = "sparrowshell.platform", name = "persistence-enabled", havingValue = "false", matchIfMissing = true)
class UnavailableBillingRepository implements BillingRepository {
    private BillingException unavailable() { return new BillingException("billing_unavailable", "AI 额度服务尚未配置"); }
    public BillingModels.Entitlement entitlement(String d, String t, Instant n) { throw unavailable(); }
    public BillingModels.TaskReservation reserveTask(String d, String t, String i, Instant n, Instant e) { throw unavailable(); }
    public void settleTask(String d, String t, boolean s, Instant n) { throw unavailable(); }
    public void releaseTask(String d, String t, Instant n) { throw unavailable(); }
    public void insertOrder(BillingModels.Order o) { throw unavailable(); }
    public Optional<BillingModels.Order> order(String t) { throw unavailable(); }
    public void saveQrCode(String t, String q, Instant e) { throw unavailable(); }
    public BillingModels.Order completePayment(String t, String r, Instant p, BillingModels.License l, BillingModels.LicensePeriod q, String n) { throw unavailable(); }
    public Optional<BillingModels.License> licenseByKeyHash(String h) { throw unavailable(); }
    public Optional<BillingModels.License> licenseById(String i) { throw unavailable(); }
    public Optional<BillingModels.License> licenseByBoundDevice(String d, String t) { throw unavailable(); }
    public String bindLicense(String h, String d, String t, Instant n) { throw unavailable(); }
    public void unbindLicense(String d, String t, Instant n) { throw unavailable(); }
    public Optional<BillingModels.LicensePeriod> activePeriod(String i, Instant n) { throw unavailable(); }
    public java.util.List<BillingModels.LicensePeriod> remainingPeriods(String i, Instant n) { throw unavailable(); }
    public java.util.List<BillingModels.LicensePeriod> subscriptionPeriods(String i) { throw unavailable(); }
    public Optional<BillingModels.LicensePeriod> latestPeriod(String i) { throw unavailable(); }
}
