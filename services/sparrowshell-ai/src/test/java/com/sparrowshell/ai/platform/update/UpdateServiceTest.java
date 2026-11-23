package com.sparrowshell.ai.platform.update;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import java.time.Instant;
import java.util.*;
import java.util.concurrent.ConcurrentHashMap;
import static org.junit.jupiter.api.Assertions.*;

class UpdateServiceTest {
    private static final String DEVICE = "0be0dd37-9379-4663-a94f-799f16ab7678";
    private FakeRepository repository;
    private UpdateService service;

    @BeforeEach void setUp() {
        repository = new FakeRepository();
        service = new UpdateService(repository);
    }

    @Test void acceptsSupportedTargetsAndRejectsInvalidArtifacts() {
        UpdateRelease draft = draft("1.2.3");
        service.createDraft(draft);
        UpdateException target = assertThrows(UpdateException.class,
                () -> service.check("1.0.0", "stable", DEVICE, "x86_64-apple-darwin"));
        assertEquals("unsupported_target", target.code);
        UpdateRelease insecure = draft("1.2.4");
        insecure.artifactUrl = "http://downloads.example.com/SparrowShell-1.2.4.dmg";
        assertEquals("invalid_artifact", assertThrows(UpdateException.class, () -> service.createDraft(insecure)).code);
        UpdateRelease invalidTarget = draft("1.2.5");
        invalidTarget.target = "linux-x86_64";
        assertEquals("unsupported_target", assertThrows(UpdateException.class, () -> service.createDraft(invalidTarget)).code);
    }

    @Test void draftRequiresOnlyThreeFieldsAndUsesServerDefaults() {
        UpdateRelease requested = draft("1.2.3");
        requested.updatePolicy = " FORCE ";
        requested.minimumMacOs = "99.0.0";
        requested.channel = "canary";
        requested.artifactBytes = 123456;
        requested.sha256 = "untrusted";
        requested.signature = "untrusted";
        requested.releaseNotes = "  新增批量执行\n修复已知问题  ";
        UpdateRelease created = service.createDraft(requested);
        assertEquals(UpdateService.TARGET, created.target);
        assertEquals("13.0.0", created.minimumMacOs);
        assertEquals("stable", created.channel);
        assertEquals("force", created.updatePolicy);
        assertEquals("新增批量执行\n修复已知问题", created.releaseNotes);
        assertEquals(0, created.artifactBytes);
        assertEquals("", created.sha256);
        assertEquals("", created.signature);
        assertEquals("no_update", service.check("1.0.0", "stable", DEVICE, UpdateService.TARGET).get("status"));
        service.publish(created.id);
        Map<String, Object> result = service.check("1.0.0", "stable", DEVICE, UpdateService.TARGET);
        assertEquals("force", result.get("updatePolicy"));
        assertEquals("新增批量执行\n修复已知问题", result.get("releaseNotes"));
    }

    @Test void macAndWindowsReleasesAreIndependentAndMayShareVersion() {
        UpdateRelease mac = service.createDraft(draft("2.0.0"));
        UpdateRelease windowsRequest = draft("2.0.0");
        windowsRequest.target = " WINDOWS-X86_64 ";
        windowsRequest.artifactUrl = "https://downloads.example.com/SparrowShell-2.0.0.exe";
        UpdateRelease windows = service.createDraft(windowsRequest);
        assertEquals(UpdateService.WINDOWS_TARGET, windows.target);
        assertEquals("", windows.minimumMacOs);
        service.publish(mac.id);
        service.publish(windows.id);

        Map<String, Object> macResult = service.check("1.0.0", "stable", DEVICE, UpdateService.TARGET);
        assertEquals("update_available", macResult.get("status"));
        assertEquals("2.0.0", macResult.get("version"));
        assertEquals(UpdateService.TARGET, macResult.get("target"));
        assertTrue(macResult.get("artifactUrl").toString().endsWith(".dmg"));

        Map<String, Object> windowsResult = service.check("1.0.0", "stable", DEVICE, UpdateService.WINDOWS_TARGET);
        assertEquals("update_available", windowsResult.get("status"));
        assertEquals("2.0.0", windowsResult.get("version"));
        assertEquals(UpdateService.WINDOWS_TARGET, windowsResult.get("target"));
        assertTrue(windowsResult.get("artifactUrl").toString().endsWith(".exe"));
    }

