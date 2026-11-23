package com.sparrowshell.ai.platform.feedback;

import com.sparrowshell.ai.platform.account.AccountException;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.multipart.MaxUploadSizeExceededException;
import org.springframework.web.multipart.MultipartFile;
import javax.servlet.http.HttpServletRequest;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

@RestController
@RequestMapping("/client-api/v1/feedback")
class FeedbackController {
    private final FeedbackService service;
    FeedbackController(FeedbackService service) { this.service = service; }

    @PostMapping(consumes = "multipart/form-data")
    @ResponseStatus(HttpStatus.CREATED)
    Map<String, Object> submit(@RequestHeader(value = "Authorization", required = false) String authorization,
            @RequestParam String description, @RequestParam(required = false) String contact,
            @RequestPart(required = false) MultipartFile attachment, HttpServletRequest request) {
        FeedbackModels.Feedback feedback = service.submit(authorization, description, contact, attachment, request.getRemoteAddr());
        Map<String, Object> response = new LinkedHashMap<String, Object>();
        response.put("feedbackId", feedback.id); response.put("status", feedback.status); return response;
    }

    @ExceptionHandler(FeedbackException.class)
    ResponseEntity<Map<String, Object>> error(FeedbackException exception) {
        HttpStatus status = HttpStatus.BAD_REQUEST;
        if ("unavailable".equals(exception.code) || "attachment_failed".equals(exception.code)) status = HttpStatus.SERVICE_UNAVAILABLE;
        else if ("rate_limited".equals(exception.code)) status = HttpStatus.TOO_MANY_REQUESTS;
        else if ("not_found".equals(exception.code)) status = HttpStatus.NOT_FOUND;
        return ResponseEntity.status(status).body(Collections.<String, Object>singletonMap("error", error(exception.code, exception.getMessage())));
    }
    @ExceptionHandler(AccountException.class)
    ResponseEntity<Map<String, Object>> accountError(AccountException exception) {
        return ResponseEntity.status(HttpStatus.UNAUTHORIZED).body(Collections.<String, Object>singletonMap("error", error(exception.code, "登录状态已失效，请重新登录或退出后匿名提交")));
    }
    @ExceptionHandler(MaxUploadSizeExceededException.class)
    ResponseEntity<Map<String, Object>> attachmentTooLarge() {
        return ResponseEntity.status(HttpStatus.PAYLOAD_TOO_LARGE).body(Collections.<String, Object>singletonMap("error", error("invalid_attachment", "附件不能超过 10 MB")));
    }
    private static Map<String, String> error(String code, String message) {
        Map<String, String> error = new LinkedHashMap<String, String>(); error.put("code", code); error.put("message", message); return error;
    }
}
