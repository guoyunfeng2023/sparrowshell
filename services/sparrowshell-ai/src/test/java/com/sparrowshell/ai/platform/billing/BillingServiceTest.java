package com.sparrowshell.ai.platform.billing;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import java.time.Clock;
import java.time.Instant;
import java.time.ZoneOffset;
import java.util.HashMap;
import java.util.Map;
import static org.junit.jupiter.api.Assertions.*;

class BillingServiceTest {
    private static final String DEVICE = "0be0dd37-9379-4663-a94f-799f16ab7678";
    private static final String OTHER = "d2d5bafa-76ce-4a72-9db7-a82f527dd17b";
    private InMemoryBillingRepository repository;
    private BillingService service;
    private FakeGateway gateway;

    @BeforeEach void setUp() {
        repository = new InMemoryBillingRepository(); gateway = new FakeGateway();
        service = new BillingService(repository, gateway, new LicenseCrypto("unit-test-secret", "mock"), "mock", "http://127.0.0.1:8070", Clock.fixed(Instant.parse("2026-11-10T04:00:00Z"), ZoneOffset.UTC));
    }

    @Test @SuppressWarnings("unchecked") void mockPurchaseActivatesPaidQuotaAndSupportsPlannedRenewal() {
        Map<String, Object> order = service.createOrder(DEVICE, "personal", null); String token = (String) order.get("token");
        assertEquals("9.90", order.get("amount").toString()); assertTrue(service.qrPng(token).length > 100);
        Map<String, Object> paid = service.mockPay(token); String key = (String) paid.get("licenseKey");
        assertTrue(key.startsWith("SS-")); assertEquals(key, service.order(token).get("licenseKey"));
        Map<String, Object> activation = service.activate(key, DEVICE); String deviceToken = (String) activation.get("deviceToken");
        Map<String, Object> entitlement = service.entitlement(DEVICE, deviceToken);
        assertEquals("personal", entitlement.get("plan")); assertEquals(200, entitlement.get("quota")); assertEquals(200, entitlement.get("remaining"));
        assertEquals("license_bound", assertThrows(BillingException.class, () -> service.activate(key, OTHER)).code);
        Map<String, Object> renewal = service.createOrder(DEVICE, "personal", key); service.mockPay((String) renewal.get("token"));
        assertEquals(2, repository.periods.size());
    }

    @Test void upgradeCarriesCurrentAndPrepaidPersonalQuotaAndStartsImmediately() {
        Instant now = Instant.parse("2026-11-10T04:00:00Z");
        String key = (String) service.mockPay((String) service.createOrder(DEVICE, "personal", null).get("token")).get("licenseKey");
        String deviceToken = (String) service.activate(key, DEVICE).get("deviceToken");
        String taskId = "6f7db91c-4e5f-4f3b-b50b-1eb3f42f7db5";
        repository.reserveTask(DEVICE, AiTaskQuotaService.hash(deviceToken), taskId, now, now.plusSeconds(900));
        repository.settleTask(DEVICE, taskId, true, now);
        assertEquals(199, service.entitlement(DEVICE, deviceToken).get("remaining"));

        service.mockPay((String) service.createOrder(DEVICE, "personal", key).get("token"));
        Map<String, Object> upgradedOrder = service.mockPay((String) service.createOrder(DEVICE, "high", key).get("token"));
        assertEquals(key, upgradedOrder.get("licenseKey"));
        BillingModels.LicensePeriod upgraded = repository.periods.values().stream()
                .filter(period -> "high".equals(period.plan)).findFirst().orElseThrow();
        assertEquals(1399, upgraded.quota);
        assertEquals(now, upgraded.startsAt);
        assertEquals(Instant.parse("2026-12-10T04:00:00Z"), upgraded.endsAt);
        assertTrue(repository.periods.values().stream().filter(period -> "personal".equals(period.plan))
                .allMatch(period -> now.equals(period.endsAt)));

        Map<String, Object> activation = service.activate(key, DEVICE);
        Map<String, Object> entitlement = (Map<String, Object>) activation.get("entitlement");
        assertNotNull(entitlement);
        assertEquals("high", entitlement.get("plan")); assertEquals(1399, entitlement.get("quota")); assertEquals(0, entitlement.get("used"));

        service.mockPay((String) service.createOrder(DEVICE, "high", key).get("token"));
        assertEquals(1000, repository.latestPeriod(upgraded.licenseId).orElseThrow().quota);
        assertEquals("plan_change_active", assertThrows(BillingException.class, () -> service.createOrder(DEVICE, "personal", key)).code);
    }

    @Test void expiredLicenseRetainsPreviousPlanAndAllowsImmediateRenewal() {
        String key = (String) service.mockPay((String) service.createOrder(DEVICE, "personal", null).get("token")).get("licenseKey");
        String deviceToken = (String) service.activate(key, DEVICE).get("deviceToken");
        service = new BillingService(repository, gateway, new LicenseCrypto("unit-test-secret", "mock"), "mock", "http://127.0.0.1:8070",
                Clock.fixed(Instant.parse("2026-12-11T04:00:00Z"), ZoneOffset.UTC));

        Map<String, Object> expired = service.entitlement(DEVICE, deviceToken);
        assertEquals("free", expired.get("plan")); assertEquals(false, expired.get("licensed")); assertEquals(true, expired.get("bound"));
        assertEquals(true, expired.get("expired")); assertEquals("personal", expired.get("previousPlan"));
        assertEquals(Instant.parse("2026-12-10T04:00:00Z"), expired.get("previousPeriodEnd"));

        Map<String, Object> renewed = service.mockPay((String) service.createOrder(DEVICE, "high", key).get("token"));
        assertEquals(key, renewed.get("licenseKey"));
        Map<String, Object> activation = service.activate(key, DEVICE);
        Map<String, Object> entitlement = (Map<String, Object>) activation.get("entitlement");
        assertEquals("high", entitlement.get("plan")); assertEquals(1000, entitlement.get("quota"));
        assertEquals(false, entitlement.get("expired"));
        assertEquals(Instant.parse("2026-12-11T04:00:00Z"), entitlement.get("periodStart"));
    }

