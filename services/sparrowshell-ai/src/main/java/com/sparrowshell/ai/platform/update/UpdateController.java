package com.sparrowshell.ai.platform.update;

import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;
import java.util.Collections;
import java.util.Map;

@RestController
@RequestMapping("/client-api/v1/updates")
class UpdateController {
    private final UpdateService service;
    UpdateController(UpdateService service) { this.service = service; }

    @GetMapping("/check")
    Map<String, Object> check(@RequestParam String currentVersion, @RequestParam String channel,
                              @RequestParam String deviceId, @RequestParam String target,
                              @RequestParam(defaultValue = "false") boolean includeSilent) {
        return service.check(currentVersion, channel, deviceId, target, includeSilent);
    }

    @ExceptionHandler(UpdateException.class)
    ResponseEntity<Map<String, Object>> error(UpdateException exception) {
        HttpStatus status = "unavailable".equals(exception.code) ? HttpStatus.SERVICE_UNAVAILABLE : HttpStatus.BAD_REQUEST;
        return ResponseEntity.status(status).body(Collections.singletonMap("code", exception.code));
    }

    @ExceptionHandler(IllegalArgumentException.class)
    ResponseEntity<Map<String, Object>> invalidIdentifier() {
        return ResponseEntity.badRequest().body(Collections.singletonMap("code", "invalid_request"));
    }
}
