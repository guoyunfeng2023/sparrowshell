package com.sparrowshell.ai.platform.job;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Repository;
import java.time.Instant;
import java.util.List;
import java.util.Optional;

@Repository
@ConditionalOnProperty(prefix = "sparrowshell.platform", name = "persistence-enabled", havingValue = "false", matchIfMissing = true)
class UnavailableBackgroundJobRepository implements BackgroundJobRepository {
    private IllegalStateException unavailable() { return new IllegalStateException("background job persistence is unavailable"); }
    @Override public BackgroundJob enqueue(BackgroundJob job) { throw unavailable(); }
    @Override public List<String> candidateIds(Instant now, int limit) { throw unavailable(); }
    @Override public Optional<BackgroundJob> claim(String jobId, String owner, Instant now, Instant until) { throw unavailable(); }
    @Override public boolean complete(String jobId, String owner, Instant completedAt) { throw unavailable(); }
    @Override public boolean fail(String jobId, String owner, String code, String status, Instant availableAt) { throw unavailable(); }
    @Override public Optional<BackgroundJob> find(String jobId) { throw unavailable(); }
}
