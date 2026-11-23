package com.sparrowshell.ai.platform.job;

import java.time.Instant;
import java.util.List;
import java.util.Optional;

public interface BackgroundJobRepository {
    BackgroundJob enqueue(BackgroundJob job);
    List<String> candidateIds(Instant now, int limit);
    Optional<BackgroundJob> claim(String jobId, String leaseOwner, Instant now, Instant leaseUntil);
    boolean complete(String jobId, String leaseOwner, Instant completedAt);
    boolean fail(String jobId, String leaseOwner, String errorCode, String nextStatus, Instant availableAt);
    Optional<BackgroundJob> find(String jobId);
}
