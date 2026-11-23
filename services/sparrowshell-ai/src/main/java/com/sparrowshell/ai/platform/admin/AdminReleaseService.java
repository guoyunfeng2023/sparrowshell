package com.sparrowshell.ai.platform.admin;

import com.sparrowshell.ai.platform.update.UpdateRelease;
import com.sparrowshell.ai.platform.update.UpdateService;
import org.springframework.stereotype.Service;
import java.util.List;

@Service
class AdminReleaseService {
    private final UpdateService updates;
    private final AdminAuthService auth;
    private final AdminService operations;
    AdminReleaseService(UpdateService updates, AdminAuthService auth, AdminService operations) { this.updates = updates; this.auth = auth; this.operations = operations; }

    List<UpdateRelease> list(AdminModels.AdminSession session) {
        auth.requireRole(session, "SUPER_ADMIN", "RELEASE_ADMIN", "VIEWER"); return updates.list();
    }
    AdminService.ActionResult create(AdminModels.AdminSession session, UpdateRelease release) {
        auth.requireRole(session, "SUPER_ADMIN", "RELEASE_ADMIN");
        String target = release == null ? "unknown" : String.valueOf(release.version);
        AdminService.DangerousRequest operation = new AdminService.DangerousRequest();
        operation.reason = "创建版本草稿，等待发布审核";
        operation.operationId = java.util.UUID.randomUUID().toString();
        return operations.executeRecorded(session, "release.create", "release", target, operation, () -> {
            UpdateRelease created = updates.createDraft(release);
            AdminService.ActionResult result = new AdminService.ActionResult(); result.release = created; return result;
        });
    }
    AdminService.ActionResult transition(AdminModels.AdminSession session, String id, String action, Integer ignoredRollout, AdminService.DangerousRequest request) {
        auth.requireRole(session, "SUPER_ADMIN", "RELEASE_ADMIN");
        if (!java.util.Arrays.asList("stable", "pause", "resume", "withdraw").contains(action)) throw new AdminException("invalid_release_action", "发布操作无效");
        return operations.executeRecorded(session, "release." + action, "release", id, request, () -> {
            UpdateRelease release;
            if ("stable".equals(action)) release = updates.publish(id);
            else if ("pause".equals(action)) release = updates.pause(id);
            else if ("resume".equals(action)) release = updates.resume(id);
            else release = updates.withdraw(id);
            AdminService.ActionResult result = new AdminService.ActionResult(); result.release = release; return result;
        });
    }
}
