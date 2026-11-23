package com.sparrowshell.ai.platform.feedback;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.dao.DataAccessException;
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
class JdbcFeedbackRepository implements FeedbackRepository {
    private static final String SELECT = "SELECT f.id,f.user_id,f.description,f.contact,f.status,f.created_at,f.updated_at,f.updated_by,"
            + "u.email AS user_email,u.nickname AS user_nickname,a.original_name,a.storage_key,a.content_type,a.size_bytes "
            + "FROM app_feedback f LEFT JOIN app_account u ON u.account_id=f.user_id "
            + "LEFT JOIN app_feedback_attachment a ON a.feedback_id=f.id ";
    private static final RowMapper<FeedbackModels.Feedback> MAPPER = (rs, row) -> {
        FeedbackModels.Feedback value = new FeedbackModels.Feedback();
        value.id = rs.getString("id"); value.userId = rs.getString("user_id");
        value.userEmail = rs.getString("user_email"); value.userNickname = rs.getString("user_nickname");
        value.description = rs.getString("description"); value.contact = rs.getString("contact");
        value.status = rs.getString("status"); value.createdAt = instant(rs.getTimestamp("created_at"));
        value.updatedAt = instant(rs.getTimestamp("updated_at")); value.updatedBy = rs.getString("updated_by");
        if (rs.getString("storage_key") != null) {
            FeedbackModels.Attachment attachment = new FeedbackModels.Attachment();
            attachment.originalName = rs.getString("original_name"); attachment.storageKey = rs.getString("storage_key");
            attachment.contentType = rs.getString("content_type"); attachment.sizeBytes = rs.getLong("size_bytes");
            value.attachment = attachment;
        }
        return value;
    };
    private final JdbcTemplate jdbc;
    JdbcFeedbackRepository(JdbcTemplate jdbc) { this.jdbc = jdbc; }

    @Override @Transactional
    public void insert(FeedbackModels.Feedback feedback) {
        try {
            jdbc.update("INSERT INTO app_feedback(id,user_id,description,contact,status,created_at,updated_at) VALUES(?,?,?,?,?,?,?)",
                    feedback.id, feedback.userId, feedback.description, feedback.contact, feedback.status,
                    timestamp(feedback.createdAt), timestamp(feedback.updatedAt));
            if (feedback.attachment != null) {
                FeedbackModels.Attachment value = feedback.attachment;
                jdbc.update("INSERT INTO app_feedback_attachment(feedback_id,original_name,storage_key,content_type,size_bytes,created_at) VALUES(?,?,?,?,?,?)",
                        feedback.id, value.originalName, value.storageKey, value.contentType, value.sizeBytes, timestamp(feedback.createdAt));
            }
        } catch (DataAccessException exception) {
            throw new FeedbackException("unavailable", "反馈存储暂时不可用");
        }
    }

    @Override @Transactional(readOnly = true)
    public List<FeedbackModels.Feedback> list(String status, int limit, int offset) {
        try {
            if (status == null) return jdbc.query(SELECT + "ORDER BY f.created_at DESC LIMIT ? OFFSET ?", MAPPER, limit, offset);
            return jdbc.query(SELECT + "WHERE f.status=? ORDER BY f.created_at DESC LIMIT ? OFFSET ?", MAPPER, status, limit, offset);
        } catch (DataAccessException exception) { throw new FeedbackException("unavailable", "反馈存储暂时不可用"); }
    }

    @Override @Transactional(readOnly = true)
    public long count(String status) {
        try {
            Long count = status == null ? jdbc.queryForObject("SELECT COUNT(*) FROM app_feedback", Long.class)
                    : jdbc.queryForObject("SELECT COUNT(*) FROM app_feedback WHERE status=?", Long.class, status);
            return count == null ? 0 : count;
        } catch (DataAccessException exception) { throw new FeedbackException("unavailable", "反馈存储暂时不可用"); }
    }

    @Override @Transactional(readOnly = true)
    public Optional<FeedbackModels.Feedback> find(String id) {
        try { return Optional.of(jdbc.queryForObject(SELECT + "WHERE f.id=?", MAPPER, id)); }
        catch (EmptyResultDataAccessException exception) { return Optional.empty(); }
        catch (DataAccessException exception) { throw new FeedbackException("unavailable", "反馈存储暂时不可用"); }
    }

    @Override @Transactional
    public FeedbackModels.Feedback updateStatus(String id, String status, String updatedBy) {
        try {
            if (jdbc.update("UPDATE app_feedback SET status=?,updated_by=?,updated_at=? WHERE id=?", status, updatedBy, timestamp(Instant.now()), id) != 1)
                throw new FeedbackException("not_found", "反馈不存在");
            return find(id).orElseThrow(() -> new FeedbackException("not_found", "反馈不存在"));
        } catch (DataAccessException exception) { throw new FeedbackException("unavailable", "反馈存储暂时不可用"); }
    }

    private static Timestamp timestamp(Instant value) { return value == null ? null : Timestamp.from(value); }
    private static Instant instant(Timestamp value) { return value == null ? null : value.toInstant(); }
}
