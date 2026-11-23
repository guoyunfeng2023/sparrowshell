package com.sparrowshell.ai.platform.billing;

import com.google.zxing.BarcodeFormat;
import com.google.zxing.EncodeHintType;
import com.google.zxing.common.BitMatrix;
import com.google.zxing.qrcode.QRCodeWriter;
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Service;
import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.io.ByteArrayOutputStream;
import java.math.BigDecimal;
import java.time.Clock;
import java.time.Instant;
import java.time.ZoneId;
import java.time.ZonedDateTime;
import java.time.temporal.ChronoUnit;
import java.util.*;

@Service
public class BillingService {
    private static final ZoneId BILLING_ZONE = ZoneId.of("Asia/Shanghai");
    private final BillingRepository repository;
    private final PaymentGateway payments;
    private final LicenseCrypto crypto;
    private final String channel;
    private final String publicBaseUrl;
    private final Clock clock;
    private final BigDecimal paymentAmountOverride;

    @Autowired
    public BillingService(BillingRepository repository, PaymentGateway payments, LicenseCrypto crypto,
            @Value("${sparrowshell.billing.payment-channel:disabled}") String channel,
            @Value("${sparrowshell.public-base-url:http://127.0.0.1:8070}") String publicBaseUrl,
            @Value("${sparrowshell.billing.test-amount:}") String paymentAmountOverride) {
        this(repository, payments, crypto, channel, publicBaseUrl, Clock.systemUTC(), parseAmountOverride(paymentAmountOverride));
    }
    BillingService(BillingRepository repository, PaymentGateway payments, LicenseCrypto crypto,
                   String channel, String publicBaseUrl, Clock clock) {
        this(repository, payments, crypto, channel, publicBaseUrl, clock, null);
    }
    BillingService(BillingRepository repository, PaymentGateway payments, LicenseCrypto crypto,
                   String channel, String publicBaseUrl, Clock clock, BigDecimal paymentAmountOverride) {
        this.repository = repository; this.payments = payments; this.crypto = crypto; this.channel = channel;
        this.publicBaseUrl = publicBaseUrl.replaceAll("/+$", ""); this.clock = clock;
        this.paymentAmountOverride = paymentAmountOverride;
    }
    private static BigDecimal parseAmountOverride(String value) {
        if (value == null || value.trim().isEmpty()) return null;
        try {
            BigDecimal amount = new BigDecimal(value.trim());
            if (amount.compareTo(new BigDecimal("0.01")) < 0 || amount.scale() > 2) throw new NumberFormatException();
            return amount;
        } catch (NumberFormatException exception) {
            throw new IllegalArgumentException("SPARROWSHELL_BILLING_TEST_AMOUNT must be a positive amount with at most two decimals", exception);
        }
    }
    private BigDecimal price(BillingModels.Plan plan) { return paymentAmountOverride == null ? plan.price : paymentAmountOverride; }

    public Map<String, Object> entitlement(String deviceId, String deviceToken) {
        return entitlementView(repository.entitlement(AiTaskQuotaService.device(deviceId), AiTaskQuotaService.hash(deviceToken), now()));
    }

    public Map<String, Object> subscription(String deviceId, String deviceToken) {
        Instant now = now();
        BillingModels.Entitlement value = repository.entitlement(AiTaskQuotaService.device(deviceId), AiTaskQuotaService.hash(deviceToken), now);
        Map<String, Object> result = entitlementView(value);
        Map<String, Object> current = null;
        List<Map<String, Object>> upcoming = new ArrayList<Map<String, Object>>();
        Instant latestPeriodEnd = value.previousPeriodEnd;
        if (value.licenseId != null) {
            for (BillingModels.LicensePeriod period : repository.subscriptionPeriods(value.licenseId)) {
                Map<String, Object> item = periodView(period, now);
                if ("active".equals(item.get("status"))) current = item;
                else if ("scheduled".equals(item.get("status"))) upcoming.add(item);
                if (latestPeriodEnd == null || period.endsAt.isAfter(latestPeriodEnd)) latestPeriodEnd = period.endsAt;
            }
        }
        result.put("current", current); result.put("upcoming", upcoming); result.put("latestPeriodEnd", latestPeriodEnd);
        result.put("autoRenew", false); result.put("renewalMode", "manual");
        result.put("canRenew", value.licenseId != null);
        result.put("canUpgrade", value.licensed && "personal".equals(value.plan));
        return result;
    }

