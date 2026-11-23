package com.sparrowshell.ai.platform.update;

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
class JdbcUpdateRepository implements UpdateRepository {
    private static final String COLUMNS = "id,version,minimum_macos,artifact_url,artifact_bytes,sha256,signature,channel,update_policy,release_notes,target,state,rollout_percent,published_at,created_at,updated_at";
    private static final RowMapper<UpdateRelease> MAPPER = (rs, row) -> {
        UpdateRelease release = new UpdateRelease();
        release.id = rs.getString("id"); release.version = rs.getString("version");
        release.minimumMacOs = rs.getString("minimum_macos");
        release.artifactUrl = rs.getString("artifact_url"); release.artifactBytes = rs.getLong("artifact_bytes");
        release.sha256 = rs.getString("sha256"); release.signature = rs.getString("signature");
        release.channel = rs.getString("channel"); release.updatePolicy = rs.getString("update_policy"); release.releaseNotes = rs.getString("release_notes"); release.target = rs.getString("target");
        release.state = rs.getString("state"); release.rolloutPercent = rs.getInt("rollout_percent");
        release.publishedAt = instant(rs.getTimestamp("published_at"));
        release.createdAt = instant(rs.getTimestamp("created_at")); release.updatedAt = instant(rs.getTimestamp("updated_at"));
        return release;
    };
    private final JdbcTemplate jdbc;
    JdbcUpdateRepository(JdbcTemplate jdbc) { this.jdbc = jdbc; }

    @Override public UpdateRelease insert(UpdateRelease release) {
        try {
            jdbc.update("INSERT INTO app_update_release(id,version,minimum_macos,artifact_url,artifact_bytes,sha256,signature,channel,update_policy,release_notes,target,state,rollout_percent,published_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                    release.id, release.version, release.minimumMacOs, release.artifactUrl, release.artifactBytes,
                    release.sha256, release.signature, release.channel, release.updatePolicy, release.releaseNotes, release.target, release.state,
                    release.rolloutPercent, timestamp(release.publishedAt), timestamp(release.createdAt), timestamp(release.updatedAt));
        } catch (DuplicateKeyException exception) {
            throw new UpdateException("immutable_artifact", "相同版本和渠道的制品已存在，不能覆盖");
        }
        return required(release.id);
    }

    @Override @Transactional(readOnly = true)
    public Optional<UpdateRelease> find(String id) {
        try { return Optional.of(jdbc.queryForObject("SELECT " + COLUMNS + " FROM app_update_release WHERE id=?", MAPPER, id)); }
        catch (EmptyResultDataAccessException exception) { return Optional.empty(); }
    }

    @Override @Transactional(readOnly = true)
    public List<UpdateRelease> visible(String channel, String target) {
        return jdbc.query("SELECT " + COLUMNS + " FROM app_update_release WHERE channel=? AND state IN('canary','stable') AND target=?", MAPPER, channel, target);
    }

    @Override @Transactional(readOnly = true)
    public List<UpdateRelease> list() {
        return jdbc.query("SELECT " + COLUMNS + " FROM app_update_release ORDER BY created_at DESC LIMIT 200", MAPPER);
    }

    @Override @Transactional
    public UpdateRelease transition(String id, String expectedState, String nextState, int rolloutPercent) {
        UpdateRelease current = forUpdate(id);
        if (!expectedState.equals(current.state)) throw new UpdateException("invalid_state", "发布状态已变化，请刷新后重试");
        Instant published = current.publishedAt == null ? Instant.now() : current.publishedAt;
        int changed = jdbc.update("UPDATE app_update_release SET state=?,rollout_percent=?,published_at=?,updated_at=? WHERE id=? AND state=?",
                nextState, rolloutPercent, timestamp(published), timestamp(Instant.now()), id, expectedState);
        if (changed != 1) throw new UpdateException("invalid_state", "发布状态已变化，请刷新后重试");
        return required(id);
    }

    private UpdateRelease forUpdate(String id) {
        try { return jdbc.queryForObject("SELECT " + COLUMNS + " FROM app_update_release WHERE id=? FOR UPDATE", MAPPER, id); }
        catch (EmptyResultDataAccessException exception) { throw new UpdateException("not_found", "更新版本不存在"); }
    }
    private UpdateRelease required(String id) { return find(id).orElseThrow(() -> new UpdateException("not_found", "更新版本不存在")); }
    private static Timestamp timestamp(Instant value) { return value == null ? null : Timestamp.from(value); }
    private static Instant instant(Timestamp value) { return value == null ? null : value.toInstant(); }
}
