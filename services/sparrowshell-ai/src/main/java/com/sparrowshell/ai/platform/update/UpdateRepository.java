package com.sparrowshell.ai.platform.update;

import java.util.List;
import java.util.Optional;

public interface UpdateRepository {
    UpdateRelease insert(UpdateRelease release);
    Optional<UpdateRelease> find(String id);
    List<UpdateRelease> visible(String channel, String target);
    default List<UpdateRelease> list() { throw new UpdateException("unavailable", "更新服务尚未配置持久化存储"); }
    UpdateRelease transition(String id, String expectedState, String nextState, int rolloutPercent);
}
