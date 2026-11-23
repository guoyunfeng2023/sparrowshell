package com.sparrowshell.ai.platform.admin;

import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.ExceptionHandler;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PatchMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestHeader;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;
import javax.servlet.http.HttpServletRequest;
import java.time.Instant;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

@RestController
@RequestMapping("/v1/admin")
class AdminController {
    private final AdminAuthService auth;
    private final AdminService service;
    AdminController(AdminAuthService auth, AdminService service) { this.auth = auth; this.service = service; }

    @PostMapping("/auth/login")
    Map<String, Object> login(@RequestBody LoginRequest request) {
        AdminAuthService.LoginResult result = auth.login(request.email, request.password);
        Map<String, Object> response = new LinkedHashMap<String, Object>();
        response.put("token", result.token); response.put("sessionId", result.sessionId); response.put("expiresAt", instant(result.expiresAt)); response.put("admin", admin(result.admin));
        return response;
    }
    @PostMapping("/auth/logout") Map<String, Object> logout(@RequestHeader(value = "Authorization", required = false) String authorization) { auth.logout(authorization); return status("ok"); }
    @GetMapping("/auth/me") Map<String, Object> me(HttpServletRequest request) { AdminModels.AdminSession session = session(request); return adminSession(session); }

    @GetMapping("/users")
    Map<String, Object> users(HttpServletRequest request, @RequestParam(required = false) String query,
                              @RequestParam(required = false) String status, @RequestParam(defaultValue = "50") int limit, @RequestParam(defaultValue = "0") int offset) {
        limit = bound(limit, 1, 100); if (offset < 0 || offset > 1_000_000) throw new AdminException("invalid_pagination", "分页参数无效");
        List<AdminModels.Account> rows = service.accounts(session(request), clean(query), clean(status), limit, offset);
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("items", rows.stream().map(AdminController::account).collect(Collectors.toList()));
        result.put("total", service.accountCount(session(request), clean(query), clean(status))); result.put("limit", limit); result.put("offset", offset); return result;
    }
    @GetMapping("/users/{id}") Map<String, Object> user(HttpServletRequest request, @PathVariable String id) { return account(service.account(session(request), id)); }
    @PatchMapping("/users/{id}/status")
    Map<String, Object> changeUser(HttpServletRequest request, @PathVariable String id, @RequestBody AccountStatusRequest body) {
        AdminService.DangerousRequest dangerous = dangerous(body);
        return action(service.changeAccountStatus(session(request), id, body.status, body.expectedRevision, dangerous));
    }
    @GetMapping("/devices")
    Map<String, Object> devices(HttpServletRequest request, @RequestParam(required = false) String accountId,
                                @RequestParam(required = false) String query, @RequestParam(required = false) String status,
                                @RequestParam(defaultValue = "100") int limit, @RequestParam(defaultValue = "0") int offset) {
        limit = bound(limit, 1, 200); if (offset < 0 || offset > 1_000_000) throw new AdminException("invalid_pagination", "分页参数无效");
        List<AdminModels.Device> rows = service.devices(session(request), clean(accountId), clean(query), clean(status), limit, offset);
        Map<String, Object> result = new LinkedHashMap<String, Object>(); result.put("items", rows.stream().map(AdminController::device).collect(Collectors.toList())); result.put("limit", limit); result.put("offset", offset); return result;
    }
    @PostMapping("/devices/{id}/revoke") Map<String, Object> revokeDevice(HttpServletRequest request, @PathVariable String id, @RequestBody AdminService.DangerousRequest body) { return action(service.revokeDevice(session(request), id, body)); }
    @PostMapping("/users/{id}/devices/revoke-all") Map<String, Object> revokeDevices(HttpServletRequest request, @PathVariable String id, @RequestBody AdminService.DangerousRequest body) { return action(service.revokeAllDevices(session(request), id, body)); }
    @GetMapping("/billing/orders")
    Map<String, Object> billingOrders(HttpServletRequest request, @RequestParam(required = false) String query,
                                      @RequestParam(required = false) String status, @RequestParam(defaultValue = "100") int limit,
                                      @RequestParam(defaultValue = "0") int offset) {
        limit = bound(limit, 1, 100); if (offset < 0 || offset > 1_000_000) throw new AdminException("invalid_pagination", "分页参数无效");
        AdminModels.AdminSession session = session(request); String cleanedQuery = clean(query); String cleanedStatus = clean(status);
        List<AdminModels.BillingOrder> rows = service.billingOrders(session, cleanedQuery, cleanedStatus, limit, offset);
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("items", rows.stream().map(AdminController::billingOrder).collect(Collectors.toList()));
        result.put("total", service.billingOrderCount(session, cleanedQuery, cleanedStatus)); result.put("limit", limit); result.put("offset", offset); return result;
    }
    @GetMapping("/billing/licenses")
    Map<String, Object> billingLicenses(HttpServletRequest request, @RequestParam(required = false) String query,
                                        @RequestParam(required = false) String status, @RequestParam(defaultValue = "100") int limit,
                                        @RequestParam(defaultValue = "0") int offset) {
        limit = bound(limit, 1, 100); if (offset < 0 || offset > 1_000_000) throw new AdminException("invalid_pagination", "分页参数无效");
        AdminModels.AdminSession session = session(request); String cleanedQuery = clean(query); String cleanedStatus = clean(status);
        List<AdminModels.BillingLicense> rows = service.billingLicenses(session, cleanedQuery, cleanedStatus, limit, offset);
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("items", rows.stream().map(AdminController::billingLicense).collect(Collectors.toList()));
        result.put("total", service.billingLicenseCount(session, cleanedQuery, cleanedStatus)); result.put("limit", limit); result.put("offset", offset); return result;
    }
    @GetMapping("/billing/subscriptions")
    Map<String, Object> billingSubscriptions(HttpServletRequest request, @RequestParam(required = false) String query,
                                             @RequestParam(required = false) String status, @RequestParam(required = false) String plan,
                                             @RequestParam(defaultValue = "100") int limit, @RequestParam(defaultValue = "0") int offset) {
        limit = bound(limit, 1, 100); if (offset < 0 || offset > 1_000_000) throw new AdminException("invalid_pagination", "分页参数无效");
        AdminModels.AdminSession session = session(request); String cleanedQuery = clean(query); String cleanedStatus = clean(status); String cleanedPlan = clean(plan);
        List<AdminModels.BillingSubscription> rows = service.billingSubscriptions(session, cleanedQuery, cleanedStatus, cleanedPlan, limit, offset);
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("items", rows.stream().map(AdminController::billingSubscription).collect(Collectors.toList()));
        result.put("total", service.billingSubscriptionCount(session, cleanedQuery, cleanedStatus, cleanedPlan)); result.put("limit", limit); result.put("offset", offset); return result;
    }
    @GetMapping("/billing/subscriptions/summary")
    Map<String, Object> billingSubscriptionSummary(HttpServletRequest request) {
        AdminModels.BillingSubscriptionSummary value = service.billingSubscriptionSummary(session(request));
        Map<String, Object> result = new LinkedHashMap<String, Object>(); result.put("total", value.total); result.put("active", value.active);
        result.put("scheduled", value.scheduled); result.put("expired", value.expired); result.put("expiring", value.expiring);
        result.put("exhausted", value.exhausted); result.put("personalActive", value.personalActive); result.put("highActive", value.highActive); return result;
    }
    @GetMapping("/audits") Map<String, Object> audits(HttpServletRequest request, @RequestParam(defaultValue = "100") int limit) {
        List<AdminModels.Audit> rows = service.audits(session(request), bound(limit, 1, 200));
        Map<String, Object> result = new LinkedHashMap<String, Object>(); result.put("items", rows.stream().map(AdminController::audit).collect(Collectors.toList())); return result;
    }