    @Test void releaseForOnePlatformIsNeverOfferedToTheOther() {
        UpdateRelease windowsRequest = draft("4.0.0");
        windowsRequest.target = UpdateService.WINDOWS_TARGET;
        UpdateRelease windows = service.createDraft(windowsRequest);
        service.publish(windows.id);
        assertEquals("no_update", service.check("1.0.0", "stable", DEVICE, UpdateService.TARGET).get("status"));
        assertEquals("update_available", service.check("1.0.0", "stable", DEVICE, UpdateService.WINDOWS_TARGET).get("status"));

        UpdateRelease mac = service.createDraft(draft("5.0.0"));
        service.publish(mac.id);
        assertEquals("5.0.0", service.check("1.0.0", "stable", DEVICE, UpdateService.TARGET).get("version"));
        assertEquals("4.0.0", service.check("1.0.0", "stable", DEVICE, UpdateService.WINDOWS_TARGET).get("version"));
    }

    @Test void immutableArtifactsAndValidStateTransitions() {
        UpdateRelease release = service.createDraft(draft("1.2.3"));
        assertThrows(UpdateException.class, () -> service.createDraft(draft("1.2.3")));
        assertThrows(UpdateException.class, () -> service.pause(release.id));
        UpdateRelease published = service.publish(release.id);
        assertEquals("stable", published.state);
        assertEquals(100, published.rolloutPercent);
        assertNotNull(published.publishedAt);
        assertThrows(UpdateException.class, () -> service.publish(release.id));
        assertEquals("paused", service.pause(release.id).state);
        assertThrows(UpdateException.class, () -> service.publish(release.id));
        assertEquals("stable", service.resume(release.id).state);
        assertEquals("withdrawn", service.withdraw(release.id).state);
        assertEquals(100, repository.releases.get(release.id).rolloutPercent);
        assertNotNull(repository.releases.get(release.id).publishedAt);
    }

    @Test void legacyCanaryReleaseCanBePublishedDirectly() {
        UpdateRelease release = service.createDraft(draft("2.0.0"));
        repository.releases.get(release.id).state = "canary";
        repository.releases.get(release.id).rolloutPercent = 20;
        assertEquals("no_update", service.check("1.0.0", "stable", DEVICE, UpdateService.TARGET).get("status"));
        UpdateRelease published = service.publish(release.id);
        assertEquals("stable", published.state);
        assertEquals(100, published.rolloutPercent);
        assertEquals("update_available", service.check("1.0.0", "stable", DEVICE, UpdateService.TARGET).get("status"));
    }

    @Test void pausedAndWithdrawnReleasesAreNeverOffered() {
        UpdateRelease paused = service.createDraft(draft("2.0.0"));
        service.publish(paused.id);
        service.pause(paused.id);
        UpdateRelease withdrawn = service.createDraft(draft("1.9.0"));
        service.publish(withdrawn.id);
        service.withdraw(withdrawn.id);
        assertEquals("no_update", service.check("1.0.0", "stable", DEVICE, UpdateService.TARGET).get("status"));
    }

    @Test void publishedReleaseIsVisibleToEveryDevice() {
        UpdateRelease release = service.createDraft(draft("2.0.0"));
        service.publish(release.id);
        List<String> devices = Arrays.asList(
                "00000000-0000-0000-0000-000000000001",
                "0be0dd37-9379-4663-a94f-799f16ab7678",
                "ffffffff-ffff-ffff-ffff-ffffffffffff");
        for (String device : devices) {
            Map<String, Object> result = service.check("1.0.0", "stable", device, UpdateService.TARGET);
            assertEquals("update_available", result.get("status"));
            assertEquals("2.0.0", result.get("version"));
        }
    }