    public List<Map<String, Object>> plans() {
        List<Map<String, Object>> result = new ArrayList<Map<String, Object>>();
        for (BillingModels.Plan plan : BillingModels.Plan.values()) {
            Map<String, Object> item = new LinkedHashMap<String, Object>(); item.put("code", plan.code);
            item.put("price", price(plan)); item.put("quota", plan.quota); item.put("days", 30);
            item.put("interval", "month"); item.put("autoRenew", false); result.add(item);
        }
        return result;
    }

    public Map<String, Object> createOrder(String deviceId, String planCode, String licenseKey) {
        AiTaskQuotaService.device(deviceId);
        requirePayment(); BillingModels.Plan plan = BillingModels.Plan.parse(planCode);
        String licenseId = null;
        if (licenseKey != null && !licenseKey.trim().isEmpty()) {
            BillingModels.License license = license(licenseKey);
            Instant latestEnd = repository.latestPeriod(license.id).map(period -> period.endsAt).orElse(null);
            if (latestEnd != null && latestEnd.isAfter(now())) {
                BillingModels.LicensePeriod current = repository.activePeriod(license.id, now()).orElse(null);
                if (current != null && !current.plan.equals(plan.code) && !isUpgrade(current.plan, plan.code))
                    throw new BillingException("plan_change_active", "当前计费周期结束前不能更换套餐");
            }
            licenseId = license.id;
        }
        BillingModels.Order order = new BillingModels.Order(); order.token = LicenseCrypto.randomHex(16);
        order.plan = plan.code; order.amount = price(plan); order.licenseId = licenseId;
        order.createdAt = now(); order.orderExpiresAt = now().plus(30, ChronoUnit.MINUTES); repository.insertOrder(order);
        return orderView(order);
    }

    public Map<String, Object> order(String token) { return orderView(orderByToken(token)); }

    public String alipayPage(String token) {
        BillingModels.Order order = pendingOrder(token);
        return payments.pagePay(order, publicBaseUrl + "/payments/alipay/notify",
                publicBaseUrl + "/client-api/v1/checkout/" + order.token);
    }

    public Map<String, Object> qr(String token) {
        BillingModels.Order order = pendingOrder(token);
        if (order.qrCode == null || order.qrExpiresAt == null || !order.qrExpiresAt.isAfter(now())) {
            PaymentGateway.Precreate precreate = payments.precreate(order, publicBaseUrl + "/payments/alipay/notify");
            repository.saveQrCode(token, precreate.qrCode, precreate.expiresAt); order = orderByToken(token);
        }
        Map<String, Object> result = orderView(order); result.put("qrContent", order.qrCode); return result;
    }

    public byte[] qrPng(String token) {
        String content = qr(token).get("qrContent").toString();
        try {
            Map<EncodeHintType, Object> hints = new EnumMap<EncodeHintType, Object>(EncodeHintType.class);
            hints.put(EncodeHintType.ERROR_CORRECTION, ErrorCorrectionLevel.M); hints.put(EncodeHintType.MARGIN, 1);
            hints.put(EncodeHintType.CHARACTER_SET, "UTF-8");
            BitMatrix matrix = new QRCodeWriter().encode(content, BarcodeFormat.QR_CODE, 320, 320, hints);
            BufferedImage image = new BufferedImage(matrix.getWidth(), matrix.getHeight(), BufferedImage.TYPE_INT_RGB);
            for (int x = 0; x < matrix.getWidth(); x++) for (int y = 0; y < matrix.getHeight(); y++)
                image.setRGB(x, y, matrix.get(x, y) ? 0x111827 : 0xFFFFFF);
            ByteArrayOutputStream output = new ByteArrayOutputStream(); ImageIO.write(image, "png", output); return output.toByteArray();
        } catch (Exception exception) {
            if (exception instanceof BillingException) throw (BillingException) exception;
            throw new BillingException("qr_unavailable", "二维码生成失败");
        }
    }

    public Map<String, Object> query(String token) {
        BillingModels.Order order = orderByToken(token);
        if ("pending".equals(order.status) && order.orderExpiresAt.isAfter(now()) && !"mock".equals(channel)) {
            PaymentGateway.Query query = payments.query(order);
            if (query.paid) order = complete(order, query.tradeNo, null);
        }
        return orderView(order);
    }

