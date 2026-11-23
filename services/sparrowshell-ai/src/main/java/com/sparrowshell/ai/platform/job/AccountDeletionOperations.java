package com.sparrowshell.ai.platform.job;

public interface AccountDeletionOperations {
    void executeStep(String accountId, String step) throws Exception;
}
