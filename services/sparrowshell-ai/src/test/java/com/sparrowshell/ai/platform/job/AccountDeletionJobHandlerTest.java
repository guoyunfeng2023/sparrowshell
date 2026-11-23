package com.sparrowshell.ai.platform.job;

import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;
import java.util.*;
import static org.junit.jupiter.api.Assertions.*;

class AccountDeletionJobHandlerTest {
    private static final String ACCOUNT = "4d3a34f1-9b27-4b5d-9766-e6c8a7dc5c1a";

    @Test void executesDeletionStepsInOrderAndReplayIsIdempotent() throws Exception {
        RecordingOperations operations = new RecordingOperations();
        AccountDeletionJobHandler handler = new AccountDeletionJobHandler(operations, new ObjectMapper());
        BackgroundJob job = job();
        handler.execute(job);
        handler.execute(job);
        assertEquals(AccountDeletionJobHandler.STEPS, operations.completed);
        assertEquals(AccountDeletionJobHandler.STEPS, operations.attempted);
    }

    @Test void retryResumesAfterLastCommittedStep() throws Exception {
        RecordingOperations operations = new RecordingOperations();
        operations.failAt = "delete_ai_data";
        AccountDeletionJobHandler handler = new AccountDeletionJobHandler(operations, new ObjectMapper());
        assertThrows(JobFailureException.class, () -> handler.execute(job()));
        assertEquals(Arrays.asList("revoke_sessions", "delete_sync_data"), operations.completed);
        operations.failAt = null;
        handler.execute(job());
        assertEquals(AccountDeletionJobHandler.STEPS, operations.completed);
        assertEquals(5, operations.attempted.size());
    }

    @Test void rejectsMalformedAccountWithoutDeletingAnything() {
        RecordingOperations operations = new RecordingOperations();
        AccountDeletionJobHandler handler = new AccountDeletionJobHandler(operations, new ObjectMapper());
        BackgroundJob job = job();
        job.payload = "{\"accountId\":\"not-a-uuid\",\"requestId\":\"request\"}";
        assertThrows(IllegalArgumentException.class, () -> handler.execute(job));
        assertTrue(operations.attempted.isEmpty());
    }

    private static BackgroundJob job() {
        BackgroundJob job = new BackgroundJob();
        job.jobId = UUID.randomUUID().toString(); job.jobType = "account_deletion";
        job.payload = "{\"accountId\":\"" + ACCOUNT + "\",\"requestId\":\"deletion-request\"}";
        return job;
    }

    private static class RecordingOperations implements AccountDeletionOperations {
        final List<String> attempted = new ArrayList<String>();
        final List<String> completed = new ArrayList<String>();
        String failAt;
        @Override public void executeStep(String accountId, String step) throws Exception {
            assertEquals(ACCOUNT, accountId);
            if (completed.contains(step)) return;
            attempted.add(step);
            if (step.equals(failAt)) throw new JobFailureException("deletion_step_failed", true);
            completed.add(step);
        }
    }
}