    @ExceptionHandler(AdminException.class) ResponseEntity<Map<String, Object>> error(AdminException exception) {
        HttpStatus status = HttpStatus.BAD_REQUEST;
        if ("unavailable".equals(exception.code)) status = HttpStatus.SERVICE_UNAVAILABLE;
        else if ("permission_denied".equals(exception.code) || "mfa_invalid".equals(exception.code)) status = HttpStatus.FORBIDDEN;
        else if (exception.code.endsWith("conflict") || "previous_operation_failed".equals(exception.code)) status = HttpStatus.CONFLICT;
        else if ("not_found".equals(exception.code)) status = HttpStatus.NOT_FOUND;
        else if ("invalid_credentials".equals(exception.code) || "authentication_required".equals(exception.code)) status = HttpStatus.UNAUTHORIZED;
        else if ("login_rate_limited".equals(exception.code)) status = HttpStatus.TOO_MANY_REQUESTS;
        Map<String, Object> error = new LinkedHashMap<String, Object>(); error.put("code", exception.code); error.put("message", exception.getMessage());
        return ResponseEntity.status(status).body(java.util.Collections.<String, Object>singletonMap("error", error));
    }

    private static AdminService.DangerousRequest dangerous(AccountStatusRequest body) { AdminService.DangerousRequest request = new AdminService.DangerousRequest(); request.reason = body.reason; request.mfaCode = body.mfaCode; request.operationId = body.operationId; return request; }
    private static Map<String, Object> action(AdminService.ActionResult result) {
        Map<String, Object> map = new LinkedHashMap<String, Object>(); map.put("replayed", result.replayed);
        if (result.account != null) map.put("account", account(result.account)); if (result.device != null) map.put("device", device(result.device));
        if (result.revokedCount != null) map.put("revokedCount", result.revokedCount); if (result.audit != null) map.put("audit", audit(result.audit)); return map;
    }
    private static Map<String, Object> adminSession(AdminModels.AdminSession value) {
        Map<String, Object> map = new LinkedHashMap<String, Object>(); map.put("sessionId", value.sessionId); map.put("id", value.adminId); map.put("email", value.email); map.put("role", value.role); map.put("expiresAt", instant(value.expiresAt)); return map;
    }
    private static Map<String, Object> admin(AdminModels.AdminUser value) { Map<String, Object> map = new LinkedHashMap<String, Object>(); map.put("id", value.id); map.put("email", value.email); map.put("role", value.role); map.put("lastLoginAt", instant(value.lastLoginAt)); return map; }
    private static Map<String, Object> account(AdminModels.Account value) {
        Map<String, Object> map = new LinkedHashMap<String, Object>(); map.put("id", value.id); map.put("email", value.email); map.put("nickname", value.nickname); map.put("status", value.status);
        map.put("revision", value.revision); map.put("disabledReason", value.disabledReason); map.put("createdAt", instant(value.createdAt)); map.put("lastActiveAt", instant(value.lastActiveAt)); map.put("deviceCount", value.deviceCount); return map;
    }
    private static Map<String, Object> device(AdminModels.Device value) {
        Map<String, Object> map = new LinkedHashMap<String, Object>(); map.put("sessionId", value.sessionId); map.put("accountId", value.accountId); map.put("deviceId", value.deviceId); map.put("accountEmail", value.accountEmail);
        map.put("deviceName", value.deviceName); map.put("clientVersion", value.clientVersion); map.put("status", value.status); map.put("createdAt", instant(value.createdAt)); map.put("lastSeenAt", instant(value.lastSeenAt));
        map.put("expiresAt", instant(value.expiresAt)); map.put("revokedAt", instant(value.revokedAt)); map.put("refreshGeneration", value.refreshGeneration); map.put("lastRefreshRotatedAt", instant(value.lastRefreshRotatedAt)); return map;
    }
    private static Map<String, Object> billingOrder(AdminModels.BillingOrder value) {
        Map<String, Object> map = new LinkedHashMap<String, Object>(); map.put("id", value.id); map.put("plan", value.plan);
        map.put("amount", value.amount); map.put("status", value.status); map.put("licenseId", value.licenseId);
        map.put("licenseBound", value.licenseId == null ? null : value.licenseBoundDeviceId != null);
        map.put("alipayTradeNo", value.alipayTradeNo); map.put("createdAt", instant(value.createdAt));
        map.put("paidAt", instant(value.paidAt)); map.put("expiresAt", instant(value.orderExpiresAt)); return map;
    }
    private static Map<String, Object> billingLicense(AdminModels.BillingLicense value) {
        Map<String, Object> map = new LinkedHashMap<String, Object>(); map.put("id", value.id); map.put("status", value.status);
        map.put("bound", value.boundDeviceId != null); map.put("boundDeviceId", value.boundDeviceId);
        map.put("activated", value.activatedAt != null); map.put("activatedAt", instant(value.activatedAt));
        map.put("createdAt", instant(value.createdAt)); map.put("updatedAt", instant(value.updatedAt));
        map.put("plan", value.plan); map.put("quota", value.quota); map.put("used", value.used);
        map.put("remaining", value.quota == null || value.used == null ? null : Math.max(0, value.quota - value.used));
        map.put("periodStart", instant(value.periodStart)); map.put("periodEnd", instant(value.periodEnd));
        map.put("latestPlan", value.latestPlan); map.put("latestPeriodEnd", instant(value.latestPeriodEnd)); return map;
    }
    private static Map<String, Object> billingSubscription(AdminModels.BillingSubscription value) {
        Map<String, Object> map = new LinkedHashMap<String, Object>(); map.put("id", value.id); map.put("licenseId", value.licenseId);
        map.put("bound", value.boundDeviceId != null); map.put("boundDeviceId", value.boundDeviceId); map.put("plan", value.plan);
        map.put("quota", value.quota); map.put("used", value.used); map.put("remaining", Math.max(0, value.quota - value.used));
        map.put("exhausted", value.used >= value.quota); map.put("status", value.status); map.put("autoRenew", false);
        map.put("periodStart", instant(value.periodStart)); map.put("periodEnd", instant(value.periodEnd)); map.put("createdAt", instant(value.createdAt)); return map;
    }
    private static Map<String, Object> audit(AdminModels.Audit value) {
        Map<String, Object> map = new LinkedHashMap<String, Object>(); map.put("id", value.id); map.put("operationId", value.operationId); map.put("adminId", value.adminId); map.put("adminEmail", value.adminEmail);
        map.put("action", value.action); map.put("targetType", value.targetType); map.put("targetId", value.targetId); map.put("reason", value.reason); map.put("result", value.result); map.put("errorCode", value.errorCode); map.put("createdAt", instant(value.createdAt)); return map;
    }
    private static AdminModels.AdminSession session(HttpServletRequest request) { return AdminAuthFilter.session(request); }
    private static Map<String, Object> status(String value) { return java.util.Collections.<String, Object>singletonMap("status", value); }
    private static String instant(Instant value) { return value == null ? null : value.toString(); }
    private static String clean(String value) { return value == null || value.trim().isEmpty() ? null : value.trim(); }
    private static int bound(int value, int min, int max) { if (value < min || value > max) throw new AdminException("invalid_pagination", "分页参数无效"); return value; }
    static class LoginRequest { public String email; public String password; public String mfaCode; }
    static class AccountStatusRequest { public String status; public long expectedRevision; public String reason; public String mfaCode; public String operationId; }
}
