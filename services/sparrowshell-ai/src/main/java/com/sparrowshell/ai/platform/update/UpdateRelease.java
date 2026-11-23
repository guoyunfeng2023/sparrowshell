package com.sparrowshell.ai.platform.update;

import java.time.Instant;

public class UpdateRelease {
    public String id;
    public String version;
    public String minimumMacOs;
    public String artifactUrl;
    public long artifactBytes;
    public String sha256;
    public String signature;
    public String channel;
    public String updatePolicy;
    public String releaseNotes;
    public String target;
    public String state;
    public int rolloutPercent;
    public Instant publishedAt;
    public Instant createdAt;
    public Instant updatedAt;

    public UpdateRelease copy() {
        UpdateRelease copy = new UpdateRelease();
        copy.id = id; copy.version = version; copy.minimumMacOs = minimumMacOs;
        copy.artifactUrl = artifactUrl; copy.artifactBytes = artifactBytes; copy.sha256 = sha256;
        copy.signature = signature; copy.channel = channel; copy.updatePolicy = updatePolicy; copy.releaseNotes = releaseNotes; copy.target = target; copy.state = state;
        copy.rolloutPercent = rolloutPercent; copy.publishedAt = publishedAt;
        copy.createdAt = createdAt; copy.updatedAt = updatedAt;
        return copy;
    }
}
