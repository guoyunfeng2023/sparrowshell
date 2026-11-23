package com.sparrowshell.ai.platform.billing;

import com.sparrowshell.ai.*;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Component;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.UUID;
import java.util.function.Supplier;

/** One user task consumes one quota unit, regardless of the Agent's model/tool turn count. */
@Component
public class AiTaskQuotaService {
    private final BillingRepository repository;
    private final boolean enabled;
    private final long reservationMinutes;

    public AiTaskQuotaService(BillingRepository repository,
            @Value("${sparrowshell.billing.enabled:false}") boolean enabled,
            @Value("${sparrowshell.billing.reservation-minutes:15}") long reservationMinutes) {
        this.repository = repository; this.enabled = enabled; this.reservationMinutes = Math.max(5, Math.min(60, reservationMinutes));
    }

    public boolean isEnabled() { return enabled; }

    public ProposalResponse propose(String deviceId, String deviceToken, ProposalRequest request, Supplier<ProposalResponse> provider) {
        if (!enabled) return provider.get();
        String taskId = uuid(request == null ? null : request.clientRequestId, "clientRequestId");
        return oneShot(deviceId, deviceToken, taskId, provider);
    }

    public AnalysisResponse analyze(String deviceId, String deviceToken, AnalysisRequest request, Supplier<AnalysisResponse> provider) {
        if (!enabled) return provider.get();
        String taskId = uuid(request == null ? null : request.clientRequestId, "clientRequestId");
        return oneShot(deviceId, deviceToken, taskId, provider);
    }

    private <T> T oneShot(String deviceId, String deviceToken, String taskId, Supplier<T> provider) {
        String device = device(deviceId);
        reserve(device, deviceToken, taskId);
        try {
            T response = provider.get();
            repository.settleTask(device, taskId, true, Instant.now());
            return response;
        } catch (RuntimeException exception) {
            releaseQuietly(device, taskId);
            throw exception;
        }
    }

    public AgentTurnResponse agentTurn(String deviceId, String deviceToken, AgentTurnRequest request, Supplier<AgentTurnResponse> provider) {
        if (!enabled) return provider.get();
        String taskId = uuid(request == null ? null : request.taskId, "taskId");
        String device = device(deviceId);
        reserve(device, deviceToken, taskId);
        try {
            AgentTurnResponse response = provider.get();
            if ("final_answer".equals(response.type)) repository.settleTask(device, taskId, true, Instant.now());
            else if (!"tool_call".equals(response.type)) {
                repository.settleTask(device, taskId, false, Instant.now());
                throw new ProviderException("invalid_provider_response", "模型响应无效");
            }
            return response;
        } catch (RuntimeException exception) {
            releaseQuietly(device, taskId);
            throw exception;
        }
    }

    public void release(String deviceId, String taskId) {
        if (!enabled) return;
        repository.releaseTask(device(deviceId), uuid(taskId, "taskId"), Instant.now());
    }

    private void reserve(String deviceId, String deviceToken, String taskId) {
        try {
            repository.reserveTask(device(deviceId), hash(deviceToken), taskId, Instant.now(),
                    Instant.now().plus(reservationMinutes, ChronoUnit.MINUTES));
        } catch (BillingException exception) {
            throw new ProviderException(exception.code, exception.getMessage());
        }
    }

    private void releaseQuietly(String deviceId, String taskId) {
        try { repository.settleTask(device(deviceId), taskId, false, Instant.now()); }
        catch (RuntimeException ignored) { /* The reservation TTL is the final recovery mechanism. */ }
    }

    static String device(String value) {
        try { return UUID.fromString(value == null ? "" : value.trim()).toString(); }
        catch (IllegalArgumentException exception) { throw new ProviderException("invalid_identity", "本机安装标识无效，请重新启动客户端"); }
    }
    static String uuid(String value, String field) {
        try { return UUID.fromString(value == null ? "" : value.trim()).toString(); }
        catch (IllegalArgumentException exception) { throw new ProviderException("invalid_request", field + " 必须是 UUID"); }
    }
    public static String hash(String value) {
        if (value == null || value.trim().isEmpty()) return null;
        try {
            byte[] bytes = MessageDigest.getInstance("SHA-256").digest(value.trim().getBytes(StandardCharsets.UTF_8));
            StringBuilder result = new StringBuilder(64);
            for (byte item : bytes) result.append(String.format("%02x", item & 0xff));
            return result.toString();
        } catch (Exception exception) { throw new IllegalStateException("SHA-256 unavailable", exception); }
    }
}
