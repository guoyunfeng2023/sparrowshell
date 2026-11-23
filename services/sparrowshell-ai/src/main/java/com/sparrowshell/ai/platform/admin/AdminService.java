package com.sparrowshell.ai.platform.admin;

import org.springframework.beans.factory.ObjectProvider;
import org.springframework.stereotype.Service;
import org.springframework.transaction.PlatformTransactionManager;
import org.springframework.transaction.support.TransactionOperations;
import org.springframework.transaction.support.TransactionTemplate;
import java.time.Instant;
import java.util.List;
import java.util.UUID;
import java.util.function.Supplier;

@Service
class AdminService {
    private final AdminRepository repository;
    private final AdminAuthService auth;
    private final TransactionOperations transactions;
    AdminService(AdminRepository repository, AdminAuthService auth, ObjectProvider<PlatformTransactionManager> transactionManager) {
        this.repository = repository; this.auth = auth;
        PlatformTransactionManager manager = transactionManager.getIfAvailable();
        this.transactions = manager == null ? null : new TransactionTemplate(manager);
    }

    List<AdminModels.Account> accounts(AdminModels.AdminSession session, String query, String status, int limit, int offset) {
        auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER");
        return repository.accounts(query, status, limit, offset);
    }
    long accountCount(AdminModels.AdminSession session, String query, String status) {
        auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER");
        return repository.accountCount(query, status);
    }
    AdminModels.Account account(AdminModels.AdminSession session, String id) {
        auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER");
        return repository.account(id).orElseThrow(() -> new AdminException("not_found", "用户不存在"));
    }
    List<AdminModels.Device> devices(AdminModels.AdminSession session, String accountId, String query, String status, int limit, int offset) {
        auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER");
        return repository.devices(accountId, query, status, limit, offset);
    }
    List<AdminModels.BillingOrder> billingOrders(AdminModels.AdminSession session, String query, String status, int limit, int offset) {
        requireBillingRead(session);
        if (status != null && !status.matches("pending|paid|expired")) throw new AdminException("invalid_status", "订单状态筛选无效");
        return repository.billingOrders(query, status, limit, offset);
    }
    long billingOrderCount(AdminModels.AdminSession session, String query, String status) {
        requireBillingRead(session);
        if (status != null && !status.matches("pending|paid|expired")) throw new AdminException("invalid_status", "订单状态筛选无效");
        return repository.billingOrderCount(query, status);
    }
    List<AdminModels.BillingLicense> billingLicenses(AdminModels.AdminSession session, String query, String status, int limit, int offset) {
        requireBillingRead(session); validateLicenseStatus(status);
        String keyHash = licenseKeyHash(query);
        return keyHash == null ? repository.billingLicenses(query, status, limit, offset)
                : repository.billingLicensesByKeyHash(keyHash, status, limit, offset);
    }
    long billingLicenseCount(AdminModels.AdminSession session, String query, String status) {
        requireBillingRead(session); validateLicenseStatus(status);
        String keyHash = licenseKeyHash(query);
        return keyHash == null ? repository.billingLicenseCount(query, status)
                : repository.billingLicenseCountByKeyHash(keyHash, status);
    }
    private void validateLicenseStatus(String status) {
        if (status != null && !status.matches("bound|unbound")) throw new AdminException("invalid_status", "许可证绑定状态筛选无效");
    }
    private String licenseKeyHash(String query) {
        String key = query == null ? "" : query.trim().toUpperCase();
        return key.matches("SS-(?:[0-9A-F]{4}-){7}[0-9A-F]{4}")
                ? com.sparrowshell.ai.platform.billing.AiTaskQuotaService.hash(key) : null;
    }
    List<AdminModels.BillingSubscription> billingSubscriptions(AdminModels.AdminSession session, String query, String status, String plan, int limit, int offset) {
        requireBillingRead(session); validateSubscriptionFilters(status, plan);
        return repository.billingSubscriptions(query, status, plan, limit, offset);
    }
    long billingSubscriptionCount(AdminModels.AdminSession session, String query, String status, String plan) {
        requireBillingRead(session); validateSubscriptionFilters(status, plan);
        return repository.billingSubscriptionCount(query, status, plan);
    }
    AdminModels.BillingSubscriptionSummary billingSubscriptionSummary(AdminModels.AdminSession session) {
        requireBillingRead(session); return repository.billingSubscriptionSummary();
    }
    private void validateSubscriptionFilters(String status, String plan) {
        if (status != null && !status.matches("active|scheduled|expired|expiring|exhausted")) throw new AdminException("invalid_status", "订阅状态筛选无效");
        if (plan != null && !plan.matches("personal|high")) throw new AdminException("invalid_plan", "订阅套餐筛选无效");
    }
    private void requireBillingRead(AdminModels.AdminSession session) {
        auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER");
    }
    List<AdminModels.Audit> audits(AdminModels.AdminSession session, int limit) {
        auth.requireRole(session, "SUPER_ADMIN", "AUDITOR");
        return repository.audits(limit);
    }