    @Test void deactivationAllowsAnotherComputerWithoutUsingAccountIdentity() {
        String key = (String) service.mockPay((String) service.createOrder(DEVICE, "high", null).get("token")).get("licenseKey");
        String token = (String) service.activate(key, DEVICE).get("deviceToken"); service.deactivate(DEVICE, token);
        String otherToken = (String) service.activate(key, OTHER).get("deviceToken");
        assertEquals(1000, service.entitlement(OTHER, otherToken).get("quota"));
        assertEquals("free", service.entitlement(DEVICE, null).get("plan"));
    }

    @Test void recoveryRequiresTheBoundDeviceTokenAndReturnsTheOriginalLicense() {
        String key = (String) service.mockPay((String) service.createOrder(DEVICE, "personal", null).get("token")).get("licenseKey");
        String deviceToken = (String) service.activate(key, DEVICE).get("deviceToken");

        assertEquals(key, service.recover(DEVICE, deviceToken).get("licenseKey"));
        assertEquals("invalid_device_token", assertThrows(BillingException.class, () -> service.recover(DEVICE, null)).code);
        assertEquals("invalid_device_token", assertThrows(BillingException.class, () -> service.recover(DEVICE, "wrong-token")).code);
        assertEquals("invalid_device_token", assertThrows(BillingException.class, () -> service.recover(OTHER, deviceToken)).code);
    }

    @Test void duplicateRealNotificationDoesNotCreateAnotherBillingPeriod() {
        service = new BillingService(repository, gateway, new LicenseCrypto("unit-test-secret", "alipay"), "alipay", "https://example.test", Clock.systemUTC());
        String token = (String) service.createOrder(DEVICE, "personal", null).get("token"); Map<String, String> notification = new HashMap<String, String>();
        notification.put("out_trade_no", token); notification.put("trade_no", "2026111022001400000001"); notification.put("notify_id", "notification-1");
        assertEquals("success", service.alipayNotification(notification)); assertEquals("success", service.alipayNotification(notification));
        assertEquals(1, repository.periods.size()); assertEquals(1, repository.licenses.size());
    }

    @Test void checkoutOnlyRevealsLicenseAfterServerConfirmedPayment() {
        String token = (String) service.createOrder(DEVICE, "personal", null).get("token");
        assertFalse(service.order(token).containsKey("licenseKey")); service.mockPay(token); assertTrue(service.order(token).containsKey("licenseKey"));
    }

    @Test void pagePaymentUsesServerNotificationAndCheckoutReturnUrls() {
        service = new BillingService(repository, gateway, new LicenseCrypto("unit-test-secret", "alipay"), "alipay", "https://www.example.test/sparrowshell/", Clock.systemUTC());
        String token = (String) service.createOrder(DEVICE, "personal", null).get("token");
        String page = service.alipayPage(token);
        assertTrue(page.contains("https://www.example.test/sparrowshell/payments/alipay/notify"));
        assertTrue(page.contains("https://www.example.test/sparrowshell/client-api/v1/checkout/" + token));
        assertEquals("https://www.example.test/sparrowshell/client-api/v1/orders/" + token + "/alipay-page", service.order(token).get("paymentUrl"));
    }

    @Test void testAmountOverrideAppliesToPlansOrdersAndEntitlement() {
        service = new BillingService(repository, gateway, new LicenseCrypto("unit-test-secret", "alipay"), "alipay", "https://example.test", Clock.systemUTC(), new java.math.BigDecimal("0.10"));
        assertEquals("0.10", service.plans().get(0).get("price").toString());
        assertEquals("0.10", service.plans().get(1).get("price").toString());
        Map<String, Object> order = service.createOrder(DEVICE, "high", null);
        assertEquals("0.10", order.get("amount").toString());
        assertEquals("0.10", service.entitlement(DEVICE, null).get("paymentAmountOverride").toString());
    }

    private static class FakeGateway implements PaymentGateway {
        public Precreate precreate(BillingModels.Order order, String notifyUrl) { return new Precreate("https://example.test/pay/" + order.token, Instant.now().plusSeconds(300)); }
        public String pagePay(BillingModels.Order order, String notifyUrl, String returnUrl) { return "<form data-order='" + order.token + "' data-notify='" + notifyUrl + "' data-return='" + returnUrl + "'></form>"; }
        public Query query(BillingModels.Order order) { return new Query(false, null); }
        public boolean validNotification(Map<String, String> parameters, BillingModels.Order order) { return true; }
        public String paidTradeNo(Map<String, String> parameters, BillingModels.Order order) { return parameters.get("trade_no"); }
    }
}
