package com.sparrowshell.ai;

import com.fasterxml.jackson.databind.ObjectMapper;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Component;
import java.math.BigDecimal;
import java.math.RoundingMode;
import java.security.MessageDigest;
import java.time.Instant;
import java.util.Map;
import java.util.UUID;
import java.util.function.Supplier;

/** Short DB reservations surround, but never contain, the Provider network call. */
@Component
class AiUsageService {
    private static final BigDecimal MILLION = new BigDecimal("1000000");
    private final AiUsageLedgerRepository repository;
    private final ObjectMapper mapper;
    private final boolean enabled;
    private final boolean required;
    private final boolean localDevelopment;
    private final String localSubject;
    private final BigDecimal quota;
    private final BigDecimal estimatedCost;
    private final BigDecimal inputPerMillion;
    private final BigDecimal outputPerMillion;
    private final String priceVersion;

    @Autowired
    AiUsageService(ObjectProvider<AiUsageLedgerRepository> repository, ObjectMapper mapper,
                   @Value("${sparrowshell.usage.enabled:false}") boolean enabled,
                   @Value("${sparrowshell.usage.required:false}") boolean required,
                   @Value("${sparrowshell.usage.local-development:false}") boolean localDevelopment,
                   @Value("${sparrowshell.usage.local-subject:}") String localSubject,
                   @Value("${sparrowshell.usage.quota:2.00}") BigDecimal quota,
                   @Value("${sparrowshell.usage.estimated-cost:0.05}") BigDecimal estimatedCost,
                   @Value("${sparrowshell.usage.input-per-million:1.00}") BigDecimal inputPerMillion,
                   @Value("${sparrowshell.usage.output-per-million:2.00}") BigDecimal outputPerMillion,
                   @Value("${sparrowshell.usage.price-version:unconfigured}") String priceVersion) {
        this(repository.getIfAvailable(), mapper, enabled, required, localDevelopment, localSubject,
                quota, estimatedCost, inputPerMillion, outputPerMillion, priceVersion);
    }

    AiUsageService(AiUsageLedgerRepository repository, ObjectMapper mapper, boolean enabled, boolean required,
                   String localSubject, BigDecimal quota, BigDecimal estimatedCost,
                   BigDecimal inputPerMillion, BigDecimal outputPerMillion, String priceVersion) {
        this(repository, mapper, enabled, required, true, localSubject, quota, estimatedCost,
                inputPerMillion, outputPerMillion, priceVersion);
    }

    private AiUsageService(AiUsageLedgerRepository repository, ObjectMapper mapper, boolean enabled, boolean required,
                           boolean localDevelopment, String localSubject, BigDecimal quota, BigDecimal estimatedCost,
                           BigDecimal inputPerMillion, BigDecimal outputPerMillion, String priceVersion) {
        this.repository = repository;
        this.mapper = mapper;
        this.enabled = enabled;
        this.required = required;
        this.localDevelopment = localDevelopment;
        this.localSubject = localSubject == null ? "" : localSubject.trim();
        this.quota = quota;
        this.estimatedCost = estimatedCost;
        this.inputPerMillion = inputPerMillion;
        this.outputPerMillion = outputPerMillion;
        this.priceVersion = priceVersion;
    }

    ProposalResponse propose(ProposalRequest request, Supplier<ProposalResponse> provider) {
        return propose(legacySubject(), request, provider);
    }

    ProposalResponse propose(String subject, ProposalRequest request, Supplier<ProposalResponse> provider) {
        String routeId = request == null ? null : request.routeId;
        String clientRequestId = request == null ? null : request.clientRequestId;
        if (!meteringApplies(subject)) return provider.get();
        AiUsage reservation = reserve(subject, clientRequestId, routeId, "proposal", request);
        try {
            ProposalResponse response = provider.get();
            settleSuccess(reservation, response.requestId, response.usage);
            return response;
        } catch (RuntimeException exception) {
            settleFailure(reservation, exception);
            throw exception;
        }
    }

    AnalysisResponse analyze(AnalysisRequest request, Supplier<AnalysisResponse> provider) {
        return analyze(legacySubject(), request, provider);
    }

    AnalysisResponse analyze(String subject, AnalysisRequest request, Supplier<AnalysisResponse> provider) {
        String routeId = request == null ? null : request.routeId;
        String clientRequestId = request == null ? null : request.clientRequestId;
        if (!meteringApplies(subject)) return provider.get();
        AiUsage reservation = reserve(subject, clientRequestId, routeId, "analysis", request);
        try {
            AnalysisResponse response = provider.get();
            settleSuccess(reservation, response.requestId, response.usage);
            return response;
        } catch (RuntimeException exception) {
            settleFailure(reservation, exception);
            throw exception;
        }
    }

