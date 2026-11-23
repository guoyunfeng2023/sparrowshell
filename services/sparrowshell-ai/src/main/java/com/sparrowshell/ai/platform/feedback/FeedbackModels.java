package com.sparrowshell.ai.platform.feedback;

import com.fasterxml.jackson.annotation.JsonIgnore;
import java.time.Instant;

public final class FeedbackModels {
    private FeedbackModels() { }

    public static final class Feedback {
        public String id;
        public String userId;
        public String userEmail;
        public String userNickname;
        public String description;
        public String contact;
        public String status;
        public Instant createdAt;
        public Instant updatedAt;
        public String updatedBy;
        public Attachment attachment;
    }

    public static final class Attachment {
        public String originalName;
        @JsonIgnore public String storageKey;
        public String contentType;
        public long sizeBytes;
    }
}
