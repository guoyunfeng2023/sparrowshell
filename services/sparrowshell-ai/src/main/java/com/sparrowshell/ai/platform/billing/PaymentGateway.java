package com.sparrowshell.ai.platform.billing;

import java.time.Instant;
import java.util.Map;

interface PaymentGateway {
    class Precreate {
        String qrCode;
        Instant expiresAt;
        Precreate(String qrCode, Instant expiresAt) { this.qrCode = qrCode; this.expiresAt = expiresAt; }
    }
    class Query {
        boolean paid;
        String tradeNo;
        Query(boolean paid, String tradeNo) { this.paid = paid; this.tradeNo = tradeNo; }
    }
    Precreate precreate(BillingModels.Order order, String notifyUrl);
    String pagePay(BillingModels.Order order, String notifyUrl, String returnUrl);
    Query query(BillingModels.Order order);
    boolean validNotification(Map<String, String> parameters, BillingModels.Order order);
    String paidTradeNo(Map<String, String> parameters, BillingModels.Order order);
}