    ActionResult changeAccountStatus(AdminModels.AdminSession session, String id, String status, long revision, DangerousRequest request) {
        if (!"active".equals(status) && !"disabled".equals(status)) throw new AdminException("invalid_status", "仅支持禁用或恢复账号");
        if (revision < 1) throw new AdminException("invalid_revision", "缺少有效的用户 revision");
        auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN");
        String action = "account." + status;
        return execute(session, action, "account", id, request, () -> {
            AdminModels.Account account = repository.changeAccountStatus(id, status, request.reason, revision);
            return ActionResult.account(account, repository.auditByOperation(session.adminId, request.operationId, action).orElse(null));
        });
    }
    ActionResult revokeDevice(AdminModels.AdminSession session, String id, DangerousRequest request) {
        auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN");
        String action = "device.revoke";
        return execute(session, action, "device", id, request, () -> {
            AdminModels.Device device = repository.revokeDevice(id, "valid");
            return ActionResult.device(device, repository.auditByOperation(session.adminId, request.operationId, action).orElse(null));
        });
    }
    ActionResult revokeAllDevices(AdminModels.AdminSession session, String accountId, DangerousRequest request) {
        auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN");
        String action = "account.devices.revoke_all";
        return execute(session, action, "account", accountId, request, () -> {
            repository.account(accountId).orElseThrow(() -> new AdminException("not_found", "用户不存在"));
            int count = repository.revokeAllDevices(accountId);
            return ActionResult.count(count, repository.auditByOperation(session.adminId, request.operationId, action).orElse(null));
        });
    }

    ActionResult execute(AdminModels.AdminSession session, String action, String targetType, String targetId,
                                 DangerousRequest input, Supplier<ActionResult> operation) {
        return execute(session, action, targetType, targetId, input, operation, true);
    }

    ActionResult executeRecorded(AdminModels.AdminSession session, String action, String targetType, String targetId,
                                 DangerousRequest input, Supplier<ActionResult> operation) {
        return execute(session, action, targetType, targetId, input, operation, false);
    }

    private ActionResult execute(AdminModels.AdminSession session, String action, String targetType, String targetId,
                                 DangerousRequest input, Supplier<ActionResult> operation, boolean requireMfa) {
        validate(input);
        AdminModels.Audit previous = repository.auditByOperation(session.adminId, input.operationId, action).orElse(null);
        if (previous != null) {
            if (!previous.targetId.equals(targetId) || !previous.reason.equals(input.reason)) throw new AdminException("operation_conflict", "操作编号已用于其他请求");
            if ("failed".equals(previous.result)) throw new AdminException("previous_operation_failed", "该操作此前未成功，请使用新的操作编号重试");
            ActionResult replay = new ActionResult(); replay.audit = previous; replay.replayed = true; return replay;
        }
        if (requireMfa) auth.requireMfa(session, input.mfaCode);
        AdminModels.Audit audit = audit(session, action, targetType, targetId, input);
        try {
            return inTransaction(() -> {
                repository.insertAudit(audit);
                ActionResult result = operation.get();
                if (result.audit == null) result.audit = audit;
                return result;
            });
        } catch (RuntimeException exception) {
            String code = exception instanceof AdminException ? ((AdminException) exception).code
                    : exception instanceof com.sparrowshell.ai.platform.update.UpdateException ? ((com.sparrowshell.ai.platform.update.UpdateException) exception).code : "operation_failed";
            audit.result = "failed"; audit.errorCode = code;
            try { repository.insertAudit(audit); } catch (RuntimeException ignored) { /* Preserve the original business error. */ }
            throw exception;
        }
    }

    private <T> T inTransaction(Supplier<T> action) { return transactions == null ? action.get() : transactions.execute(status -> action.get()); }
    private void validate(DangerousRequest request) {
        if (request == null || request.reason == null || request.reason.trim().length() < 4 || request.reason.length() > 500
                || request.operationId == null || !request.operationId.matches("[A-Za-z0-9_.:-]{8,100}"))
            throw new AdminException("invalid_operation", "请提供 4–500 字操作原因和有效的操作编号");
        request.reason = request.reason.trim();
    }
    private AdminModels.Audit audit(AdminModels.AdminSession session, String action, String targetType, String targetId, DangerousRequest request) {
        AdminModels.Audit audit = new AdminModels.Audit();
        audit.id = UUID.randomUUID().toString(); audit.operationId = request.operationId; audit.adminId = session.adminId; audit.adminEmail = session.email;
        audit.action = action; audit.targetType = targetType; audit.targetId = targetId; audit.reason = request.reason; audit.result = "success"; audit.createdAt = Instant.now();
        return audit;
    }

    static final class DangerousRequest { public String reason; public String mfaCode; public String operationId; }
    static final class ActionResult {
        AdminModels.Account account; AdminModels.Device device; Integer revokedCount; AdminModels.Audit audit;
        com.sparrowshell.ai.platform.update.UpdateRelease release;
        boolean replayed;
        static ActionResult account(AdminModels.Account value, AdminModels.Audit audit) { ActionResult r = new ActionResult(); r.account = value; r.audit = audit; return r; }
        static ActionResult device(AdminModels.Device value, AdminModels.Audit audit) { ActionResult r = new ActionResult(); r.device = value; r.audit = audit; return r; }
        static ActionResult count(int value, AdminModels.Audit audit) { ActionResult r = new ActionResult(); r.revokedCount = value; r.audit = audit; return r; }
    }
}
