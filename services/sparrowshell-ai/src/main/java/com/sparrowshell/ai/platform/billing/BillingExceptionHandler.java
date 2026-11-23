package com.sparrowshell.ai.platform.billing;

import com.sparrowshell.ai.ProviderException;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.ExceptionHandler;
import org.springframework.web.bind.annotation.RestControllerAdvice;
import java.util.LinkedHashMap;
import java.util.Map;

@RestControllerAdvice(basePackages = "com.sparrowshell.ai.platform.billing")
class BillingExceptionHandler {
    @ExceptionHandler(BillingException.class)
    ResponseEntity<Map<String, Object>> billing(BillingException exception) { return response(exception.code, exception.getMessage()); }
    @ExceptionHandler(ProviderException.class)
    ResponseEntity<Map<String, Object>> provider(ProviderException exception) { return response(exception.code(), exception.getMessage()); }
    private ResponseEntity<Map<String, Object>> response(String code, String message) {
        HttpStatus status = HttpStatus.BAD_REQUEST;
        if ("quota_exceeded".equals(code)) status = HttpStatus.PAYMENT_REQUIRED;
        else if ("billing_unavailable".equals(code) || "payment_unavailable".equals(code) || "alipay_unavailable".equals(code)) status = HttpStatus.SERVICE_UNAVAILABLE;
        else if ("order_not_found".equals(code) || "not_found".equals(code) || "license_not_found".equals(code)) status = HttpStatus.NOT_FOUND;
        else if ("license_bound".equals(code) || "order_not_pending".equals(code) || "duplicate_task".equals(code)) status = HttpStatus.CONFLICT;
        Map<String, Object> body = new LinkedHashMap<String, Object>(); body.put("code", code); body.put("message", message);
        return ResponseEntity.status(status).body(body);
    }
}
