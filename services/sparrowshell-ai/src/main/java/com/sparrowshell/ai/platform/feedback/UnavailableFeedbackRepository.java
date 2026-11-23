package com.sparrowshell.ai.platform.feedback;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Repository;
import java.util.List;
import java.util.Optional;

@Repository
@ConditionalOnProperty(prefix = "sparrowshell.platform", name = "persistence-enabled", havingValue = "false", matchIfMissing = true)
class UnavailableFeedbackRepository implements FeedbackRepository {
    private FeedbackException unavailable() { return new FeedbackException("unavailable", "反馈存储尚未配置"); }
    @Override public void insert(FeedbackModels.Feedback feedback) { throw unavailable(); }
    @Override public List<FeedbackModels.Feedback> list(String status, int limit, int offset) { throw unavailable(); }
    @Override public long count(String status) { throw unavailable(); }
    @Override public Optional<FeedbackModels.Feedback> find(String id) { throw unavailable(); }
    @Override public FeedbackModels.Feedback updateStatus(String id, String status, String updatedBy) { throw unavailable(); }
}
