package com.sparrowshell.ai.platform.job;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.stereotype.Repository;
import org.springframework.transaction.annotation.Transactional;
import java.sql.Timestamp;
import java.time.Instant;

@Repository
@ConditionalOnProperty(prefix = "sparrowshell.platform", name = "persistence-enabled", havingValue = "true")
class JdbcAccountDeletionOperations implements AccountDeletionOperations {
    private final JdbcTemplate jdbc;
    JdbcAccountDeletionOperations(JdbcTemplate jdbc) { this.jdbc = jdbc; }

    @Override @Transactional
    public void executeStep(String accountId, String step) {
        int inserted = jdbc.update("INSERT IGNORE INTO app_account_deletion_step(account_id,step,completed_at) VALUES(?,?,?)",
                accountId, step, Timestamp.from(Instant.now()));
        if (inserted == 0) return;
        switch (step) {
            case "revoke_sessions":
                jdbc.update("UPDATE app_device_session SET revoked_at=COALESCE(revoked_at,?) WHERE account_id=?", Timestamp.from(Instant.now()), accountId);
                jdbc.update("UPDATE app_refresh_token SET revoked_at=COALESCE(revoked_at,?) WHERE session_id IN (SELECT session_id FROM app_device_session WHERE account_id=?)", Timestamp.from(Instant.now()), accountId);
                break;
            case "delete_sync_data":
                jdbc.update("DELETE FROM app_sync_receipt WHERE user_id=?", accountId);
                jdbc.update("DELETE FROM app_sync_entity WHERE user_id=?", accountId);
                break;
            case "delete_ai_data":
                jdbc.update("DELETE FROM ai_usage_ledger WHERE account_id=?", accountId);
                jdbc.update("DELETE FROM ai_account_quota WHERE account_id=?", accountId);
                break;
            case "anonymize_account":
                int changed = jdbc.update("UPDATE app_account SET status='deleted',email=?,nickname=NULL,password_hash=NULL,password_salt=NULL,deleted_at=COALESCE(deleted_at,?),revision=revision+1 WHERE account_id=?",
                        "deleted-" + accountId + "@deleted.invalid", Timestamp.from(Instant.now()), accountId);
                if (changed != 1) throw new IllegalStateException("account deletion target missing");
                break;
            default: throw new IllegalArgumentException("unknown account deletion step");
        }
    }
}
