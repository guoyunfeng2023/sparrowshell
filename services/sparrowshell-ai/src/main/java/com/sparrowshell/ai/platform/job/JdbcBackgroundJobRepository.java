package com.sparrowshell.ai.platform.job;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.dao.DuplicateKeyException;
import org.springframework.dao.EmptyResultDataAccessException;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.jdbc.core.RowMapper;
import org.springframework.stereotype.Repository;
import org.springframework.transaction.annotation.Transactional;
import java.sql.Timestamp;
import java.time.Instant;
import java.util.List;
import java.util.Optional;

@Repository
@ConditionalOnProperty(prefix = "sparrowshell.platform", name = "persistence-enabled", havingValue = "true")
class JdbcBackgroundJobRepository implements BackgroundJobRepository {
    private static final String COLUMNS = "job_id,job_type,payload,dedupe_key,status,available_at,lease_owner,lease_until,attempts,max_attempts,last_error_code,created_at,completed_at";
    private static final RowMapper<BackgroundJob> MAPPER = (rs, row) -> {
        BackgroundJob job = new BackgroundJob();
        job.jobId = rs.getString("job_id"); job.jobType = rs.getString("job_type"); job.payload = rs.getString("payload");
        job.dedupeKey = rs.getString("dedupe_key"); job.status = rs.getString("status");
        job.availableAt = instant(rs.getTimestamp("available_at")); job.leaseOwner = rs.getString("lease_owner");
        job.leaseUntil = instant(rs.getTimestamp("lease_until")); job.attempts = rs.getInt("attempts");
        job.maxAttempts = rs.getInt("max_attempts"); job.lastErrorCode = rs.getString("last_error_code");
        job.createdAt = instant(rs.getTimestamp("created_at")); job.completedAt = instant(rs.getTimestamp("completed_at"));
        return job;
    };
    private final JdbcTemplate jdbc;
    JdbcBackgroundJobRepository(JdbcTemplate jdbc) { this.jdbc = jdbc; }

    @Override public BackgroundJob enqueue(BackgroundJob job) {
        try {
            jdbc.update("INSERT INTO app_background_job(job_id,job_type,payload,dedupe_key,status,available_at,attempts,max_attempts,created_at) VALUES(?,?,?,?,?,?,?,?,?)",
                    job.jobId, job.jobType, job.payload, job.dedupeKey, job.status, timestamp(job.availableAt),
                    job.attempts, job.maxAttempts, timestamp(job.createdAt));
            return required(job.jobId);
        } catch (DuplicateKeyException exception) {
            BackgroundJob existing = byDedupe(job.jobType, job.dedupeKey);
            if (!existing.payload.equals(job.payload)) throw new IllegalArgumentException("dedupe key reused for a different payload");
            return existing;
        }
    }

    @Override @Transactional(readOnly = true)
    public List<String> candidateIds(Instant now, int limit) {
        return jdbc.queryForList("SELECT job_id FROM app_background_job WHERE (status IN('pending','retry') AND available_at<=?) OR (status='running' AND lease_until<=?) ORDER BY created_at,job_id LIMIT ?",
                String.class, timestamp(now), timestamp(now), Math.max(1, Math.min(100, limit)));
    }

    @Override @Transactional
    public Optional<BackgroundJob> claim(String jobId, String leaseOwner, Instant now, Instant leaseUntil) {
        int changed = jdbc.update("UPDATE app_background_job SET status='running',lease_owner=?,lease_until=?,attempts=attempts+1,last_error_code=NULL,completed_at=NULL WHERE job_id=? AND ((status IN('pending','retry') AND available_at<=?) OR (status='running' AND lease_until<=?))",
                leaseOwner, timestamp(leaseUntil), jobId, timestamp(now), timestamp(now));
        return changed == 1 ? find(jobId) : Optional.empty();
    }

    @Override public boolean complete(String jobId, String leaseOwner, Instant completedAt) {
        return jdbc.update("UPDATE app_background_job SET status='completed',completed_at=?,lease_owner=NULL,lease_until=NULL WHERE job_id=? AND status='running' AND lease_owner=?",
                timestamp(completedAt), jobId, leaseOwner) == 1;
    }

    @Override public boolean fail(String jobId, String leaseOwner, String errorCode, String nextStatus, Instant availableAt) {
        if (!"retry".equals(nextStatus) && !"dead_letter".equals(nextStatus)) throw new IllegalArgumentException("invalid failure status");
        return jdbc.update("UPDATE app_background_job SET status=?,available_at=?,last_error_code=?,lease_owner=NULL,lease_until=NULL,completed_at=CASE WHEN ?='dead_letter' THEN ? ELSE NULL END WHERE job_id=? AND status='running' AND lease_owner=?",
                nextStatus, timestamp(availableAt), errorCode, nextStatus, timestamp(Instant.now()), jobId, leaseOwner) == 1;
    }

    @Override @Transactional(readOnly = true)
    public Optional<BackgroundJob> find(String jobId) {
        try { return Optional.of(jdbc.queryForObject("SELECT " + COLUMNS + " FROM app_background_job WHERE job_id=?", MAPPER, jobId)); }
        catch (EmptyResultDataAccessException exception) { return Optional.empty(); }
    }

    private BackgroundJob byDedupe(String type, String dedupe) {
        try { return jdbc.queryForObject("SELECT " + COLUMNS + " FROM app_background_job WHERE job_type=? AND dedupe_key=?", MAPPER, type, dedupe); }
        catch (EmptyResultDataAccessException exception) { throw new IllegalStateException("deduplicated job disappeared"); }
    }
    private BackgroundJob required(String id) { return find(id).orElseThrow(() -> new IllegalStateException("job insert did not persist")); }
    private static Timestamp timestamp(Instant value) { return value == null ? null : Timestamp.from(value); }
    private static Instant instant(Timestamp value) { return value == null ? null : value.toInstant(); }
}
