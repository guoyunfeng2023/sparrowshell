package com.sparrowshell.ai.platform.update;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Repository;
import java.util.List;
import java.util.Optional;

@Repository
@ConditionalOnProperty(prefix = "sparrowshell.platform", name = "persistence-enabled", havingValue = "false", matchIfMissing = true)
class UnavailableUpdateRepository implements UpdateRepository {
    private UpdateException unavailable() { return new UpdateException("unavailable", "更新服务尚未配置持久化存储"); }
    @Override public UpdateRelease insert(UpdateRelease release) { throw unavailable(); }
    @Override public Optional<UpdateRelease> find(String id) { throw unavailable(); }
    @Override public List<UpdateRelease> visible(String channel, String target) { throw unavailable(); }
    @Override public UpdateRelease transition(String id, String expectedState, String nextState, int rolloutPercent) { throw unavailable(); }
}
