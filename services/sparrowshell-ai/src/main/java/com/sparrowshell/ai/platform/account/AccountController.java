package com.sparrowshell.ai.platform.account;

import com.fasterxml.jackson.annotation.JsonProperty;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;
import javax.servlet.http.HttpServletResponse;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

@RestController
@RequestMapping("/client-api/v1")
class AccountController {
    private final AccountService service;
    private final SyncRequestMonitor monitor;
    AccountController(AccountService service, SyncRequestMonitor monitor) {
        this.service = service; this.monitor = monitor;
    }

    @PostMapping("/auth/verification-code") Map<String, String> verificationCode(@RequestBody VerificationCodeRequest request) {
        service.sendVerificationCode(request.email, request.purpose, request.locale); return Collections.singletonMap("status", "sent");
    }
    @PostMapping("/auth/register") AccountModels.TokenResponse register(@RequestBody AuthRequest request) {
        return service.register(request.email, request.password, request.code, request.deviceId, request.deviceName, request.clientVersion);
    }
    @PostMapping("/auth/login") AccountModels.TokenResponse login(@RequestBody AuthRequest request) {
        return service.login(request.email, request.password, request.deviceId, request.deviceName, request.clientVersion);
    }
    @PostMapping("/auth/password-reset") Map<String, String> resetPassword(@RequestBody PasswordResetRequest request) {
        service.resetPassword(request.email, request.code, request.newPassword); return Collections.singletonMap("status", "password_reset");
    }
    @PostMapping("/auth/refresh") AccountModels.TokenResponse refresh(@RequestBody RefreshRequest request) {
        return service.refresh(request.refreshToken, request.deviceId);
    }
    @PostMapping("/auth/logout") Map<String, String> logout(@RequestHeader(value = "Authorization", required = false) String authorization) {
        service.logout(authorization); return Collections.singletonMap("status", "ok");
    }
    @GetMapping("/account") AccountModels.ProfileResponse profile(@RequestHeader(value = "Authorization", required = false) String authorization) {
        return service.profile(authorization);
    }
    @PatchMapping("/account/profile") AccountModels.ProfileResponse updateProfile(@RequestHeader(value = "Authorization", required = false) String authorization, @RequestBody ProfileRequest request) {
        return service.updateNickname(authorization, request.nickname);
    }
    @PostMapping("/account/password") Map<String, String> changePassword(@RequestHeader(value = "Authorization", required = false) String authorization, @RequestBody PasswordChangeRequest request) {
        service.changePassword(authorization, request.currentPassword, request.newPassword); return Collections.singletonMap("status", "password_changed");
    }
    @DeleteMapping("/account") Map<String, String> delete(@RequestHeader(value = "Authorization", required = false) String authorization) {
        service.deleteAccount(authorization); return Collections.singletonMap("status", "deletion_pending");
    }

    @PostMapping("/sync/push") List<AccountModels.SyncPushResult> push(
            @RequestHeader(value = "Authorization", required = false) String authorization,
            @RequestBody PushRequest request, HttpServletResponse response) {
        String requestId = monitor.start(); long started = System.nanoTime();
        AccountModels.DeviceSession session = null;
        int batchSize = request.changes == null ? 0 : request.changes.size();
        try {
            session = service.authenticate(authorization);
            List<AccountModels.SyncPushResult> results = service.push(session, request.deviceId, request.changes);
            Long revision = null; Long changeSeq = null; int conflicts = 0;
            for (AccountModels.SyncPushResult result : results) {
                if (result.revision != null && (revision == null || result.revision > revision)) revision = result.revision;
                if (result.changeSeq != null && (changeSeq == null || result.changeSeq > changeSeq)) changeSeq = result.changeSeq;
                if (!result.accepted) conflicts++;
            }
            monitor.success(requestId, session, "push", started, revision, changeSeq, conflicts, batchSize);
            response.setHeader("X-Request-Id", requestId);
            return results;
        } catch (RuntimeException exception) {
            monitor.failure(requestId, session, "push", started, errorCode(exception), errorStatus(exception), batchSize);
            throw exception;
        }
    }

