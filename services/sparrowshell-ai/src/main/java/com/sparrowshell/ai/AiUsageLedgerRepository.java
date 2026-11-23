package com.sparrowshell.ai;

import java.math.BigDecimal;
import java.time.Instant;
import java.util.List;
import java.util.Optional;

interface AiUsageLedgerRepository {
    AiUsage reserve(AiUsage requested, BigDecimal quota);
    void settle(AiUsage settlement);
    Optional<AiUsage> find(String accountId, String clientRequestId);
    List<AiUsage> between(Instant from, Instant to);
}
