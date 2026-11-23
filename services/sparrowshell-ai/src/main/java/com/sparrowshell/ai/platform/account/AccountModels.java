package com.sparrowshell.ai.platform.account;

import com.fasterxml.jackson.annotation.JsonIgnore;
import com.fasterxml.jackson.annotation.JsonProperty;
import java.time.Instant;

final class AccountModels {
    private AccountModels() { }

    static final class AccountRecord {
        String id;
        String email;
        String nickname;
        String passwordHash;
        String passwordSalt;
        String status;
        Instant createdAt;
        Instant lastActiveAt;
    }

    static final class DeviceSession {
        String sessionId;
        String accountId;
        String deviceId;
        String deviceName;
        String clientVersion;
        String tokenHash;
        Instant createdAt;
        Instant expiresAt;
        Instant lastSeenAt;
    }

    static final class TokenResponse {
        @JsonProperty("user_id") String userId;
        @JsonProperty("device_id") String deviceId;
        @JsonProperty("access_token") String accessToken;
        @JsonProperty("refresh_token") String refreshToken;
        @JsonProperty("expires_at") String expiresAt;
        @JsonProperty("email") String email;
        @JsonProperty("nickname") String nickname;
        @JsonIgnore String sessionId;
    }

    static final class ProfileResponse {
        @JsonProperty("email") String email;
        @JsonProperty("nickname") String nickname;
    }

    static final class VerificationCode {
        String id;
        String email;
        String purpose;
        String codeHash;
        int attempts;
        Instant createdAt;
        Instant expiresAt;
        Instant usedAt;
    }

    static final class RefreshToken {
        String tokenHash;
        String sessionId;
        String accountId;
        String deviceId;
        String accountStatus;
        long generation;
        Instant expiresAt;
        Instant rotatedAt;
        Instant revokedAt;
        Instant sessionRevokedAt;
    }

    static final class SyncMutation {
        @JsonProperty("mutation_id") String mutationId;
        @JsonProperty("entity_type") String entityType;
        @JsonProperty("entity_id") String entityId;
        @JsonProperty("base_revision") long baseRevision;
        @JsonProperty("schema_version") int schemaVersion;
        @JsonProperty("deleted") boolean deleted;
        @JsonProperty("data") Object data;
    }

    static final class SyncChange {
        @JsonProperty("change_seq") long changeSeq;
        @JsonProperty("entity_type") String entityType;
        @JsonProperty("entity_id") String entityId;
        @JsonProperty("revision") long revision;
        @JsonProperty("deleted") boolean deleted;
        @JsonProperty("data") Object data;
    }

    static final class SyncPushResult {
        @JsonProperty("mutation_id") String mutationId;
        @JsonProperty("accepted") boolean accepted;
        @JsonProperty("revision") Long revision;
        @JsonProperty("change_seq") Long changeSeq;
        @JsonProperty("current") AccountModels.SyncChange current;
        @JsonProperty("error_code") String errorCode;
    }

    static final class SyncEntity {
        String userId;
        String entityType;
        String entityId;
        long revision;
        int schemaVersion;
        long changeSeq;
        Instant deletedAt;
        Instant updatedAt;
        String payload;
    }

    static final class SyncPullPage {
        @JsonProperty("changes") java.util.List<SyncChange> changes = new java.util.ArrayList<SyncChange>();
        @JsonProperty("next_cursor") long nextCursor;
        @JsonProperty("has_more") boolean hasMore;
        @JsonProperty("snapshot_required") boolean snapshotRequired;
    }
}
