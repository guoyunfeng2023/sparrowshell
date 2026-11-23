package com.sparrowshell.ai.platform.update;

import org.springframework.stereotype.Service;
import java.net.URI;
import java.time.Instant;
import java.util.*;

@Service
public class UpdateService {
    public static final String TARGET = "darwin-aarch64";
    public static final String WINDOWS_TARGET = "windows-x86_64";
    private static final List<String> SUPPORTED_TARGETS = Arrays.asList(TARGET, WINDOWS_TARGET);
    private final UpdateRepository repository;

    public UpdateService(UpdateRepository repository) {
        this.repository = repository;
    }

    public List<UpdateRelease> list() { return repository.list(); }

    public UpdateRelease createDraft(UpdateRelease requested) {
        validateDraft(requested);
        String target = normalizeTarget(requested.target);
        UpdateRelease release = requested.copy();
        release.id = UUID.randomUUID().toString();
        release.version = requested.version.trim();
        release.artifactUrl = requested.artifactUrl.trim();
        release.updatePolicy = requested.updatePolicy.trim().toLowerCase(Locale.ROOT);
        release.releaseNotes = requested.releaseNotes == null ? "" : requested.releaseNotes.trim();
        release.minimumMacOs = TARGET.equals(target) ? "13.0.0" : "";
        release.artifactBytes = 0;
        release.sha256 = "";
        release.signature = "";
        release.channel = "stable";
        release.target = target;
        release.state = "draft";
        release.rolloutPercent = 0;
        release.publishedAt = null;
        release.createdAt = Instant.now();
        release.updatedAt = release.createdAt;
        return repository.insert(release);
    }

    public UpdateRelease publish(String id) {
        UpdateRelease current = required(id);
        if (!"draft".equals(current.state) && !"canary".equals(current.state)) throw invalidState();
        return repository.transition(id, current.state, "stable", 100);
    }
    public UpdateRelease pause(String id) {
        UpdateRelease current = required(id);
        if (!"stable".equals(current.state)) throw invalidState();
        return repository.transition(id, "stable", "paused", 100);
    }
    public UpdateRelease resume(String id) {
        UpdateRelease current = required(id);
        if (!"paused".equals(current.state)) throw invalidState();
        return repository.transition(id, "paused", "stable", 100);
    }
    public UpdateRelease withdraw(String id) {
        UpdateRelease current = required(id);
        if (!Arrays.asList("canary", "stable", "paused").contains(current.state)) throw invalidState();
        return repository.transition(id, current.state, "withdrawn", current.rolloutPercent);
    }

    public Map<String, Object> check(String currentVersion, String channel, String deviceId, String target) {
        return check(currentVersion, channel, deviceId, target, false);
    }

    public Map<String, Object> check(String currentVersion, String channel, String deviceId, String target, boolean includeSilent) {
        String normalizedTarget = normalizeTarget(target);
        if (!"stable".equals(channel) && !"canary".equals(channel)) throw new UpdateException("invalid_channel", "更新渠道无效");
        compareVersions(currentVersion, "0.0.0");
        UUID.fromString(deviceId);
        List<UpdateRelease> candidates = new ArrayList<UpdateRelease>();
        for (UpdateRelease release : repository.visible(channel, normalizedTarget)) {
            if ("stable".equals(channel) && !"stable".equals(release.state)) continue;
            if (compareVersions(release.version, currentVersion) > 0) candidates.add(release);
        }
        candidates.sort((left, right) -> compareVersions(right.version, left.version));
        Map<String, Object> response = new LinkedHashMap<String, Object>();
        if (candidates.isEmpty()) {
            response.put("status", "no_update");
            return response;
        }
        UpdateRelease selected = candidates.get(0);
        if (!includeSilent && "silent".equals(selected.updatePolicy)) {
            response.put("status", "no_update");
            return response;
        }
        response.put("status", "update_available");
        response.put("version", selected.version);
        response.put("minimumMacOs", selected.minimumMacOs);
        response.put("target", selected.target);
        response.put("channel", selected.channel);
        response.put("updatePolicy", selected.updatePolicy);
        response.put("releaseNotes", selected.releaseNotes);
        response.put("artifactUrl", selected.artifactUrl);
        response.put("artifactBytes", selected.artifactBytes);
        response.put("sha256", selected.sha256);
        response.put("signature", selected.signature);
        response.put("publishedAt", selected.publishedAt);
        return response;
    }

    private static String normalizeTarget(String target) {
        String normalized = target == null || target.trim().isEmpty()
                ? TARGET : target.trim().toLowerCase(Locale.ROOT);
        if (!SUPPORTED_TARGETS.contains(normalized))
            throw new UpdateException("unsupported_target", "仅支持 Mac Apple Silicon 和 Windows x64 客户端");
        return normalized;
    }

    private UpdateRelease required(String id) {
        return repository.find(id).orElseThrow(() -> new UpdateException("not_found", "更新版本不存在"));
    }

    private void validateDraft(UpdateRelease release) {
        if (release == null) throw new UpdateException("invalid_artifact", "版本草稿不能为空");
        compareVersions(release.version == null ? null : release.version.trim(), "0.0.0");
        String policy = release.updatePolicy == null ? "" : release.updatePolicy.trim().toLowerCase(Locale.ROOT);
        if (!Arrays.asList("force", "optional", "silent").contains(policy))
            throw new UpdateException("invalid_update_policy", "更新策略仅支持 force、optional 或 silent");
        if (release.artifactUrl == null || release.artifactUrl.trim().isEmpty() || release.artifactUrl.trim().length() > 2048)
            throw new UpdateException("invalid_artifact", "制品 URL 不能为空且不能超过 2048 个字符");
        if (release.releaseNotes != null && release.releaseNotes.trim().length() > 10000)
            throw new UpdateException("invalid_release_notes", "更新内容不能超过 10000 个字符");
        try {
            URI url = URI.create(release.artifactUrl.trim());
            if (!"https".equals(url.getScheme()) || url.getHost() == null || url.getUserInfo() != null)
                throw new UpdateException("invalid_artifact", "制品 URL 必须是合法的 HTTPS 地址");
        } catch (IllegalArgumentException exception) {
            throw new UpdateException("invalid_artifact", "制品 URL 无效");
        }
    }

    static int compareVersions(String left, String right) {
        int[] a = parseVersion(left), b = parseVersion(right);
        for (int index = 0; index < 3; index++) if (a[index] != b[index]) return Integer.compare(a[index], b[index]);
        return 0;
    }

    private static int[] parseVersion(String value) {
        if (value == null || !value.matches("[0-9]+\\.[0-9]+\\.[0-9]+")) throw new UpdateException("invalid_version", "版本必须使用 x.y.z 格式");
        String[] parts = value.split("\\.");
        int[] result = new int[3];
        for (int index = 0; index < 3; index++) {
            try { result[index] = Integer.parseInt(parts[index]); }
            catch (NumberFormatException exception) { throw new UpdateException("invalid_version", "版本号超出范围"); }
        }
        return result;
    }

    private UpdateException invalidState() { return new UpdateException("invalid_state", "当前发布状态不允许此操作"); }
}