    AgentTurnResponse agentTurn(AgentTurnRequest request, Supplier<AgentTurnResponse> provider) {
        return agentTurn(legacySubject(), request, provider);
    }

    AgentTurnResponse agentTurn(String subject, AgentTurnRequest request, Supplier<AgentTurnResponse> provider) {
        String routeId = request == null ? null : request.routeId;
        String clientRequestId = request == null ? null : request.clientRequestId;
        if (!meteringApplies(subject)) return provider.get();
        AiUsage reservation = reserve(subject, clientRequestId, routeId, "agent_turn", request);
        try {
            AgentTurnResponse response = provider.get();
            settleSuccess(reservation, response.requestId, response.usage);
            return response;
        } catch (RuntimeException exception) {
            settleFailure(reservation, exception);
            throw exception;
        }
    }

    private String legacySubject() {
        if (!localDevelopment || localSubject.isEmpty())
            throw new ProviderException("usage_unavailable", "本地开发用量身份未配置");
        return localSubject;
    }

    private boolean meteringApplies(String subject) {
        if (!enabled) {
            if (required) throw new ProviderException("usage_unavailable", "AI 用量账本尚未配置");
            return false;
        }
        if (repository == null || subject == null || subject.trim().isEmpty() || subject.length() > 128)
            throw new ProviderException("usage_unavailable", "当前没有安全的 AI 用量身份");
        return true;
    }

    private AiUsage reserve(String subject, String clientRequestId, String routeId, String operation, Object request) {
        try { UUID.fromString(clientRequestId); }
        catch (Exception exception) { throw new ProviderException("invalid_request", "clientRequestId 必须是 UUID"); }
        if (routeId == null || routeId.trim().isEmpty())
            throw new ProviderException("invalid_request", "启用账本时必须指定固定模型路由");
        if (repository.find(subject, clientRequestId).isPresent())
            throw new ProviderException("duplicate_request", "该请求已受理，请勿重复使用请求编号");
        AiUsage usage = new AiUsage();
        usage.accountId = subject;
        usage.clientRequestId = clientRequestId;
        usage.requestFingerprint = fingerprint(request);
        usage.routeId = routeId;
        usage.operation = operation;
        usage.status = "reserved";
        usage.estimatedCost = estimatedCost;
        usage.usageStatus = "pending";
        usage.priceVersion = priceVersion;
        AiUsage reserved = repository.reserve(usage, quota);
        if (!reserved.newReservation) throw new ProviderException("duplicate_request", "该请求已受理，请勿重复使用请求编号");
        return reserved;
    }

    private void settleSuccess(AiUsage reservation, String providerRequestId, Map<String, Object> rawUsage) {
        AiUsage settlement = baseSettlement(reservation, "succeeded");
        settlement.providerRequestId = providerRequestId;
        Long input = token(rawUsage, "prompt_tokens");
        Long output = token(rawUsage, "completion_tokens");
        if (input == null || output == null || input < 0 || output < 0) {
            settlement.usageStatus = "unknown";
        } else {
            settlement.inputTokens = input;
            settlement.outputTokens = output;
            settlement.usageStatus = "reported";
            settlement.actualCost = BigDecimal.valueOf(input).multiply(inputPerMillion)
                    .add(BigDecimal.valueOf(output).multiply(outputPerMillion))
                    .divide(MILLION, 8, RoundingMode.HALF_UP);
        }
        repository.settle(settlement);
    }

    private void settleFailure(AiUsage reservation, RuntimeException exception) {
        AiUsage settlement = baseSettlement(reservation, "failed");
        settlement.errorCode = exception instanceof ProviderException
                ? ((ProviderException) exception).code() : "provider_unavailable";
        repository.settle(settlement);
    }

    private AiUsage baseSettlement(AiUsage reservation, String status) {
        AiUsage settlement = reservation.copy(false);
        settlement.status = status;
        settlement.usageStatus = "unknown";
        settlement.actualCost = null;
        settlement.inputTokens = null;
        settlement.outputTokens = null;
        settlement.updatedAt = Instant.now();
        return settlement;
    }

    private Long token(Map<String, Object> usage, String name) {
        if (usage == null) return null;
        Object value = usage.get(name);
        if (!(value instanceof Number)) return null;
        double number = ((Number) value).doubleValue();
        if (!Double.isFinite(number) || number != Math.rint(number) || number > Long.MAX_VALUE) return null;
        return ((Number) value).longValue();
    }

    private String fingerprint(Object request) {
        try {
            byte[] digest = MessageDigest.getInstance("SHA-256")
                    .digest(mapper.writeValueAsBytes(request));
            StringBuilder result = new StringBuilder(64);
            for (byte value : digest) result.append(String.format("%02x", value & 0xff));
            return result.toString();
        } catch (Exception exception) {
            throw new ProviderException("invalid_request", "无法生成请求指纹");
        }
    }
}
