package com.sparrowshell.ai.platform.billing;

import com.sparrowshell.ai.AiController;
import org.springframework.core.io.ClassPathResource;
import org.springframework.http.MediaType;
import org.springframework.util.StreamUtils;
import org.springframework.web.bind.annotation.*;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Map;

@RestController
@RequestMapping("/client-api/v1")
class BillingController {
    private final BillingService billing;
    private final AiTaskQuotaService quotas;
    BillingController(BillingService billing, AiTaskQuotaService quotas) { this.billing = billing; this.quotas = quotas; }

    @GetMapping("/ai-entitlement") Map<String, Object> entitlement(
            @RequestHeader(AiController.ANONYMOUS_ID_HEADER) String deviceId,
            @RequestHeader(value = AiController.DEVICE_TOKEN_HEADER, required = false) String token) { return billing.entitlement(deviceId, token); }
    @GetMapping("/subscription") Map<String, Object> subscription(
            @RequestHeader(AiController.ANONYMOUS_ID_HEADER) String deviceId,
            @RequestHeader(value = AiController.DEVICE_TOKEN_HEADER, required = false) String token) { return billing.subscription(deviceId, token); }
    @GetMapping("/billing/plans") List<Map<String, Object>> plans() { return billing.plans(); }
    @PostMapping("/orders") Map<String, Object> create(@RequestHeader(AiController.ANONYMOUS_ID_HEADER) String deviceId, @RequestBody CreateOrder request) {
        return billing.createOrder(deviceId, request.plan, request.licenseKey);
    }
    @GetMapping("/orders/{token}") Map<String, Object> order(@PathVariable String token) { return billing.order(token); }
    @PostMapping("/orders/{token}/alipay-qr") Map<String, Object> qr(@PathVariable String token) { return billing.qr(token); }
    @GetMapping(value = "/orders/{token}/qr.png", produces = MediaType.IMAGE_PNG_VALUE) byte[] qrPng(@PathVariable String token) { return billing.qrPng(token); }
    @PostMapping("/orders/{token}/query") Map<String, Object> query(@PathVariable String token) { return billing.query(token); }
    @GetMapping(value = "/orders/{token}/alipay-page", produces = "text/html;charset=UTF-8") String alipayPage(@PathVariable String token) { return billing.alipayPage(token); }
    @PostMapping("/licenses/activate") Map<String, Object> activate(@RequestHeader(AiController.ANONYMOUS_ID_HEADER) String deviceId, @RequestBody Activate request) {
        return billing.activate(request.licenseKey, deviceId);
    }
    @PostMapping("/licenses/recover") Map<String, Object> recover(@RequestHeader(AiController.ANONYMOUS_ID_HEADER) String deviceId,
            @RequestHeader(value = AiController.DEVICE_TOKEN_HEADER, required = false) String token) {
        return billing.recover(deviceId, token);
    }
    @PostMapping("/licenses/deactivate") Map<String, Object> deactivate(@RequestHeader(AiController.ANONYMOUS_ID_HEADER) String deviceId,
            @RequestHeader(value = AiController.DEVICE_TOKEN_HEADER, required = false) String token) { return billing.deactivate(deviceId, token); }
    @PostMapping("/ai-tasks/{taskId}/release") void release(@RequestHeader(AiController.ANONYMOUS_ID_HEADER) String deviceId, @PathVariable String taskId) {
        quotas.release(deviceId, taskId);
    }

    @GetMapping(value = "/checkout/{token}", produces = "text/html;charset=UTF-8") String checkout(@PathVariable String token) throws Exception {
        billing.order(token);
        return StreamUtils.copyToString(new ClassPathResource("static/checkout.html").getInputStream(), StandardCharsets.UTF_8);
    }
    @GetMapping(value = "/orders/{token}/mock-pay", produces = "text/html;charset=UTF-8") String mockPage(@PathVariable String token) {
        return "<!doctype html><meta charset='utf-8'><meta name='viewport' content='width=device-width'><title>SparrowShell 开发支付</title>"
                + "<body style='font-family:-apple-system,sans-serif;background:#f5f6f8;display:grid;place-items:center;min-height:95vh;margin:0'>"
                + "<main style='background:white;padding:32px;border-radius:16px;max-width:420px;text-align:center'><h2>开发环境模拟支付</h2>"
                + "<p style='color:#667085;line-height:1.7'>仅用于开发联调，不会发生真实扣款。确认后后台会发放许可证。</p>"
                + "<button id='pay' style='padding:12px 24px;border:0;border-radius:8px;background:#1677ff;color:white;font-size:16px'>确认模拟支付</button>"
                + "<p id='status'></p><script>document.getElementById('pay').onclick=async()=>{const b=document.getElementById('pay');b.disabled=true;"
                + "const r=await fetch(location.pathname,{method:'POST'});const j=await r.json();document.getElementById('status').textContent=r.ok?'支付成功，请返回收银台复制许可证':(j.message||'支付失败');"
                + "if(r.ok)setTimeout(()=>location.href=location.pathname.replace('/orders/','/checkout/').replace('/mock-pay',''),900);};</script></main>";
    }
    @PostMapping("/orders/{token}/mock-pay") Map<String, Object> mockPay(@PathVariable String token) { return billing.mockPay(token); }

    static class CreateOrder { public String plan; public String licenseKey; }
    static class Activate { public String licenseKey; }
}
