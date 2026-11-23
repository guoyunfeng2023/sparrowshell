package com.sparrowshell.ai.platform.job;

import com.fasterxml.jackson.databind.ObjectMapper;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Component;
import java.util.Arrays;
import java.util.List;
import java.util.UUID;

@Component
@ConditionalOnProperty(prefix = "sparrowshell.platform", name = "persistence-enabled", havingValue = "true")
class AccountDeletionJobHandler implements JobHandler {
    static final List<String> STEPS = Arrays.asList("revoke_sessions", "delete_sync_data", "delete_ai_data", "anonymize_account");
    private final AccountDeletionOperations operations;
    private final ObjectMapper objectMapper;
    AccountDeletionJobHandler(AccountDeletionOperations operations, ObjectMapper objectMapper) {
        this.operations = operations; this.objectMapper = objectMapper;
    }
    @Override public String type() { return "account_deletion"; }
    @Override public void execute(BackgroundJob job) throws Exception {
        AccountDeletionPayload payload = objectMapper.readValue(job.payload, AccountDeletionPayload.class);
        UUID.fromString(payload.accountId);
        for (String step : STEPS) operations.executeStep(payload.accountId, step);
    }
    static class AccountDeletionPayload { public String accountId; public String requestId; }
}
