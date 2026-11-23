package com.sparrowshell.ai;

import java.math.BigDecimal;
import java.time.Instant;

/** Billing metadata only: never stores prompts, command output, targets or credentials. */
public class AiUsage {
    public String accountId;
    public String clientRequestId;
    public String requestFingerprint;
    public String routeId;
    public String operation;
    public String status;
    public BigDecimal estimatedCost;
    public Long inputTokens;
    public Long outputTokens;
    public BigDecimal actualCost;
    public String usageStatus;
    public String priceVersion;
    public String providerRequestId;
    public String errorCode;
    public Instant createdAt;
    public Instant updatedAt;
    public boolean newReservation;

    AiUsage copy(boolean isNew) {
        AiUsage copy = new AiUsage();
        copy.accountId = accountId;
        copy.clientRequestId = clientRequestId;
        copy.requestFingerprint = requestFingerprint;
        copy.routeId = routeId;
        copy.operation = operation;
        copy.status = status;
        copy.estimatedCost = estimatedCost;
        copy.inputTokens = inputTokens;
        copy.outputTokens = outputTokens;
        copy.actualCost = actualCost;
        copy.usageStatus = usageStatus;
        copy.priceVersion = priceVersion;
        copy.providerRequestId = providerRequestId;
        copy.errorCode = errorCode;
        copy.createdAt = createdAt;
        copy.updatedAt = updatedAt;
        copy.newReservation = isNew;
        return copy;
    }
}
