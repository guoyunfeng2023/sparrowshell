package com.sparrowshell.ai.platform.job;

import java.time.Instant;

public class BackgroundJob {
    public String jobId;
    public String jobType;
    public String payload;
    public String dedupeKey;
    public String status;
    public Instant availableAt;
    public String leaseOwner;
    public Instant leaseUntil;
    public int attempts;
    public int maxAttempts;
    public String lastErrorCode;
    public Instant createdAt;
    public Instant completedAt;
}
