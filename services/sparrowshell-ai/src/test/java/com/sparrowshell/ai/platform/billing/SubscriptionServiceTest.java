package com.sparrowshell.ai.platform.billing;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import java.time.Clock;
import java.time.Instant;
import java.time.ZoneOffset;
import java.util.List;
import java.util.Map;
import static org.junit.jupiter.api.Assertions.*;

class SubscriptionServiceTest {
    private static final String DEVICE = "0be0dd37-9379-4663-a94f-799f16ab7678";
    private InMemoryBillingRepository repository;
    private BillingService service;

    @BeforeEach void setUp() {
        repository = new InMemoryBillingRepository();
        service = new BillingService(repository, new FakeGateway(), new LicenseCrypto("unit-test-secret", "mock"), "mock", "http://127.0.0.1:8070",
                Clock.fixed(Instant.parse("2026-11-10T04:00:00Z"), ZoneOffset.UTC));
    }

    @Test @SuppressWarnings("unchecked") void subscriptionSeparatesCurrentAndPrepaidPeriods() {
        String key = (String) service.mockPay((String) service.createOrder(DEVICE, "personal", null).get("token")).get("licenseKey");
        String deviceToken = (String) service.activate(key, DEVICE).get("deviceToken");
        Map<String, Object> initial = service.subscription(DEVICE, deviceToken);
        Map<String, Object> current = (Map<String, Object>) initial.get("current");
        assertEquals("active", current.get("status")); assertEquals("personal", current.get("plan"));
        assertEquals(200, current.get("remaining")); assertEquals(false, current.get("exhausted"));
        assertEquals(false, initial.get("autoRenew")); assertEquals("manual", initial.get("renewalMode"));
        assertEquals(true, initial.get("canRenew")); assertEquals(true, initial.get("canUpgrade"));
        assertTrue(((List<Map<String, Object>>) initial.get("upcoming")).isEmpty());

        service.mockPay((String) service.createOrder(DEVICE, "personal", key).get("token"));
        Map<String, Object> renewed = service.subscription(DEVICE, deviceToken);
        List<Map<String, Object>> upcoming = (List<Map<String, Object>>) renewed.get("upcoming");
        assertEquals(1, upcoming.size()); assertEquals("scheduled", upcoming.get(0).get("status"));
        assertEquals(Instant.parse("2026-12-10T04:00:00Z"), upcoming.get(0).get("startsAt"));
        assertEquals(Instant.parse("2027-01-10T04:00:00Z"), renewed.get("latestPeriodEnd"));
        assertEquals("month", service.plans().get(0).get("interval")); assertEquals(false, service.plans().get(0).get("autoRenew"));
    }

    @Test @SuppressWarnings("unchecked") void invalidTokenCannotReadPeriodsAndExpiredSubscriptionCanRenew() {
        String key = (String) service.mockPay((String) service.createOrder(DEVICE, "high", null).get("token")).get("licenseKey");
        String deviceToken = (String) service.activate(key, DEVICE).get("deviceToken");
        Map<String, Object> invalid = service.subscription(DEVICE, "invalid-token");
        assertNull(invalid.get("current")); assertEquals(false, invalid.get("canRenew"));
        assertTrue(((List<Map<String, Object>>) invalid.get("upcoming")).isEmpty());

        service = new BillingService(repository, new FakeGateway(), new LicenseCrypto("unit-test-secret", "mock"), "mock", "http://127.0.0.1:8070",
                Clock.fixed(Instant.parse("2026-12-11T04:00:00Z"), ZoneOffset.UTC));
        Map<String, Object> expired = service.subscription(DEVICE, deviceToken);
        assertNull(expired.get("current")); assertEquals(true, expired.get("expired")); assertEquals("high", expired.get("previousPlan"));
        assertEquals(true, expired.get("canRenew")); assertEquals(false, expired.get("canUpgrade"));
        assertEquals(Instant.parse("2026-12-10T04:00:00Z"), expired.get("latestPeriodEnd"));
    }

    private static class FakeGateway implements PaymentGateway {
        public Precreate precreate(BillingModels.Order order, String notifyUrl) { return new Precreate("https://example.test/pay/" + order.token, Instant.now().plusSeconds(300)); }
        public String pagePay(BillingModels.Order order, String notifyUrl, String returnUrl) { return "<form></form>"; }
        public Query query(BillingModels.Order order) { return new Query(false, null); }
        public boolean validNotification(Map<String, String> parameters, BillingModels.Order order) { return true; }
        public String paidTradeNo(Map<String, String> parameters, BillingModels.Order order) { return parameters.get("trade_no"); }
    }
}
