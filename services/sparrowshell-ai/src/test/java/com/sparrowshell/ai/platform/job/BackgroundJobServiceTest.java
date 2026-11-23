package com.sparrowshell.ai.platform.job;

import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import java.time.Instant;
import java.util.*;
import java.util.concurrent.atomic.AtomicInteger;
import static org.junit.jupiter.api.Assertions.*;

class BackgroundJobServiceTest {
    private FakeRepository repository;

    @BeforeEach void setUp() { repository = new FakeRepository(); }

    @Test void duplicateKeyReturnsSameJobButCannotBeReusedForDifferentPayload() {
        BackgroundJobService service = service(new NoopHandler());
        BackgroundJob first = service.enqueue("email", Collections.singletonMap("to", "a@example.test"), "welcome:1", 3);
        BackgroundJob duplicate = service.enqueue("email", Collections.singletonMap("to", "a@example.test"), "welcome:1", 3);
        assertEquals(first.jobId, duplicate.jobId);
        assertThrows(IllegalArgumentException.class,
                () -> service.enqueue("email", Collections.singletonMap("to", "b@example.test"), "welcome:1", 3));
    }

    @Test void retriesWithBackoffThenMovesToDeadLetter() {
        AtomicInteger executions = new AtomicInteger();
        BackgroundJobService service = service(new JobHandler() {
            @Override public String type() { return "email"; }
            @Override public void execute(BackgroundJob job) throws Exception {
                executions.incrementAndGet();
                throw new JobFailureException("smtp_timeout", true);
            }
        });
        BackgroundJob enqueued = service.enqueue("email", Collections.singletonMap("id", 1), "mail:1", 2);
        BackgroundJob first = service.runNext("worker-a").get();
        assertEquals("retry", first.status);
        assertEquals(1, first.attempts);
        assertEquals("smtp_timeout", first.lastErrorCode);
        assertTrue(!first.availableAt.isAfter(Instant.now().plusSeconds(2)));
        first.availableAt = Instant.now().minusSeconds(1);
        BackgroundJob second = service.runNext("worker-a").get();
        assertEquals("dead_letter", second.status);
        assertEquals(2, second.attempts);
        assertEquals(2, executions.get());
        assertNotNull(second.completedAt);
        assertTrue(BackgroundJobService.backoffSeconds(1) < BackgroundJobService.backoffSeconds(4));
        assertEquals(300, BackgroundJobService.backoffSeconds(20));
    }

    @Test void nonRetryableAndMissingHandlersDeadLetterImmediately() {
        BackgroundJobService service = service(new JobHandler() {
            @Override public String type() { return "email"; }
            @Override public void execute(BackgroundJob job) throws Exception { throw new JobFailureException("bad_address", false); }
        });
        service.enqueue("email", Collections.singletonMap("id", 1), "bad:1", 10);
        assertEquals("dead_letter", service.runNext("worker-a").get().status);
        service.enqueue("unknown", Collections.singletonMap("id", 1), "unknown:1", 10);
        BackgroundJob missing = service.runNext("worker-a").get();
        assertEquals("dead_letter", missing.status);
        assertEquals("handler_unavailable", missing.lastErrorCode);
    }

    @Test void onlyLeaseOwnerCanCompleteAndSettleFailure() {
        BackgroundJobService service = service(new NoopHandler());
        BackgroundJob job = service.enqueue("email", Collections.singletonMap("id", 1), "lease:1", 3);
        repository.stealNextLease = true;
        IllegalStateException error = assertThrows(IllegalStateException.class, () -> service.runNext("worker-a"));
        assertTrue(error.getMessage().contains("lease"));
        assertEquals("running", repository.jobs.get(job.jobId).status);
        assertEquals("other-worker", repository.jobs.get(job.jobId).leaseOwner);
    }

