package com.sparrowshell.ai;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.dao.EmptyResultDataAccessException;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.jdbc.core.RowMapper;
import org.springframework.stereotype.Repository;
import org.springframework.transaction.annotation.Transactional;
import java.math.BigDecimal;
import java.sql.Timestamp;
import java.time.Instant;
import java.util.List;
import java.util.Map;
import java.util.Optional;

@Repository
@ConditionalOnProperty(prefix = "sparrowshell.usage", name = "enabled", havingValue = "true")
class JdbcAiUsageLedgerRepository implements AiUsageLedgerRepository {
    private static final String COLUMNS = "account_id,client_request_id,request_fingerprint,route_id,operation,status,"
            + "estimated_cost,input_tokens,output_tokens,actual_cost,usage_status,price_version,provider_request_id,error_code,created_at,updated_at";
    private static final RowMapper<AiUsage> MAPPER = (rs, row) -> {
        AiUsage usage = new AiUsage();
        usage.accountId = rs.getString("account_id");
        usage.clientRequestId = rs.getString("client_request_id");
        usage.requestFingerprint = rs.getString("request_fingerprint");
        usage.routeId = rs.getString("route_id");
        usage.operation = rs.getString("operation");
        usage.status = rs.getString("status");
        usage.estimatedCost = rs.getBigDecimal("estimated_cost");
        usage.inputTokens = (Long) rs.getObject("input_tokens");
        usage.outputTokens = (Long) rs.getObject("output_tokens");
        usage.actualCost = rs.getBigDecimal("actual_cost");
        usage.usageStatus = rs.getString("usage_status");
        usage.priceVersion = rs.getString("price_version");
        usage.providerRequestId = rs.getString("provider_request_id");
        usage.errorCode = rs.getString("error_code");
        usage.createdAt = rs.getTimestamp("created_at").toInstant();
        usage.updatedAt = rs.getTimestamp("updated_at").toInstant();
        return usage;
    };
    private final JdbcTemplate jdbc;

    JdbcAiUsageLedgerRepository(JdbcTemplate jdbc) { this.jdbc = jdbc; }

    @Override @Transactional
    public AiUsage reserve(AiUsage requested, BigDecimal quota) {
        jdbc.update("INSERT IGNORE INTO ai_account_quota(account_id,quota_limit,reserved_cost) VALUES (?,?,0)",
                requested.accountId, quota);
        Map<String, Object> account = jdbc.queryForMap(
                "SELECT quota_limit,reserved_cost FROM ai_account_quota WHERE account_id=? FOR UPDATE", requested.accountId);
        Optional<AiUsage> existing = findForUpdate(requested.accountId, requested.clientRequestId);
        if (existing.isPresent()) {
            AiUsage usage = existing.get();
            if (!requested.requestFingerprint.equals(usage.requestFingerprint)
                    || !requested.routeId.equals(usage.routeId) || !requested.operation.equals(usage.operation))
                throw new ProviderException("duplicate_request", "请求编号已用于不同内容");
            return usage.copy(false);
        }
        BigDecimal reserved = (BigDecimal) account.get("reserved_cost");
        BigDecimal limit = (BigDecimal) account.get("quota_limit");
        if (reserved.add(requested.estimatedCost).compareTo(limit) > 0)
            throw new ProviderException("quota_exceeded", "AI 配额不足");
        jdbc.update("UPDATE ai_account_quota SET reserved_cost=reserved_cost+? WHERE account_id=?",
                requested.estimatedCost, requested.accountId);
        jdbc.update("INSERT INTO ai_usage_ledger(account_id,client_request_id,request_fingerprint,route_id,operation,status,"
                        + "estimated_cost,usage_status,price_version) VALUES (?,?,?,?,?,?,?,?,?)",
                requested.accountId, requested.clientRequestId, requested.requestFingerprint, requested.routeId,
                requested.operation, requested.status, requested.estimatedCost, requested.usageStatus, requested.priceVersion);
        return findForUpdate(requested.accountId, requested.clientRequestId).get().copy(true);
    }

    @Override @Transactional
    public void settle(AiUsage settlement) {
        AiUsage current = findForUpdate(settlement.accountId, settlement.clientRequestId)
                .orElseThrow(() -> new IllegalStateException("usage reservation missing"));
        if (!"reserved".equals(current.status)) return;
        jdbc.update("UPDATE ai_account_quota SET reserved_cost=GREATEST(0,reserved_cost-?+?) WHERE account_id=?",
                current.estimatedCost, settlement.actualCost == null ? BigDecimal.ZERO : settlement.actualCost,
                settlement.accountId);
        jdbc.update("UPDATE ai_usage_ledger SET status=?,input_tokens=?,output_tokens=?,actual_cost=?,usage_status=?,"
                        + "provider_request_id=?,error_code=? WHERE account_id=? AND client_request_id=? AND status='reserved'",
                settlement.status, settlement.inputTokens, settlement.outputTokens, settlement.actualCost,
                settlement.usageStatus, settlement.providerRequestId, settlement.errorCode,
                settlement.accountId, settlement.clientRequestId);
    }

    @Override @Transactional(readOnly = true)
    public Optional<AiUsage> find(String accountId, String clientRequestId) {
        try {
            return Optional.of(jdbc.queryForObject("SELECT " + COLUMNS
                    + " FROM ai_usage_ledger WHERE account_id=? AND client_request_id=?", MAPPER, accountId, clientRequestId));
        } catch (EmptyResultDataAccessException exception) { return Optional.empty(); }
    }

    @Override @Transactional(readOnly = true)
    public List<AiUsage> between(Instant from, Instant to) {
        return jdbc.query("SELECT " + COLUMNS + " FROM ai_usage_ledger WHERE created_at>=? AND created_at<?",
                MAPPER, Timestamp.from(from), Timestamp.from(to));
    }

    private Optional<AiUsage> findForUpdate(String accountId, String clientRequestId) {
        List<AiUsage> result = jdbc.query("SELECT " + COLUMNS
                + " FROM ai_usage_ledger WHERE account_id=? AND client_request_id=? FOR UPDATE",
                MAPPER, accountId, clientRequestId);
        return result.isEmpty() ? Optional.empty() : Optional.of(result.get(0));
    }
}
