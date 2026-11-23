package com.sparrowshell.ai.platform.feedback;

import java.util.List;
import java.util.Optional;

interface FeedbackRepository {
    void insert(FeedbackModels.Feedback feedback);
    List<FeedbackModels.Feedback> list(String status, int limit, int offset);
    long count(String status);
    Optional<FeedbackModels.Feedback> find(String id);
    FeedbackModels.Feedback updateStatus(String id, String status, String updatedBy);
}