    @GetMapping("/sync/pull") AccountModels.SyncPullPage pull(
            @RequestHeader(value = "Authorization", required = false) String authorization,
            @RequestParam(defaultValue = "0") long cursor, @RequestParam(defaultValue = "100") int limit,
            HttpServletResponse response) {
        String requestId = monitor.start(); long started = System.nanoTime();
        AccountModels.DeviceSession session = null;
        try {
            session = service.authenticate(authorization);
            AccountModels.SyncPullPage page = service.pull(session, cursor, limit);
            Long revision = null;
            for (AccountModels.SyncChange change : page.changes)
                if (revision == null || change.revision > revision) revision = change.revision;
            monitor.success(requestId, session, "pull", started, revision, page.nextCursor, 0, page.changes.size());
            response.setHeader("X-Request-Id", requestId);
            return page;
        } catch (RuntimeException exception) {
            monitor.failure(requestId, session, "pull", started, errorCode(exception), errorStatus(exception), 0);
            throw exception;
        }
    }

    @GetMapping("/sync/snapshot") AccountModels.SyncPullPage snapshot(
            @RequestHeader(value = "Authorization", required = false) String authorization,
            @RequestParam(defaultValue = "0") long cursor, @RequestParam(defaultValue = "100") int limit,
            HttpServletResponse response) {
        String requestId = monitor.start(); long started = System.nanoTime();
        AccountModels.DeviceSession session = null;
        try {
            session = service.authenticate(authorization);
            AccountModels.SyncPullPage page = service.snapshot(session, cursor, limit);
            Long revision = null;
            for (AccountModels.SyncChange change : page.changes)
                if (revision == null || change.revision > revision) revision = change.revision;
            monitor.success(requestId, session, "snapshot", started, revision, page.nextCursor, 0, page.changes.size());
            response.setHeader("X-Request-Id", requestId);
            return page;
        } catch (RuntimeException exception) {
            monitor.failure(requestId, session, "snapshot", started, errorCode(exception), errorStatus(exception), 0);
            throw exception;
        }
    }

    @ExceptionHandler(AccountException.class) ResponseEntity<Map<String, Object>> error(AccountException exception) {
        HttpStatus status = HttpStatus.BAD_REQUEST;
        if ("unavailable".equals(exception.code) || "mail_unavailable".equals(exception.code)) status = HttpStatus.SERVICE_UNAVAILABLE;
        else if ("invalid_credentials".equals(exception.code)) status = HttpStatus.UNAUTHORIZED;
        else if ("login_rate_limited".equals(exception.code) || "verification_rate_limited".equals(exception.code)) status = HttpStatus.TOO_MANY_REQUESTS;
        else if ("email_registered".equals(exception.code)) status = HttpStatus.CONFLICT;
        Map<String, Object> error = new LinkedHashMap<String, Object>(); error.put("code", exception.code); error.put("message", exception.getMessage());
        return ResponseEntity.status(status).body(Collections.<String, Object>singletonMap("error", error));
    }

    private static String errorCode(RuntimeException exception) {
        return exception instanceof AccountException ? ((AccountException) exception).code : "internal_error";
    }
    private static int errorStatus(RuntimeException exception) {
        if (!(exception instanceof AccountException)) return 500;
        String code = ((AccountException) exception).code;
        if ("invalid_credentials".equals(code)) return 401;
        if ("login_rate_limited".equals(code)) return 429;
        if ("unavailable".equals(code)) return 503;
        return 400;
    }

    static class VerificationCodeRequest {
        public String email; public String purpose; public String locale;
    }
    static class AuthRequest {
        public String email; public String password; public String code;
        @JsonProperty("device_id") public String deviceId;
        @JsonProperty("device_name") public String deviceName;
        @JsonProperty("client_version") public String clientVersion;
    }
    static class PasswordResetRequest {
        public String email; public String code;
        @JsonProperty("new_password") public String newPassword;
    }
    static class ProfileRequest {
        public String nickname;
    }
    static class PasswordChangeRequest {
        @JsonProperty("current_password") public String currentPassword;
        @JsonProperty("new_password") public String newPassword;
    }
    static class RefreshRequest {
        @JsonProperty("refresh_token") public String refreshToken;
        @JsonProperty("device_id") public String deviceId;
    }
    static class PushRequest {
        @JsonProperty("device_id") public String deviceId;
        public List<AccountModels.SyncMutation> changes;
    }
}