    @Test void stableChannelSelectsOnlyNewerStableRelease() {
        service.createDraft(draft("3.0.0"));
        assertEquals("no_update", service.check("1.0.0", "stable", DEVICE, UpdateService.TARGET).get("status"));
        UpdateRelease stable = service.createDraft(draft("2.0.0"));
        service.publish(stable.id);
        Map<String, Object> result = service.check("1.0.0", "stable", DEVICE, UpdateService.TARGET);
        assertEquals("update_available", result.get("status"));
        assertEquals("2.0.0", result.get("version"));
        assertEquals("optional", result.get("updatePolicy"));
        assertEquals("no_update", service.check("2.0.0", "stable", DEVICE, UpdateService.TARGET).get("status"));
    }

    @Test void latestSilentReleaseSuppressesUpdatePrompt() {
        UpdateRelease silent = service.createDraft(draft("3.0.0"));
        repository.releases.get(silent.id).updatePolicy = "silent";
        service.publish(silent.id);
        UpdateRelease optional = service.createDraft(draft("2.0.0"));
        service.publish(optional.id);
        Map<String, Object> result = service.check("1.0.0", "stable", DEVICE, UpdateService.TARGET);
        assertEquals("no_update", result.get("status"));
        assertFalse(result.containsKey("version"));

        Map<String, Object> manual = service.check("1.0.0", "stable", DEVICE, UpdateService.TARGET, true);
        assertEquals("update_available", manual.get("status"));
        assertEquals("3.0.0", manual.get("version"));
        assertEquals("silent", manual.get("updatePolicy"));
        assertEquals("", manual.get("releaseNotes"));
    }

    @Test void rejectsMalformedIdentityVersionAndPolicy() {
        service.createDraft(draft("1.2.3"));
        assertThrows(IllegalArgumentException.class, () -> service.check("1.0.0", "stable", "not-a-uuid", UpdateService.TARGET));
        assertThrows(UpdateException.class, () -> service.check("v1", "stable", DEVICE, UpdateService.TARGET));
        UpdateRelease nullUrl = draft("1.2.4");
        nullUrl.artifactUrl = null;
        assertEquals("invalid_artifact", assertThrows(UpdateException.class, () -> service.createDraft(nullUrl)).code);
        UpdateRelease invalidPolicy = draft("1.2.5");
        invalidPolicy.updatePolicy = "automatic";
        assertEquals("invalid_update_policy", assertThrows(UpdateException.class, () -> service.createDraft(invalidPolicy)).code);
    }

    private static UpdateRelease draft(String version) {
        UpdateRelease release = new UpdateRelease();
        release.version = version;
        release.artifactUrl = "https://downloads.example.com/SparrowShell-" + version + ".dmg";
        release.updatePolicy = "optional";
        return release;
    }

    private static class FakeRepository implements UpdateRepository {
        final Map<String, UpdateRelease> releases = new ConcurrentHashMap<String, UpdateRelease>();
        @Override public synchronized UpdateRelease insert(UpdateRelease release) {
            for (UpdateRelease existing : releases.values()) {
                if (existing.version.equals(release.version) && existing.channel.equals(release.channel)
                        && existing.target.equals(release.target))
                    throw new UpdateException("immutable_artifact", "duplicate");
            }
            releases.put(release.id, release.copy());
            return release.copy();
        }
        @Override public Optional<UpdateRelease> find(String id) {
            UpdateRelease value = releases.get(id);
            return value == null ? Optional.empty() : Optional.of(value.copy());
        }
        @Override public List<UpdateRelease> visible(String channel, String target) {
            List<UpdateRelease> result = new ArrayList<UpdateRelease>();
            for (UpdateRelease value : releases.values()) {
                if (value.channel.equals(channel) && target.equals(value.target)
                        && ("canary".equals(value.state) || "stable".equals(value.state))) result.add(value.copy());
            }
            return result;
        }
        @Override public synchronized UpdateRelease transition(String id, String expected, String next, int rollout) {
            UpdateRelease current = releases.get(id);
            if (current == null) throw new UpdateException("not_found", "missing");
            if (!expected.equals(current.state)) throw new UpdateException("invalid_state", "stale");
            current.state = next; current.rolloutPercent = rollout;
            if (current.publishedAt == null) current.publishedAt = Instant.now();
            current.updatedAt = Instant.now();
            return current.copy();
        }
    }
}
