package com.sparrowshell.ai.platform.billing;

import org.springframework.http.MediaType;
import org.springframework.web.bind.annotation.*;
import java.util.Map;

@RestController
class AlipayNotificationController {
    private final BillingService billing;
    AlipayNotificationController(BillingService billing) { this.billing = billing; }
    @PostMapping(value = "/payments/alipay/notify", produces = MediaType.TEXT_PLAIN_VALUE)
    String notifyPayment(@RequestParam Map<String, String> parameters) { return billing.alipayNotification(parameters); }
}
