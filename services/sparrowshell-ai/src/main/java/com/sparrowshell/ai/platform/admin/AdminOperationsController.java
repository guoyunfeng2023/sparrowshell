package com.sparrowshell.ai.platform.admin;

import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.ExceptionHandler;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;
import javax.servlet.http.HttpServletRequest;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

/** Stable P0 read-only API. Domain permissions are enforced by the service, not by hidden UI buttons. */
@RestController
@RequestMapping("/v1/admin")
class AdminOperationsController {
    private final AdminOperationsService service;
    AdminOperationsController(AdminOperationsService service) { this.service = service; }

    @GetMapping("/dashboard")
    Map<String, Object> dashboard(HttpServletRequest request, @RequestParam(defaultValue = "today") String range) {
        return service.dashboard(AdminAuthFilter.session(request), range);
    }
    @GetMapping("/users/{id}/detail")
    Map<String, Object> userDetail(HttpServletRequest request, @PathVariable String id) {
        return service.userDetail(AdminAuthFilter.session(request), id);
    }
    @GetMapping("/sync/overview")
    Map<String, Object> syncOverview(HttpServletRequest request, @RequestParam(defaultValue = "today") String range) {
        return service.syncOverview(AdminAuthFilter.session(request), range);
    }
    @GetMapping("/sync/requests")
    Map<String, Object> syncRequests(HttpServletRequest request,
            @RequestParam(required = false) String accountId, @RequestParam(required = false) String status,
            @RequestParam(required = false) String errorCode,
            @RequestParam(defaultValue = "100") int limit, @RequestParam(defaultValue = "0") int offset) {
        return service.syncRequests(AdminAuthFilter.session(request), clean(accountId), clean(status), clean(errorCode), limit, offset);
    }
    @GetMapping("/ai/quotas")
    Map<String, Object> quotas(HttpServletRequest request, @RequestParam(required = false) String accountId,
            @RequestParam(defaultValue = "200") int limit) {
        return service.quotas(AdminAuthFilter.session(request), clean(accountId), limit);
    }
    @GetMapping("/system/jobs")
    Map<String, Object> jobs(HttpServletRequest request, @RequestParam(required = false) String status,
            @RequestParam(defaultValue = "100") int limit, @RequestParam(defaultValue = "0") int offset) {
        return service.jobs(AdminAuthFilter.session(request), clean(status), limit, offset);
    }
    @GetMapping("/system/health")
    Map<String, Object> health(HttpServletRequest request) { return service.health(AdminAuthFilter.session(request)); }
    @GetMapping("/access-control")
    Map<String, Object> accessControl(HttpServletRequest request) { return service.accessControl(AdminAuthFilter.session(request)); }

    @ExceptionHandler(AdminException.class)
    ResponseEntity<Map<String, Object>> error(AdminException exception) {
        HttpStatus status = HttpStatus.BAD_REQUEST;
        if ("unavailable".equals(exception.code)) status = HttpStatus.SERVICE_UNAVAILABLE;
        else if ("permission_denied".equals(exception.code)) status = HttpStatus.FORBIDDEN;
        else if ("not_found".equals(exception.code)) status = HttpStatus.NOT_FOUND;
        else if ("authentication_required".equals(exception.code)) status = HttpStatus.UNAUTHORIZED;
        Map<String, Object> error = new LinkedHashMap<String, Object>();
        error.put("code", exception.code); error.put("message", exception.getMessage());
        return ResponseEntity.status(status).body(Collections.<String, Object>singletonMap("error", error));
    }
    private static String clean(String value) { return value == null || value.trim().isEmpty() ? null : value.trim(); }
}