    public Map<String, Object> mockPay(String token) {
        if (!"mock".equals(channel)) throw new BillingException("not_found", "页面不存在");
        BillingModels.Order order = pendingOrder(token);
        return orderView(complete(order, "MOCK-" + UUID.randomUUID().toString().replace("-", ""), "mock-" + UUID.randomUUID()));
    }

    public String alipayNotification(Map<String, String> parameters) {
        if (!"alipay".equals(channel)) throw new BillingException("not_found", "页面不存在");
        String token = parameters.get("out_trade_no"); BillingModels.Order order = orderByToken(token);
        if (!payments.validNotification(parameters, order)) throw new BillingException("payment_verification_failed", "支付通知验签或订单校验失败");
        String tradeNo = payments.paidTradeNo(parameters, order);
        if (tradeNo == null) return "success";
        String notificationId = parameters.get("notify_id"); if (notificationId == null || notificationId.length() > 128) notificationId = tradeNo;
        complete(order, tradeNo, notificationId); return "success";
    }

    private BillingModels.Order complete(BillingModels.Order order, String tradeNo, String notificationId) {
        if (tradeNo == null || tradeNo.trim().isEmpty() || tradeNo.length() > 64)
            throw new BillingException("payment_verification_failed", "支付流水号无效");
        BillingModels.License newLicense = null; String licenseId = order.licenseId;
        BillingModels.Plan plan = BillingModels.Plan.parse(order.plan);
        BillingModels.LicensePeriod current = null;
        if (licenseId == null) {
            String plaintextKey = LicenseCrypto.newLicenseKey(); newLicense = new BillingModels.License();
            newLicense.keyHash = AiTaskQuotaService.hash(plaintextKey); newLicense.keyCiphertext = crypto.encrypt(plaintextKey);
            newLicense.createdAt = now(); newLicense.updatedAt = now(); licenseId = newLicense.id;
        } else {
            repository.licenseById(licenseId).orElseThrow(() -> new BillingException("license_not_found", "续费许可证不存在"));
            current = repository.activePeriod(licenseId, now()).orElse(null);
            if (current != null && !current.plan.equals(plan.code) && !isUpgrade(current.plan, plan.code))
                throw new BillingException("plan_change_active", "当前计费周期结束前不能更换套餐");
        }
        boolean upgrade = current != null && isUpgrade(current.plan, plan.code);
        Instant start;
        int quota = plan.quota;
        if (upgrade) {
            start = now();
            for (BillingModels.LicensePeriod remaining : repository.remainingPeriods(licenseId, start)) {
                if ("personal".equals(remaining.plan)) quota += remaining.quota - remaining.used;
            }
        } else {
            start = repository.latestPeriod(licenseId).map(latest -> latest.endsAt).orElse(null);
            if (start == null || start.isBefore(now())) start = now();
        }
        Instant end = ZonedDateTime.ofInstant(start, BILLING_ZONE).plusMonths(1).toInstant();
        BillingModels.LicensePeriod period = new BillingModels.LicensePeriod();
        period.licenseId = licenseId; period.plan = plan.code; period.quota = Math.max(0, quota); period.startsAt = start; period.endsAt = end; period.createdAt = now();
        period.supersedePersonalPeriods = upgrade;
        return repository.completePayment(order.token, tradeNo, now(), newLicense, period, notificationId);
    }

    private static boolean isUpgrade(String currentPlan, String targetPlan) {
        return "personal".equals(currentPlan) && "high".equals(targetPlan);
    }

    public Map<String, Object> activate(String licenseKey, String deviceId) {
        String key = LicenseCrypto.normalizeLicenseKey(licenseKey);
        if (!key.matches("SS-(?:[0-9A-F]{4}-){7}[0-9A-F]{4}")) throw new BillingException("invalid_license", "许可证格式不正确");
        String device = AiTaskQuotaService.device(deviceId); String token = LicenseCrypto.randomHex(32);
        repository.bindLicense(AiTaskQuotaService.hash(key), device, AiTaskQuotaService.hash(token), now());
        Map<String, Object> result = new LinkedHashMap<String, Object>(); result.put("deviceToken", token);
        result.put("entitlement", entitlement(device, token)); return result;
    }

