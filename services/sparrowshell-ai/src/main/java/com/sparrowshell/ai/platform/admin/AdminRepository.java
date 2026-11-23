package com.sparrowshell.ai.platform.admin;

import java.time.Instant;
import java.util.List;
import java.util.Optional;

interface AdminRepository {
    Optional<AdminModels.AdminUser> adminByEmail(String email);
    Optional<AdminModels.AdminUser> adminById(String id);
    int adminCount();
    void insertAdmin(AdminModels.AdminUser admin);
    void updateAdminLogin(String id, Instant at);
    void insertAdminSession(String sessionId, String adminId, String tokenHash, Instant expiresAt);
    Optional<AdminModels.AdminSession> adminSession(String tokenHash, Instant now);
    void revokeAdminSession(String sessionId);

    List<AdminModels.Account> accounts(String query, String status, int limit, int offset);
    long accountCount(String query, String status);
    Optional<AdminModels.Account> account(String id);
    AdminModels.Account changeAccountStatus(String id, String status, String reason, long expectedRevision);
    List<AdminModels.Device> devices(String accountId, String query, String status, int limit, int offset);
    Optional<AdminModels.Device> device(String sessionId);
    AdminModels.Device revokeDevice(String sessionId, String expectedStatus);
    int revokeAllDevices(String accountId);
    List<AdminModels.BillingOrder> billingOrders(String query, String status, int limit, int offset);
    long billingOrderCount(String query, String status);
    List<AdminModels.BillingLicense> billingLicenses(String query, String status, int limit, int offset);
    long billingLicenseCount(String query, String status);
    List<AdminModels.BillingLicense> billingLicensesByKeyHash(String keyHash, String status, int limit, int offset);
    long billingLicenseCountByKeyHash(String keyHash, String status);
    List<AdminModels.BillingSubscription> billingSubscriptions(String query, String status, String plan, int limit, int offset);
    long billingSubscriptionCount(String query, String status, String plan);
    AdminModels.BillingSubscriptionSummary billingSubscriptionSummary();
    List<AdminModels.Audit> audits(int limit);
    Optional<AdminModels.Audit> auditByOperation(String adminId, String operationId, String action);
    void insertAudit(AdminModels.Audit audit);
}
