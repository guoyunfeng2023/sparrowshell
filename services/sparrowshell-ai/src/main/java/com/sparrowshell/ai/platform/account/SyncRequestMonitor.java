package com.sparrowshell.ai.platform.account;

import org.springframework.beans.factory.ObjectProvider;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.stereotype.Component;
import java.sql.Timestamp;
import java.time.Instant;
import java.util.UUID;
import java.util.concurrent.TimeUnit;

/** Best-effort protocol metadata only. Observability never changes sync correctness. */
@Component
class SyncRequestMonitor {
    private final JdbcTemplate jdbc;
    SyncRequestMonitor(ObjectProvider<JdbcTemplate> jdbc) { this.jdbc = jdbc.getIfAvailable(); }

    String start() { return UUID.randomUUID().toString(); }

    void success(String requestId, AccountModels.DeviceSession session, String operation, long startedNanos,
                 Long revision, Long changeSeq, int conflicts, int batchSize) {
        write(requestId, session, operation, "succeeded", null, 200, revision, changeSeq,
                conflicts, false, batchSize, elapsedMillis(startedNanos));
    }

    void failure(String requestId, AccountModels.DeviceSession session, String operation, long startedNanos,
                 String errorCode, int httpStatus, int batchSize) {
        write(requestId, session, operation, "failed", errorCode, httpStatus, null, null,
                0, "snapshot_pagination_required".equals(errorCode), batchSize, elapsedMillis(startedNanos));
    }

    private void write(String requestId, AccountModels.DeviceSession session, String operation, String status,
                       String errorCode, int httpStatus, Long revision, Long changeSeq, int conflicts,
                       boolean cursorExpired, int batchSize, long durationMs) {
        if (jdbc == null) return;
        try {
            jdbc.update("INSERT INTO app_sync_request(request_id,user_id,device_id,operation,status,error_code,http_status,revision,change_seq,conflict_count,cursor_expired,batch_size,duration_ms,client_version,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                    requestId, session == null ? null : session.accountId, session == null ? null : session.deviceId,
                    operation, status, errorCode, httpStatus, revision, changeSeq, conflicts, cursorExpired,
                    Math.max(0, batchSize), durationMs, session == null ? null : session.clientVersion, Timestamp.from(Instant.now()));
        } catch (RuntimeException ignored) {
            // Monitoring storage failure must not turn an acknowledged sync request into a retry.
        }
    }

    private static long elapsedMillis(long startedNanos) {
        return Math.max(0L, TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - startedNanos));
    }
}