    public Map<String, Object> recover(String deviceId, String deviceToken) {
        BillingModels.License license = repository.licenseByBoundDevice(
                AiTaskQuotaService.device(deviceId), AiTaskQuotaService.hash(deviceToken))
                .orElseThrow(() -> new BillingException("invalid_device_token", "本机设备凭据无效，请重新激活许可证"));
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("licenseKey", crypto.decrypt(license.keyCiphertext));
        return result;
    }

    public Map<String, Object> deactivate(String deviceId, String deviceToken) {
        repository.unbindLicense(AiTaskQuotaService.device(deviceId), AiTaskQuotaService.hash(deviceToken), now());
        return entitlement(deviceId, null);
    }

    private BillingModels.License license(String key) {
        return repository.licenseByKeyHash(AiTaskQuotaService.hash(LicenseCrypto.normalizeLicenseKey(key)))
                .orElseThrow(() -> new BillingException("license_not_found", "许可证不存在或已停用"));
    }
    private BillingModels.Order orderByToken(String token) {
        if (token == null || !token.matches("[a-f0-9]{32}")) throw new BillingException("order_not_found", "订单不存在");
        return repository.order(token).orElseThrow(() -> new BillingException("order_not_found", "订单不存在"));
    }
    private BillingModels.Order pendingOrder(String token) {
        BillingModels.Order order = orderByToken(token);
        if (!"pending".equals(order.status)) throw new BillingException("order_not_pending", "订单已支付或不可支付");
        if (!order.orderExpiresAt.isAfter(now())) throw new BillingException("order_expired", "订单已过期，请重新选择套餐");
        return order;
    }
    private Map<String, Object> orderView(BillingModels.Order order) {
        Map<String, Object> result = new LinkedHashMap<String, Object>(); String status = order.status;
        if ("pending".equals(status) && !order.orderExpiresAt.isAfter(now())) status = "expired";
        result.put("token", order.token); result.put("plan", order.plan); result.put("amount", order.amount); result.put("status", status);
        result.put("createdAt", order.createdAt); result.put("expiresAt", order.orderExpiresAt); result.put("paidAt", order.paidAt);
        result.put("checkoutUrl", publicBaseUrl + "/client-api/v1/checkout/" + order.token);
        result.put("paymentUrl", publicBaseUrl + "/client-api/v1/orders/" + order.token + "/alipay-page");
        result.put("qrImageUrl", publicBaseUrl + "/client-api/v1/orders/" + order.token + "/qr.png");
        result.put("qrExpiresAt", order.qrExpiresAt); result.put("mock", "mock".equals(channel));
        if ("paid".equals(order.status) && order.licenseId != null) {
            BillingModels.License license = repository.licenseById(order.licenseId).orElse(null);
            if (license != null) result.put("licenseKey", crypto.decrypt(license.keyCiphertext));
        }
        return result;
    }
    private Map<String, Object> periodView(BillingModels.LicensePeriod period, Instant now) {
        String status = period.startsAt.isAfter(now) ? "scheduled" : !period.endsAt.isAfter(now) ? "expired" : "active";
        Map<String, Object> result = new LinkedHashMap<String, Object>(); result.put("id", period.id); result.put("plan", period.plan);
        result.put("quota", period.quota); result.put("used", period.used); result.put("remaining", Math.max(0, period.quota - period.used));
        result.put("status", status); result.put("exhausted", period.used >= period.quota);
        result.put("startsAt", period.startsAt); result.put("endsAt", period.endsAt); return result;
    }
    private Map<String, Object> entitlementView(BillingModels.Entitlement value) {
        Map<String, Object> result = new LinkedHashMap<String, Object>(); result.put("plan", value.plan);
        result.put("licensed", value.licensed); result.put("bound", value.bound); result.put("quota", value.quota);
        result.put("used", value.used); result.put("remaining", value.remaining()); result.put("periodStart", value.startsAt);
        result.put("periodEnd", value.endsAt); result.put("paymentEnabled", !"disabled".equals(channel));
        result.put("expired", value.expired); result.put("previousPlan", value.previousPlan); result.put("previousPeriodEnd", value.previousPeriodEnd);
        result.put("mockPayment", "mock".equals(channel)); result.put("paymentAmountOverride", paymentAmountOverride); return result;
    }
    private void requirePayment() { if ("disabled".equals(channel)) throw new BillingException("payment_unavailable", "支付通道尚未开启"); }
    private Instant now() { return clock.instant(); }
}
