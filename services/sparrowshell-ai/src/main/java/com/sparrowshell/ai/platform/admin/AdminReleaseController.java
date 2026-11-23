package com.sparrowshell.ai.platform.admin;

import com.sparrowshell.ai.platform.update.UpdateException;
import com.sparrowshell.ai.platform.update.UpdateRelease;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;
import javax.servlet.http.HttpServletRequest;
import java.time.Instant;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

@RestController
@RequestMapping("/v1/admin/releases")
class AdminReleaseController {
    private final AdminReleaseService service;
    AdminReleaseController(AdminReleaseService service) { this.service = service; }

    @GetMapping Map<String, Object> list(HttpServletRequest request) {
        List<UpdateRelease> rows = service.list(AdminAuthFilter.session(request));
        return java.util.Collections.<String, Object>singletonMap("items", rows.stream().map(AdminReleaseController::release).collect(Collectors.toList()));
    }
    @PostMapping Map<String, Object> create(HttpServletRequest request, @RequestBody UpdateRelease release) {
        return action(service.create(AdminAuthFilter.session(request), release));
    }
    @PostMapping("/{id}/{action}") Map<String, Object> transition(HttpServletRequest request, @PathVariable String id, @PathVariable String action, @RequestBody TransitionRequest body) {
        return action(service.transition(AdminAuthFilter.session(request), id, action, body.rolloutPercent, body.operation));
    }
    @ExceptionHandler(UpdateException.class) ResponseEntity<Map<String, Object>> updateError(UpdateException exception) {
        HttpStatus status = "unavailable".equals(exception.code) ? HttpStatus.SERVICE_UNAVAILABLE : "not_found".equals(exception.code) ? HttpStatus.NOT_FOUND : HttpStatus.BAD_REQUEST;
        if ("invalid_state".equals(exception.code) || "immutable_artifact".equals(exception.code)) status = HttpStatus.CONFLICT;
        return ResponseEntity.status(status).body(java.util.Collections.<String, Object>singletonMap("error", error(exception.code, exception.getMessage())));
    }
    @ExceptionHandler(AdminException.class) ResponseEntity<Map<String, Object>> adminError(AdminException exception) {
        HttpStatus status = "unavailable".equals(exception.code) ? HttpStatus.SERVICE_UNAVAILABLE : exception.code.endsWith("conflict") ? HttpStatus.CONFLICT : "permission_denied".equals(exception.code) || "mfa_invalid".equals(exception.code) ? HttpStatus.FORBIDDEN : HttpStatus.BAD_REQUEST;
        return ResponseEntity.status(status).body(java.util.Collections.<String, Object>singletonMap("error", error(exception.code, exception.getMessage())));
    }
    private static Map<String, Object> action(AdminService.ActionResult result) {
        Map<String, Object> map = new LinkedHashMap<String, Object>(); map.put("replayed", result.replayed); if (result.release != null) map.put("release", release(result.release)); return map;
    }
    private static Map<String, Object> release(UpdateRelease value) {
        Map<String, Object> map = new LinkedHashMap<String, Object>(); map.put("id", value.id); map.put("version", value.version); map.put("minimumMacOs", value.minimumMacOs);
        map.put("artifactUrl", value.artifactUrl); map.put("artifactBytes", value.artifactBytes); map.put("sha256", value.sha256); map.put("signature", value.signature);
        map.put("channel", value.channel); map.put("updatePolicy", value.updatePolicy); map.put("releaseNotes", value.releaseNotes); map.put("target", value.target); map.put("state", value.state); map.put("rolloutPercent", value.rolloutPercent);
        map.put("publishedAt", instant(value.publishedAt)); map.put("createdAt", instant(value.createdAt)); map.put("updatedAt", instant(value.updatedAt)); return map;
    }
    private static Map<String, Object> error(String code, String message) { Map<String, Object> map = new LinkedHashMap<String, Object>(); map.put("code", code); map.put("message", message); return map; }
    private static String instant(Instant value) { return value == null ? null : value.toString(); }
    static class TransitionRequest { public Integer rolloutPercent; public AdminService.DangerousRequest operation; }
}
