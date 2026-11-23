package com.sparrowshell.ai.platform.admin;

import com.sparrowshell.ai.platform.feedback.FeedbackException;
import com.sparrowshell.ai.platform.feedback.FeedbackModels;
import com.sparrowshell.ai.platform.feedback.FeedbackService;
import org.springframework.core.io.FileSystemResource;
import org.springframework.core.io.Resource;
import org.springframework.http.ContentDisposition;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpStatus;
import org.springframework.http.MediaType;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;
import javax.servlet.http.HttpServletRequest;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

@RestController
@RequestMapping("/v1/admin/feedback")
class AdminFeedbackController {
    private final FeedbackService service;
    private final AdminAuthService auth;
    AdminFeedbackController(FeedbackService service, AdminAuthService auth) { this.service = service; this.auth = auth; }

    @GetMapping Map<String, Object> list(HttpServletRequest request, @RequestParam(required = false) String status,
            @RequestParam(defaultValue = "100") int limit, @RequestParam(defaultValue = "0") int offset) {
        readable(AdminAuthFilter.session(request));
        String cleanStatus = status == null || status.trim().isEmpty() ? null : status.trim();
        Map<String, Object> response = new LinkedHashMap<String, Object>();
        response.put("items", service.list(cleanStatus, limit, offset)); response.put("total", service.count(cleanStatus));
        response.put("limit", limit); response.put("offset", offset); return response;
    }
    @GetMapping("/{id}") FeedbackModels.Feedback detail(HttpServletRequest request, @PathVariable String id) {
        readable(AdminAuthFilter.session(request)); return service.get(id);
    }
    @PatchMapping("/{id}/status") FeedbackModels.Feedback update(HttpServletRequest request, @PathVariable String id, @RequestBody StatusRequest body) {
        AdminModels.AdminSession session = AdminAuthFilter.session(request);
        auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN");
        return service.updateStatus(id, body.status, session.adminId);
    }
    @GetMapping("/{id}/attachment") ResponseEntity<Resource> attachment(HttpServletRequest request, @PathVariable String id) {
        readable(AdminAuthFilter.session(request));
        FeedbackModels.Attachment attachment = service.get(id).attachment;
        Path path = service.attachmentPath(attachment);
        MediaType mediaType;
        try { mediaType = MediaType.parseMediaType(attachment.contentType); }
        catch (Exception exception) { mediaType = MediaType.APPLICATION_OCTET_STREAM; }
        return ResponseEntity.ok().contentType(mediaType).contentLength(attachment.sizeBytes)
                .header(HttpHeaders.CONTENT_DISPOSITION, ContentDisposition.attachment().filename(attachment.originalName, StandardCharsets.UTF_8).build().toString())
                .header(HttpHeaders.CACHE_CONTROL, "no-store").header("X-Content-Type-Options", "nosniff")
                .body(new FileSystemResource(path));
    }

    private void readable(AdminModels.AdminSession session) { auth.requireRole(session, "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER", "AUDITOR"); }
    @ExceptionHandler(FeedbackException.class) ResponseEntity<Map<String, Object>> error(FeedbackException exception) {
        HttpStatus status = "not_found".equals(exception.code) ? HttpStatus.NOT_FOUND : "unavailable".equals(exception.code) ? HttpStatus.SERVICE_UNAVAILABLE : HttpStatus.BAD_REQUEST;
        Map<String, String> error = new LinkedHashMap<String, String>(); error.put("code", exception.code); error.put("message", exception.getMessage());
        return ResponseEntity.status(status).body(Collections.<String, Object>singletonMap("error", error));
    }
    @ExceptionHandler(AdminException.class) ResponseEntity<Map<String, Object>> adminError(AdminException exception) {
        Map<String, String> error = new LinkedHashMap<String, String>(); error.put("code", exception.code); error.put("message", exception.getMessage());
        return ResponseEntity.status("permission_denied".equals(exception.code) ? HttpStatus.FORBIDDEN : HttpStatus.BAD_REQUEST).body(Collections.<String, Object>singletonMap("error", error));
    }
    static class StatusRequest { public String status; }
}
