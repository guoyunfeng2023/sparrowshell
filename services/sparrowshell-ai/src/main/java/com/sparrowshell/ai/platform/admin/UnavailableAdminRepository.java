package com.sparrowshell.ai.platform.admin;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Repository;
import java.time.Instant;
import java.util.List;
import java.util.Optional;

@Repository
@ConditionalOnProperty(prefix = "sparrowshell.platform", name = "persistence-enabled", havingValue = "false", matchIfMissing = true)
class UnavailableAdminRepository implements AdminRepository {
    public Optional<AdminModels.AdminUser> adminByEmail(String email) { throw AdminException.unavailable(); }
    public Optional<AdminModels.AdminUser> adminById(String id) { throw AdminException.unavailable(); }
    public int adminCount() { throw AdminException.unavailable(); }
    public void insertAdmin(AdminModels.AdminUser admin) { throw AdminException.unavailable(); }
    public void updateAdminLogin(String id, Instant at) { throw AdminException.unavailable(); }
    public void insertAdminSession(String sessionId, String adminId, String tokenHash, Instant expiresAt) { throw AdminException.unavailable(); }
    public Optional<AdminModels.AdminSession> adminSession(String tokenHash, Instant now) { throw AdminException.unavailable(); }
    public void revokeAdminSession(String sessionId) { throw AdminException.unavailable(); }
    public List<AdminModels.Account> accounts(String query, String status, int limit, int offset) { throw AdminException.unavailable(); }
    public long accountCount(String query, String status) { throw AdminException.unavailable(); }
    public Optional<AdminModels.Account> account(String id) { throw AdminException.unavailable(); }
    public AdminModels.Account changeAccountStatus(String id, String status, String reason, long expectedRevision) { throw AdminException.unavailable(); }
    public List<AdminModels.Device> devices(String accountId, String query, String status, int limit, int offset) { throw AdminException.unavailable(); }
    public Optional<AdminModels.Device> device(String sessionId) { throw AdminException.unavailable(); }
    public AdminModels.Device revokeDevice(String sessionId, String expectedStatus) { throw AdminException.unavailable(); }
    public int revokeAllDevices(String accountId) { throw AdminException.unavailable(); }
    public List<AdminModels.BillingOrder> billingOrders(String query, String status, int limit, int offset) { throw AdminException.unavailable(); }
    public long billingOrderCount(String query, String status) { throw AdminException.unavailable(); }
    public List<AdminModels.BillingLicense> billingLicenses(String query, String status, int limit, int offset) { throw AdminException.unavailable(); }
    public long billingLicenseCount(String query, String status) { throw AdminException.unavailable(); }
    public List<AdminModels.BillingLicense> billingLicensesByKeyHash(String keyHash, String status, int limit, int offset) { throw AdminException.unavailable(); }
    public long billingLicenseCountByKeyHash(String keyHash, String status) { throw AdminException.unavailable(); }
    public List<AdminModels.BillingSubscription> billingSubscriptions(String query, String status, String plan, int limit, int offset) { throw AdminException.unavailable(); }
    public long billingSubscriptionCount(String query, String status, String plan) { throw AdminException.unavailable(); }
    public AdminModels.BillingSubscriptionSummary billingSubscriptionSummary() { throw AdminException.unavailable(); }
    public List<AdminModels.Audit> audits(int limit) { throw AdminException.unavailable(); }
    public Optional<AdminModels.Audit> auditByOperation(String adminId, String operationId, String action) { throw AdminException.unavailable(); }
    public void insertAudit(AdminModels.Audit audit) { throw AdminException.unavailable(); }
}