    @Test void expiredLeaseCanBeReclaimedButAttemptIsBounded() {
        BackgroundJobService service = service(new NoopHandler());
        BackgroundJob job = service.enqueue("email", Collections.singletonMap("id", 1), "recover:1", 3);
        BackgroundJob claimed = repository.claim(job.jobId, "dead-worker", Instant.now(), Instant.now().minusSeconds(60)).get();
        assertEquals(1, claimed.attempts);
        BackgroundJob completed = service.runNext("live-worker").get();
        assertEquals("completed", completed.status);
        assertEquals(2, completed.attempts);
        assertNull(completed.leaseOwner);
        assertFalse(service.runNext("live-worker").isPresent());
    }

    @Test void validatesWorkerAndJobLimits() {
        BackgroundJobService service = service(new NoopHandler());
        assertThrows(IllegalArgumentException.class, () -> service.runNext("bad worker"));
        assertThrows(IllegalArgumentException.class, () -> service.enqueue("BAD", new Object(), "key", 3));
        assertThrows(IllegalArgumentException.class, () -> service.enqueue("email", new Object(), "key", 21));
    }

    private BackgroundJobService service(JobHandler... handlers) {
        return new BackgroundJobService(repository, new ObjectMapper(), Arrays.asList(handlers));
    }

    private static class NoopHandler implements JobHandler {
        @Override public String type() { return "email"; }
        @Override public void execute(BackgroundJob job) { }
    }

    private static class FakeRepository implements BackgroundJobRepository {
        final Map<String, BackgroundJob> jobs = new LinkedHashMap<String, BackgroundJob>();
        boolean stealNextLease;
        @Override public BackgroundJob enqueue(BackgroundJob job) {
            for (BackgroundJob existing : jobs.values()) {
                if (existing.jobType.equals(job.jobType) && existing.dedupeKey.equals(job.dedupeKey)) {
                    if (!existing.payload.equals(job.payload)) throw new IllegalArgumentException("dedupe key reused");
                    return existing;
                }
            }
            jobs.put(job.jobId, job);
            return job;
        }
        @Override public List<String> candidateIds(Instant now, int limit) {
            List<String> ids = new ArrayList<String>();
            for (BackgroundJob job : jobs.values()) {
                if ((("pending".equals(job.status) || "retry".equals(job.status)) && !job.availableAt.isAfter(now))
                        || ("running".equals(job.status) && job.leaseUntil != null && !job.leaseUntil.isAfter(now))) ids.add(job.jobId);
            }
            return ids;
        }
        @Override public Optional<BackgroundJob> claim(String id, String owner, Instant now, Instant until) {
            BackgroundJob job = jobs.get(id);
            if (job == null) return Optional.empty();
            boolean eligible = (("pending".equals(job.status) || "retry".equals(job.status)) && !job.availableAt.isAfter(now))
                    || ("running".equals(job.status) && job.leaseUntil != null && !job.leaseUntil.isAfter(now));
            if (!eligible) return Optional.empty();
            job.status = "running"; job.leaseOwner = owner; job.leaseUntil = until; job.attempts++;
            return Optional.of(job);
        }
        @Override public boolean complete(String id, String owner, Instant completedAt) {
            BackgroundJob job = jobs.get(id);
            if (stealNextLease) {
                stealNextLease = false; job.leaseOwner = "other-worker"; return false;
            }
            if (job == null || !"running".equals(job.status) || !owner.equals(job.leaseOwner)) return false;
            job.status = "completed"; job.completedAt = completedAt; job.leaseOwner = null; job.leaseUntil = null;
            return true;
        }
        @Override public boolean fail(String id, String owner, String code, String status, Instant availableAt) {
            BackgroundJob job = jobs.get(id);
            if (job == null || !"running".equals(job.status) || !owner.equals(job.leaseOwner)) return false;
            job.status = status; job.lastErrorCode = code; job.availableAt = availableAt;
            job.leaseOwner = null; job.leaseUntil = null;
            if ("dead_letter".equals(status)) job.completedAt = Instant.now();
            return true;
        }
        @Override public Optional<BackgroundJob> find(String id) { return Optional.ofNullable(jobs.get(id)); }
    }
}
