package com.sparrowshell.ai.platform.job;

import com.fasterxml.jackson.databind.ObjectMapper;
import org.springframework.stereotype.Service;
import java.time.Instant;
import java.util.*;

@Service
public class BackgroundJobService {
    private final BackgroundJobRepository repository;
    private final ObjectMapper objectMapper;
    private final Map<String, JobHandler> handlers;

    public BackgroundJobService(BackgroundJobRepository repository, ObjectMapper objectMapper, List<JobHandler> handlers) {
        this.repository = repository;
        this.objectMapper = objectMapper;
        this.handlers = new HashMap<String, JobHandler>();
        for (JobHandler handler : handlers) this.handlers.put(handler.type(), handler);
    }

    public BackgroundJob enqueue(String type, Object payload, String dedupeKey, int maxAttempts) {
        if (type == null || !type.matches("[a-z0-9_]{1,64}") || dedupeKey == null
                || dedupeKey.trim().isEmpty() || dedupeKey.length() > 128 || maxAttempts < 1 || maxAttempts > 20)
            throw new IllegalArgumentException("invalid background job");
        try {
            BackgroundJob job = new BackgroundJob();
            job.jobId = UUID.randomUUID().toString(); job.jobType = type;
            job.payload = objectMapper.writeValueAsString(payload);
            if (job.payload.length() > 16_384) throw new IllegalArgumentException("background job payload too large");
            job.dedupeKey = dedupeKey; job.status = "pending"; job.availableAt = Instant.now();
            job.attempts = 0; job.maxAttempts = maxAttempts; job.createdAt = Instant.now();
            return repository.enqueue(job);
        } catch (IllegalArgumentException exception) { throw exception; }
        catch (Exception exception) { throw new IllegalArgumentException("background job payload cannot be serialized", exception); }
    }

    public Optional<BackgroundJob> runNext(String workerId) {
        if (workerId == null || !workerId.matches("[a-zA-Z0-9_-]{1,128}")) throw new IllegalArgumentException("invalid worker");
        for (String candidate : repository.candidateIds(Instant.now(), 10)) {
            String lease = workerId + ":" + UUID.randomUUID();
            Optional<BackgroundJob> claimed = repository.claim(candidate, lease, Instant.now(), Instant.now().plusSeconds(60));
            if (!claimed.isPresent()) continue;
            BackgroundJob job = claimed.get();
            JobHandler handler = handlers.get(job.jobType);
            if (handler == null) {
                repository.fail(job.jobId, lease, "handler_unavailable", "dead_letter", job.availableAt);
                return Optional.of(repository.find(job.jobId).orElse(job));
            }
            try {
                handler.execute(job);
                if (!repository.complete(job.jobId, lease, Instant.now())) throw new IllegalStateException("job lease changed during completion");
            } catch (Exception exception) {
                String code = exception instanceof JobFailureException ? ((JobFailureException) exception).code : "handler_failed";
                boolean retryable = !(exception instanceof JobFailureException) || ((JobFailureException) exception).retryable;
                boolean retry = retryable && job.attempts < job.maxAttempts;
                Instant next = retry ? Instant.now().plusSeconds(backoffSeconds(job.attempts)) : job.availableAt;
                String status = retry ? "retry" : "dead_letter";
                if (!repository.fail(job.jobId, lease, code, status, next)) throw new IllegalStateException("job lease changed during failure settlement");
            }
            return Optional.of(repository.find(job.jobId).orElse(job));
        }
        return Optional.empty();
    }

    static long backoffSeconds(int attempts) {
        int exponent = Math.max(0, Math.min(9, attempts - 1));
        return Math.min(300L, 1L << exponent);
    }
}
