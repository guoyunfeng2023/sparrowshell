package com.sparrowshell.ai.platform.billing;

import com.alipay.api.AlipayApiException;
import com.alipay.api.AlipayClient;
import com.alipay.api.DefaultAlipayClient;
import com.alipay.api.internal.util.AlipaySignature;
import com.alipay.api.domain.AlipayTradePagePayModel;
import com.alipay.api.domain.AlipayTradePrecreateModel;
import com.alipay.api.domain.AlipayTradeQueryModel;
import com.alipay.api.request.AlipayTradePagePayRequest;
import com.alipay.api.request.AlipayTradePrecreateRequest;
import com.alipay.api.request.AlipayTradeQueryRequest;
import com.alipay.api.response.AlipayTradePagePayResponse;
import com.alipay.api.response.AlipayTradePrecreateResponse;
import com.alipay.api.response.AlipayTradeQueryResponse;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Component;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.Map;

@Component
class AlipayPaymentGateway implements PaymentGateway {
    private final String channel;
    private final String publicBaseUrl;
    private final String gatewayUrl;
    private final String appId;
    private final String privateKey;
    private final String alipayPublicKey;
    private final String sellerId;

    AlipayPaymentGateway(@Value("${sparrowshell.billing.payment-channel:disabled}") String channel,
            @Value("${sparrowshell.public-base-url:http://127.0.0.1:8070}") String publicBaseUrl,
            @Value("${sparrowshell.alipay.gateway:https://openapi.alipay.com/gateway.do}") String gatewayUrl,
            @Value("${sparrowshell.alipay.app-id:}") String appId,
            @Value("${sparrowshell.alipay.private-key:}") String privateKey,
            @Value("${sparrowshell.alipay.public-key:}") String alipayPublicKey,
            @Value("${sparrowshell.alipay.seller-id:}") String sellerId) {
        this.channel = channel; this.publicBaseUrl = publicBaseUrl.replaceAll("/+$", ""); this.gatewayUrl = gatewayUrl;
        this.appId = appId; this.privateKey = privateKey; this.alipayPublicKey = alipayPublicKey; this.sellerId = sellerId;
    }

    @Override public Precreate precreate(BillingModels.Order order, String notifyUrl) {
        Instant expiry = Instant.now().plus(5, ChronoUnit.MINUTES);
        if ("mock".equals(channel)) return new Precreate(publicBaseUrl + "/client-api/v1/orders/" + order.token + "/mock-pay", expiry);
        requireAlipay();
        try {
            AlipayTradePrecreateRequest request = new AlipayTradePrecreateRequest(); request.setNotifyUrl(notifyUrl);
            AlipayTradePrecreateModel model = new AlipayTradePrecreateModel(); model.setOutTradeNo(order.token);
            model.setTotalAmount(order.amount.toPlainString()); model.setSubject("SparrowShell AI " + order.plan);
            request.setBizModel(model); AlipayTradePrecreateResponse response = client().execute(request);
            if (!response.isSuccess() || response.getQrCode() == null || response.getQrCode().trim().isEmpty())
                throw new BillingException("alipay_unavailable", "支付宝二维码暂时无法生成");
            return new Precreate(response.getQrCode(), expiry);
        } catch (AlipayApiException exception) { throw new BillingException("alipay_unavailable", "支付宝预下单失败，请稍后重试"); }
    }

    @Override public String pagePay(BillingModels.Order order, String notifyUrl, String returnUrl) {
        requireAlipay();
        try {
            AlipayTradePagePayRequest request = new AlipayTradePagePayRequest();
            request.setNotifyUrl(notifyUrl); request.setReturnUrl(returnUrl);
            AlipayTradePagePayModel model = new AlipayTradePagePayModel(); model.setOutTradeNo(order.token);
            model.setTotalAmount(order.amount.toPlainString()); model.setSubject("SparrowShell AI " + order.plan);
            model.setProductCode("FAST_INSTANT_TRADE_PAY"); request.setBizModel(model);
            AlipayTradePagePayResponse response = client().pageExecute(request);
            if (!response.isSuccess() || response.getBody() == null || response.getBody().trim().isEmpty())
                throw new BillingException("alipay_unavailable", "支付宝收银台暂时无法打开");
            return response.getBody();
        } catch (AlipayApiException exception) {
            throw new BillingException("alipay_unavailable", "支付宝网页支付下单失败，请稍后重试");
        }
    }

    @Override public Query query(BillingModels.Order order) {
        if ("mock".equals(channel)) return new Query(false, null);
        requireAlipay();
        try {
            AlipayTradeQueryRequest request = new AlipayTradeQueryRequest(); AlipayTradeQueryModel model = new AlipayTradeQueryModel();
            model.setOutTradeNo(order.token); request.setBizModel(model); AlipayTradeQueryResponse response = client().execute(request);
            if (!response.isSuccess()) return new Query(false, null);
            if (!order.token.equals(response.getOutTradeNo()) || order.amount.compareTo(new java.math.BigDecimal(response.getTotalAmount())) != 0)
                throw new BillingException("payment_verification_failed", "支付宝查单结果与订单不一致");
            boolean paid = "TRADE_SUCCESS".equals(response.getTradeStatus()) || "TRADE_FINISHED".equals(response.getTradeStatus());
            return new Query(paid, response.getTradeNo());
        } catch (AlipayApiException | NumberFormatException exception) { throw new BillingException("alipay_unavailable", "支付宝查单失败，请稍后重试"); }
    }

    @Override public boolean validNotification(Map<String, String> parameters, BillingModels.Order order) {
        if (!"alipay".equals(channel)) return false;
        requireAlipay();
        try {
            if (!AlipaySignature.rsaCheckV1(parameters, alipayPublicKey, "UTF-8", "RSA2")) return false;
            if (!appId.equals(parameters.get("app_id")) || !order.token.equals(parameters.get("out_trade_no"))) return false;
            if (sellerId != null && !sellerId.trim().isEmpty() && !sellerId.equals(parameters.get("seller_id"))) return false;
            return order.amount.compareTo(new java.math.BigDecimal(parameters.get("total_amount"))) == 0;
        } catch (AlipayApiException | NumberFormatException exception) { return false; }
    }

    @Override public String paidTradeNo(Map<String, String> parameters, BillingModels.Order order) {
        String status = parameters.get("trade_status"); String tradeNo = parameters.get("trade_no");
        if (!("TRADE_SUCCESS".equals(status) || "TRADE_FINISHED".equals(status)) || tradeNo == null || tradeNo.length() > 64) return null;
        return tradeNo;
    }

    private AlipayClient client() { return new DefaultAlipayClient(gatewayUrl, appId, privateKey, "json", "UTF-8", alipayPublicKey, "RSA2"); }
    private void requireAlipay() {
        if (!"alipay".equals(channel)) throw new BillingException("payment_unavailable", "支付通道尚未开启");
        if (appId.trim().isEmpty() || privateKey.trim().isEmpty() || alipayPublicKey.trim().isEmpty())
            throw new BillingException("payment_unavailable", "支付宝商户参数尚未配置");
    }
}
